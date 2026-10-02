use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::secretexchange::SecretExchange;
use crate::services::states::States;

const NAMES: [&str; 2] = [
    "org.gnome.keyring.SystemPrompter",
    "org.gnome.keyring.PrivatePrompter",
];
const PATH: &str = "/org/gnome/keyring/Prompter";
const INTERFACE: &str = "org.gnome.keyring.internal.Prompter";
const CALLBACK: &str = "org.gnome.keyring.internal.Prompter.Callback";
const FAILED: &str = "org.gnome.keyring.Prompter.Failed";
const IN_PROGRESS: &str = "org.gnome.keyring.Prompter.InProgress";
const SECRETS: &str = "org.freedesktop.secrets";

const INTROSPECTION: &str = r#"<node>
  <interface name="org.gnome.keyring.internal.Prompter">
    <method name="BeginPrompting">
      <arg type="o" name="callback" direction="in"/>
    </method>
    <method name="PerformPrompt">
      <arg type="o" name="callback" direction="in"/>
      <arg type="s" name="type" direction="in"/>
      <arg type="a{sv}" name="properties" direction="in"/>
      <arg type="s" name="exchange" direction="in"/>
    </method>
    <method name="StopPrompting">
      <arg type="o" name="callback" direction="in"/>
    </method>
  </interface>
</node>"#;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Password,
    Confirm,
}

pub enum Answer {
    Cancel,
    Continue { password: String, choice: bool },
}

pub struct Prompt {
    caller: String,
    callback: String,
    kind: Cell<Kind>,
    properties: RefCell<HashMap<String, glib::Variant>>,
    exchange: RefCell<SecretExchange>,
    waiting: Cell<bool>,
    tried_login_password: Cell<bool>,
}

impl Prompt {
    pub fn kind(&self) -> Kind {
        self.kind.get()
    }

    pub fn text(&self, name: &str) -> String {
        self.properties
            .borrow()
            .get(name)
            .and_then(|value| value.str().map(str::to_owned))
            .unwrap_or_default()
    }

    pub fn flag(&self, name: &str) -> bool {
        self.properties
            .borrow()
            .get(name)
            .and_then(|value| value.get::<bool>())
            .unwrap_or(false)
    }
}

pub struct Prompter {
    connection: Option<gio::DBusConnection>,
    states: States,
    prompts: RefCell<Vec<Rc<Prompt>>>,
    unwatches: RefCell<HashMap<String, Box<dyn FnOnce()>>>,
    listeners: Listeners,
}

impl Prompter {
    pub fn new(session: Option<gio::DBusConnection>, states: &States) -> Rc<Self> {
        let prompter = Rc::new(Prompter {
            connection: session.clone(),
            states: states.clone(),
            prompts: RefCell::new(Vec::new()),
            unwatches: RefCell::new(HashMap::new()),
            listeners: Listeners::default(),
        });
        states
            .subscribe({
                let prompter = Rc::downgrade(&prompter);
                move || {
                    if let Some(prompter) = prompter.upgrade() {
                        prompter.announce();
                    }
                }
            })
            .forever();
        if let Some(session) = session {
            prompter.export(&session);
        }
        prompter
    }

    pub fn current(&self) -> Option<Rc<Prompt>> {
        if self.states.screen_locked.get() {
            return None;
        }
        self.prompts
            .borrow()
            .iter()
            .find(|prompt| prompt.waiting.get())
            .cloned()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn answer(&self, prompt: &Rc<Prompt>, answer: Answer) {
        if !prompt.waiting.replace(false) {
            return;
        }
        let mut changed: HashMap<&str, glib::Variant> = HashMap::new();
        if !prompt.text("choice-label").is_empty() {
            let choice = matches!(answer, Answer::Continue { choice: true, .. });
            changed.insert("choice-chosen", choice.to_variant());
        }
        let (reply, exchange) = match &answer {
            Answer::Cancel => ("no", Some(prompt.exchange.borrow().begin())),
            Answer::Continue { password, .. } if prompt.kind() == Kind::Password => {
                if prompt.flag("password-new") {
                    changed.insert(
                        "password-strength",
                        i32::from(!password.is_empty()).to_variant(),
                    );
                }
                ("yes", prompt.exchange.borrow().send(password.as_bytes()))
            }
            Answer::Continue { .. } => ("yes", Some(prompt.exchange.borrow().begin())),
        };
        let (reply, exchange) = match exchange {
            Some(exchange) => (reply, exchange),
            None => ("no", prompt.exchange.borrow().begin()),
        };
        self.call_back(
            prompt,
            "PromptReady",
            (reply, changed, exchange.as_str()).to_variant(),
        );
        self.announce();
    }

    pub fn unlocked_with(&self, password: &str) {
        if password.is_empty() {
            return;
        }
        let Some(connection) = &self.connection else {
            return;
        };
        let owner = connection
            .call_sync(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetNameOwner",
                Some(&(SECRETS,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                -1,
                gio::Cancellable::NONE,
            )
            .ok()
            .and_then(|reply| reply.child_value(0).str().map(str::to_owned));
        let Some(owner) = owner else {
            return;
        };
        let unlocking: Vec<Rc<Prompt>> = self
            .prompts
            .borrow()
            .iter()
            .filter(|prompt| {
                prompt.waiting.get()
                    && prompt.kind() == Kind::Password
                    && !prompt.flag("password-new")
                    && prompt.caller == owner
                    && !prompt.tried_login_password.replace(true)
            })
            .cloned()
            .collect();
        for prompt in unlocking {
            let choice = prompt.flag("choice-chosen");
            self.answer(
                &prompt,
                Answer::Continue {
                    password: password.to_owned(),
                    choice,
                },
            );
        }
    }

    fn export(self: &Rc<Self>, session: &gio::DBusConnection) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        let Some(interface) = node.lookup_interface(INTERFACE) else {
            return;
        };
        let prompter = Rc::downgrade(self);
        let registered = session
            .register_object(PATH, &interface)
            .method_call(move |_, sender, _, _, method, parameters, invocation| {
                let Some(prompter) = prompter.upgrade() else {
                    invocation.return_dbus_error(FAILED, "The prompter is gone");
                    return;
                };
                let sender = sender.unwrap_or_default().to_owned();
                let callback = parameters
                    .child_value(0)
                    .str()
                    .unwrap_or_default()
                    .to_owned();
                match method {
                    "BeginPrompting" => prompter.begin(sender, callback, invocation),
                    "PerformPrompt" => {
                        prompter.perform(&sender, &callback, &parameters, invocation)
                    }
                    "StopPrompting" => prompter.stop(&sender, &callback, invocation),
                    _ => invocation.return_dbus_error(FAILED, "Unknown method"),
                }
            })
            .build();
        let Ok(registration) = registered else {
            return;
        };
        std::mem::forget(registration);
        for name in NAMES {
            gio::bus_own_name_on_connection(
                session,
                name,
                gio::BusNameOwnerFlags::REPLACE,
                |_, _| {},
                |_, _| {},
            );
        }
    }

    fn begin(
        self: &Rc<Self>,
        sender: String,
        callback: String,
        invocation: gio::DBusMethodInvocation,
    ) {
        if self.find(&sender, &callback).is_some() {
            invocation
                .return_dbus_error(FAILED, "Already begun prompting for this prompt callback");
            return;
        }
        let Some(exchange) = SecretExchange::new() else {
            invocation.return_dbus_error(FAILED, "No secret exchange could be set up");
            return;
        };
        let prompt = Rc::new(Prompt {
            caller: sender.clone(),
            callback,
            kind: Cell::new(Kind::Password),
            properties: RefCell::new(HashMap::new()),
            exchange: RefCell::new(exchange),
            waiting: Cell::new(false),
            tried_login_password: Cell::new(false),
        });
        self.prompts.borrow_mut().push(prompt.clone());
        self.watch(&sender);
        invocation.return_value(None);
        let begun = prompt.exchange.borrow().begin();
        let none: HashMap<&str, glib::Variant> = HashMap::new();
        self.call_back(
            &prompt,
            "PromptReady",
            ("", none, begun.as_str()).to_variant(),
        );
    }

    fn perform(
        &self,
        sender: &str,
        callback: &str,
        parameters: &glib::Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        let Some(prompt) = self.find(sender, callback) else {
            invocation.return_dbus_error(FAILED, "Not begun prompting for this prompt callback");
            return;
        };
        if prompt.waiting.get() {
            invocation.return_dbus_error(
                IN_PROGRESS,
                "Already performing a prompt for this prompt callback",
            );
            return;
        }
        let kind = match parameters.child_value(1).str() {
            Some("password") => Kind::Password,
            Some("confirm") => Kind::Confirm,
            _ => {
                invocation.return_dbus_error(FAILED, "Invalid type argument");
                return;
            }
        };
        {
            let mut properties = prompt.properties.borrow_mut();
            for entry in parameters.child_value(2).iter() {
                let (Some(name), Some(value)) = (
                    entry.child_value(0).str().map(str::to_owned),
                    entry.child_value(1).as_variant(),
                ) else {
                    continue;
                };
                properties.insert(name, value);
            }
        }
        let exchange = parameters
            .child_value(3)
            .str()
            .unwrap_or_default()
            .to_owned();
        if prompt.exchange.borrow_mut().receive(&exchange).is_err() {
            invocation.return_dbus_error(FAILED, "Invalid secret exchange received");
            return;
        }
        prompt.kind.set(kind);
        prompt.waiting.set(true);
        invocation.return_value(None);
        self.announce();
    }

    fn stop(&self, sender: &str, callback: &str, invocation: gio::DBusMethodInvocation) {
        let Some(prompt) = self.find(sender, callback) else {
            invocation.return_dbus_error(FAILED, "Not begun prompting for this prompt callback");
            return;
        };
        self.prompts
            .borrow_mut()
            .retain(|other| !Rc::ptr_eq(other, &prompt));
        invocation.return_value(None);
        self.call_back(&prompt, "PromptDone", ().to_variant());
        self.forget_caller_if_idle(sender);
        self.announce();
    }

    fn vanished(&self, name: &str) {
        self.prompts
            .borrow_mut()
            .retain(|prompt| prompt.caller != name);
        self.forget_caller_if_idle(name);
        self.announce();
    }

    fn find(&self, sender: &str, callback: &str) -> Option<Rc<Prompt>> {
        self.prompts
            .borrow()
            .iter()
            .find(|prompt| prompt.caller == sender && prompt.callback == callback)
            .cloned()
    }

    fn watch(self: &Rc<Self>, name: &str) {
        let Some(connection) = &self.connection else {
            return;
        };
        if self.unwatches.borrow().contains_key(name) {
            return;
        }
        let prompter = Rc::downgrade(self);
        let id = gio::bus_watch_name_on_connection(
            connection,
            name,
            gio::BusNameWatcherFlags::NONE,
            |_, _, _| {},
            move |_, name| {
                if let Some(prompter) = prompter.upgrade() {
                    prompter.vanished(name);
                }
            },
        );
        self.unwatches
            .borrow_mut()
            .insert(name.to_owned(), Box::new(move || gio::bus_unwatch_name(id)));
    }

    fn forget_caller_if_idle(&self, name: &str) {
        if self
            .prompts
            .borrow()
            .iter()
            .any(|prompt| prompt.caller == name)
        {
            return;
        }
        let unwatch = self.unwatches.borrow_mut().remove(name);
        if let Some(unwatch) = unwatch {
            unwatch();
        }
    }

    fn call_back(&self, prompt: &Prompt, method: &str, arguments: glib::Variant) {
        let Some(connection) = &self.connection else {
            return;
        };
        connection.call(
            Some(&prompt.caller),
            &prompt.callback,
            CALLBACK,
            method,
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
            |_| {},
        );
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}
