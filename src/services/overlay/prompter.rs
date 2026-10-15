use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::security::pam;
use crate::platform::security::secretexchange::SecretExchange;
use crate::services::session::states::States;

const NAMES: [&str; 2] = [
    "org.gnome.keyring.SystemPrompter",
    "org.gnome.keyring.PrivatePrompter",
];
const PATH: &str = "/org/gnome/keyring/Prompter";
const INTERFACE: &str = "org.gnome.keyring.internal.Prompter";
const CALLBACK: &str = "org.gnome.keyring.internal.Prompter.Callback";
const FAILED_ERROR_NAME: &str = "org.gnome.keyring.Prompter.Failed";
const IN_PROGRESS_ERROR_NAME: &str = "org.gnome.keyring.Prompter.InProgress";
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
    Continue {
        password: String,
        is_choice_chosen: bool,
    },
}

impl Drop for Answer {
    fn drop(&mut self) {
        if let Answer::Continue { password, .. } = self {
            pam::wipe(&mut std::mem::take(password).into_bytes());
        }
    }
}

pub struct Prompt {
    caller: String,
    callback: String,
    kind: Cell<Kind>,
    properties: RefCell<HashMap<String, glib::Variant>>,
    exchange: RefCell<SecretExchange>,
    is_waiting: Cell<bool>,
    has_tried_login_password: Cell<bool>,
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
    unwatch_callbacks: RefCell<HashMap<String, Box<dyn FnOnce()>>>,
    secrets_owner: RefCell<Option<String>>,
    listeners: Listeners,
}

impl Prompter {
    pub fn new(session: Option<gio::DBusConnection>, states: &States) -> Rc<Self> {
        let prompter = Rc::new(Prompter {
            connection: session.clone(),
            states: states.clone(),
            prompts: RefCell::new(Vec::new()),
            unwatch_callbacks: RefCell::new(HashMap::new()),
            secrets_owner: RefCell::new(None),
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
            .detach();
        if let Some(session) = session {
            prompter.export(&session);
            prompter.follow_secrets_owner(&session);
        }
        prompter
    }

    pub fn current(&self) -> Option<Rc<Prompt>> {
        if self.states.is_screen_locked.get() {
            return None;
        }
        self.prompts
            .borrow()
            .iter()
            .find(|prompt| prompt.is_waiting.get())
            .cloned()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn answer(&self, prompt: &Rc<Prompt>, answer: Answer) {
        if !prompt.is_waiting.replace(false) {
            return;
        }
        let mut changed_properties: HashMap<&str, glib::Variant> = HashMap::new();
        if !prompt.text("choice-label").is_empty() {
            let is_choice_chosen = matches!(
                answer,
                Answer::Continue {
                    is_choice_chosen: true,
                    ..
                }
            );
            changed_properties.insert("choice-chosen", is_choice_chosen.to_variant());
        }
        let (reply, exchange) = match &answer {
            Answer::Cancel => ("no", Some(prompt.exchange.borrow().begin())),
            Answer::Continue { password, .. } if prompt.kind() == Kind::Password => {
                if prompt.flag("password-new") {
                    changed_properties.insert(
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
            (reply, changed_properties, exchange.as_str()).to_variant(),
        );
        self.announce();
    }

    pub fn answer_with_login_password(&self, password: &str) {
        if password.is_empty() {
            return;
        }
        let Some(owner) = self.secrets_owner.borrow().clone() else {
            return;
        };
        let login_prompts: Vec<Rc<Prompt>> = self
            .prompts
            .borrow()
            .iter()
            .filter(|prompt| {
                prompt.is_waiting.get()
                    && prompt.kind() == Kind::Password
                    && !prompt.flag("password-new")
                    && prompt.caller == owner
                    && !prompt.has_tried_login_password.replace(true)
            })
            .cloned()
            .collect();
        for prompt in login_prompts {
            let is_choice_chosen = prompt.flag("choice-chosen");
            self.answer(
                &prompt,
                Answer::Continue {
                    password: password.to_owned(),
                    is_choice_chosen,
                },
            );
        }
    }

    fn follow_secrets_owner(self: &Rc<Self>, session: &gio::DBusConnection) {
        gio::bus_watch_name_on_connection(
            session,
            SECRETS,
            gio::BusNameWatcherFlags::NONE,
            {
                let prompter = Rc::downgrade(self);
                move |_, _, owner| {
                    if let Some(prompter) = prompter.upgrade() {
                        prompter.secrets_owner.replace(Some(owner.to_owned()));
                    }
                }
            },
            {
                let prompter = Rc::downgrade(self);
                move |_, _| {
                    if let Some(prompter) = prompter.upgrade() {
                        prompter.secrets_owner.take();
                    }
                }
            },
        );
    }

    fn export(self: &Rc<Self>, session: &gio::DBusConnection) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        let Some(interface) = node.lookup_interface(INTERFACE) else {
            return;
        };
        let prompter = Rc::downgrade(self);
        let registration_result = session
            .register_object(PATH, &interface)
            .method_call(move |_, sender, _, _, method, parameters, invocation| {
                let Some(prompter) = prompter.upgrade() else {
                    invocation.return_dbus_error(FAILED_ERROR_NAME, "The prompter is gone");
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
                    _ => invocation.return_dbus_error(FAILED_ERROR_NAME, "Unknown method"),
                }
            })
            .build();
        let Ok(registration) = registration_result else {
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
            invocation.return_dbus_error(
                FAILED_ERROR_NAME,
                "Already begun prompting for this prompt callback",
            );
            return;
        }
        let Some(exchange) = SecretExchange::new() else {
            invocation.return_dbus_error(FAILED_ERROR_NAME, "No secret exchange could be set up");
            return;
        };
        let prompt = Rc::new(Prompt {
            caller: sender.clone(),
            callback,
            kind: Cell::new(Kind::Password),
            properties: RefCell::new(HashMap::new()),
            exchange: RefCell::new(exchange),
            is_waiting: Cell::new(false),
            has_tried_login_password: Cell::new(false),
        });
        self.prompts.borrow_mut().push(prompt.clone());
        self.watch(&sender);
        invocation.return_value(None);
        let exchange_text = prompt.exchange.borrow().begin();
        let empty_properties: HashMap<&str, glib::Variant> = HashMap::new();
        self.call_back(
            &prompt,
            "PromptReady",
            ("", empty_properties, exchange_text.as_str()).to_variant(),
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
            invocation.return_dbus_error(
                FAILED_ERROR_NAME,
                "Not begun prompting for this prompt callback",
            );
            return;
        };
        if prompt.is_waiting.get() {
            invocation.return_dbus_error(
                IN_PROGRESS_ERROR_NAME,
                "Already performing a prompt for this prompt callback",
            );
            return;
        }
        let kind = match parameters.child_value(1).str() {
            Some("password") => Kind::Password,
            Some("confirm") => Kind::Confirm,
            _ => {
                invocation.return_dbus_error(FAILED_ERROR_NAME, "Invalid type argument");
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
            invocation.return_dbus_error(FAILED_ERROR_NAME, "Invalid secret exchange received");
            return;
        }
        prompt.kind.set(kind);
        prompt.is_waiting.set(true);
        invocation.return_value(None);
        self.announce();
    }

    fn stop(&self, sender: &str, callback: &str, invocation: gio::DBusMethodInvocation) {
        let Some(prompt) = self.find(sender, callback) else {
            invocation.return_dbus_error(
                FAILED_ERROR_NAME,
                "Not begun prompting for this prompt callback",
            );
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

    fn on_name_vanished(&self, name: &str) {
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
        if self.unwatch_callbacks.borrow().contains_key(name) {
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
                    prompter.on_name_vanished(name);
                }
            },
        );
        self.unwatch_callbacks
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
        let unwatch = self.unwatch_callbacks.borrow_mut().remove(name);
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
