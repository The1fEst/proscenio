use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::core::listeners::{Listeners, Subscription};
use crate::services::network::bluez::{BUS, Bluez};

const INTERFACE: &str = "org.bluez.Agent1";
const PATH: &str = "/dev/fEst/Proscenio/BluezAgent";
const MANAGER_PATH: &str = "/org/bluez";
const MANAGER_INTERFACE: &str = "org.bluez.AgentManager1";
const CAPABILITY: &str = "KeyboardDisplay";
const REJECTED_ERROR_NAME: &str = "org.bluez.Error.Rejected";
const CANCELED_ERROR_NAME: &str = "org.bluez.Error.Canceled";
const UNKNOWN_METHOD_ERROR_NAME: &str = "org.freedesktop.DBus.Error.UnknownMethod";
const PIN_LENGTH_MAX: usize = 16;
const PASSKEY_DIGITS: usize = 6;
const REGISTER_TIMEOUT_MILLIS: i32 = 5000;
const DISPLAY_TIMEOUT: Duration = Duration::from_secs(90);

const INTROSPECTION: &str = r#"<node>
  <interface name="org.bluez.Agent1">
    <method name="Release"/>
    <method name="RequestPinCode">
      <arg type="o" name="device" direction="in"/>
      <arg type="s" name="pincode" direction="out"/>
    </method>
    <method name="DisplayPinCode">
      <arg type="o" name="device" direction="in"/>
      <arg type="s" name="pincode" direction="in"/>
    </method>
    <method name="RequestPasskey">
      <arg type="o" name="device" direction="in"/>
      <arg type="u" name="passkey" direction="out"/>
    </method>
    <method name="DisplayPasskey">
      <arg type="o" name="device" direction="in"/>
      <arg type="u" name="passkey" direction="in"/>
      <arg type="q" name="entered" direction="in"/>
    </method>
    <method name="RequestConfirmation">
      <arg type="o" name="device" direction="in"/>
      <arg type="u" name="passkey" direction="in"/>
    </method>
    <method name="RequestAuthorization">
      <arg type="o" name="device" direction="in"/>
    </method>
    <method name="AuthorizeService">
      <arg type="o" name="device" direction="in"/>
      <arg type="s" name="uuid" direction="in"/>
    </method>
    <method name="Cancel"/>
  </interface>
</node>"#;

#[derive(Clone, Debug, PartialEq)]
pub enum Prompt {
    PinCode,
    Passkey,
    PinCodeDisplay(String),
    PasskeyDisplay(u32),
    Confirmation(u32),
    Authorization,
    Service(String),
}

impl Prompt {
    pub fn has_input(&self) -> bool {
        matches!(self, Prompt::PinCode | Prompt::Passkey)
    }

    pub fn is_display(&self) -> bool {
        matches!(self, Prompt::PinCodeDisplay(_) | Prompt::PasskeyDisplay(_))
    }

    pub fn code(&self) -> Option<String> {
        match self {
            Prompt::PinCodeDisplay(pin) => Some(pin.clone()),
            Prompt::PasskeyDisplay(passkey) | Prompt::Confirmation(passkey) => {
                Some(format_passkey(*passkey))
            }
            _ => None,
        }
    }

    pub fn refusal(&self) -> Answer {
        if self.has_input() {
            Answer::Cancel
        } else {
            Answer::Reject
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Answer {
    Accept(String),
    Reject,
    Cancel,
}

#[derive(Debug, PartialEq)]
pub enum Reply {
    Empty,
    PinCode(String),
    Passkey(u32),
    Error(&'static str),
}

pub fn format_passkey(passkey: u32) -> String {
    format!("{passkey:06}")
}

pub fn is_valid_pin(pin: &str) -> bool {
    (1..=PIN_LENGTH_MAX).contains(&pin.chars().count())
        && pin
            .chars()
            .all(|character| character.is_ascii_alphanumeric())
}

pub fn parse_passkey(text: &str) -> Option<u32> {
    let digits: String = text.chars().filter(|character| *character != ' ').collect();
    if digits.is_empty()
        || digits.len() > PASSKEY_DIGITS
        || !digits.chars().all(|digit| digit.is_ascii_digit())
    {
        return None;
    }
    digits.parse().ok()
}

pub fn is_valid_answer(prompt: &Prompt, text: &str) -> bool {
    !matches!(
        choose_reply(prompt, &Answer::Accept(text.to_owned())),
        Reply::Error(_)
    )
}

pub fn choose_reply(prompt: &Prompt, answer: &Answer) -> Reply {
    let text = match answer {
        Answer::Reject => return Reply::Error(REJECTED_ERROR_NAME),
        Answer::Cancel => return Reply::Error(CANCELED_ERROR_NAME),
        Answer::Accept(text) => text,
    };
    match prompt {
        Prompt::PinCode if is_valid_pin(text) => Reply::PinCode(text.clone()),
        Prompt::Passkey => parse_passkey(text)
            .map(Reply::Passkey)
            .unwrap_or(Reply::Error(REJECTED_ERROR_NAME)),
        Prompt::PinCode => Reply::Error(REJECTED_ERROR_NAME),
        _ => Reply::Empty,
    }
}

pub struct Request {
    pub device_path: String,
    pub device_name: String,
    pub prompt: Prompt,
    was_paired: bool,
    invocation: RefCell<Option<gio::DBusMethodInvocation>>,
}

struct ClosedDisplay {
    device_path: String,
    prompt: Prompt,
    close_instant: Instant,
}

pub struct BluezAgent {
    bluez: Bluez,
    request: RefCell<Option<Rc<Request>>>,
    closed_display: RefCell<Option<ClosedDisplay>>,
    owner: RefCell<Option<String>>,
    listeners: Listeners,
}

impl BluezAgent {
    pub fn new(system: Option<gio::DBusConnection>, bluez: &Bluez) -> Rc<Self> {
        let agent = Rc::new(BluezAgent {
            bluez: bluez.clone(),
            request: RefCell::new(None),
            closed_display: RefCell::new(None),
            owner: RefCell::new(None),
            listeners: Listeners::default(),
        });
        let Some(system) = system else {
            return agent;
        };
        if !agent.export(&system) {
            eprintln!("bluezagent: {PATH} could not be exported");
            return agent;
        }
        agent.follow_owner(&system);
        bluez
            .subscribe({
                let agent = Rc::downgrade(&agent);
                move || {
                    if let Some(agent) = agent.upgrade() {
                        agent.close_paired_display();
                    }
                }
            })
            .detach();
        agent
    }

    pub fn request(&self) -> Option<Rc<Request>> {
        self.request.borrow().clone()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn answer(&self, request: &Rc<Request>, answer: Answer) {
        let is_current = self
            .request()
            .is_some_and(|current| Rc::ptr_eq(&current, request));
        if !is_current {
            return;
        }
        self.request.take();
        if request.prompt.is_display() && answer != Answer::Cancel {
            self.closed_display.replace(Some(ClosedDisplay {
                device_path: request.device_path.clone(),
                prompt: request.prompt.clone(),
                close_instant: Instant::now(),
            }));
        }
        reply(request, &answer);
        self.listeners.notify();
    }

    fn show(self: &Rc<Self>, request: Request) {
        let request = Rc::new(request);
        if let Some(previous) = self.request.replace(Some(request.clone())) {
            reply(&previous, &Answer::Cancel);
        }
        self.listeners.notify();
        if !request.prompt.is_display() {
            return;
        }
        let agent = Rc::downgrade(self);
        let request = Rc::downgrade(&request);
        glib::timeout_add_local_once(DISPLAY_TIMEOUT, move || {
            let (Some(agent), Some(request)) = (agent.upgrade(), request.upgrade()) else {
                return;
            };
            agent.answer(&request, Answer::Cancel);
        });
    }

    fn cancel(&self) {
        if let Some(request) = self.request() {
            self.answer(&request, Answer::Cancel);
        }
    }

    fn close_paired_display(&self) {
        let Some(request) = self.request() else {
            return;
        };
        let is_paired = request.prompt.is_display()
            && !request.was_paired
            && self
                .bluez
                .devices
                .borrow()
                .iter()
                .any(|device| device.path == request.device_path && device.is_paired);
        if is_paired {
            self.answer(&request, Answer::Cancel);
        }
    }

    fn export(self: &Rc<Self>, system: &gio::DBusConnection) -> bool {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return false;
        };
        let Some(interface) = node.lookup_interface(INTERFACE) else {
            return false;
        };
        let agent = Rc::downgrade(self);
        system
            .register_object(PATH, &interface)
            .method_call(move |_, sender, _, _, method, parameters, invocation| {
                let Some(agent) = agent.upgrade() else {
                    invocation.return_dbus_error(REJECTED_ERROR_NAME, "The agent is gone");
                    return;
                };
                let is_from_bluez = sender.is_some() && sender == agent.owner.borrow().as_deref();
                if !is_from_bluez {
                    invocation.return_dbus_error(REJECTED_ERROR_NAME, "Not from BlueZ");
                    return;
                }
                agent.handle(method, &parameters, invocation);
            })
            .build()
            .is_ok()
    }

    fn handle(
        self: &Rc<Self>,
        method: &str,
        parameters: &Variant,
        invocation: gio::DBusMethodInvocation,
    ) {
        let device_path = parameters
            .try_child_value(0)
            .and_then(|value| value.get::<glib::variant::ObjectPath>())
            .map(|path| path.as_str().to_owned())
            .unwrap_or_default();
        let read_text = || {
            parameters
                .try_child_value(1)
                .and_then(|value| value.get::<String>())
                .unwrap_or_default()
        };
        let read_number = || {
            parameters
                .try_child_value(1)
                .and_then(|value| value.get::<u32>())
                .unwrap_or_default()
        };
        let prompt = match method {
            "Release" | "Cancel" => {
                invocation.return_value(None);
                self.cancel();
                return;
            }
            "RequestPinCode" => Prompt::PinCode,
            "RequestPasskey" => Prompt::Passkey,
            "DisplayPinCode" => Prompt::PinCodeDisplay(read_text()),
            "DisplayPasskey" => Prompt::PasskeyDisplay(read_number()),
            "RequestConfirmation" => Prompt::Confirmation(read_number()),
            "RequestAuthorization" => Prompt::Authorization,
            "AuthorizeService" => Prompt::Service(read_text()),
            _ => {
                invocation.return_dbus_error(UNKNOWN_METHOD_ERROR_NAME, "Unknown method");
                return;
            }
        };
        let is_same_display = prompt.is_display()
            && (self.request().is_some_and(|request| {
                request.device_path == device_path && request.prompt == prompt
            }) || self.closed_display.borrow().as_ref().is_some_and(|closed| {
                closed.device_path == device_path
                    && closed.prompt == prompt
                    && closed.close_instant.elapsed() < DISPLAY_TIMEOUT
            }));
        let invocation = if prompt.is_display() {
            invocation.return_value(None);
            None
        } else {
            Some(invocation)
        };
        if is_same_display {
            return;
        }
        let device = self
            .bluez
            .devices
            .borrow()
            .iter()
            .find(|device| device.path == device_path)
            .cloned();
        let was_paired = device.as_ref().is_some_and(|device| device.is_paired);
        let device_name = device
            .map(|device| device.name)
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| extract_address(&device_path));
        self.show(Request {
            device_path,
            device_name,
            prompt,
            was_paired,
            invocation: RefCell::new(invocation),
        });
    }

    fn follow_owner(self: &Rc<Self>, system: &gio::DBusConnection) {
        let id = gio::bus_watch_name_on_connection(
            system,
            BUS,
            gio::BusNameWatcherFlags::NONE,
            {
                let agent = Rc::downgrade(self);
                move |system, _, owner| {
                    let Some(agent) = agent.upgrade() else {
                        return;
                    };
                    agent.owner.replace(Some(owner.to_owned()));
                    register(&system);
                }
            },
            {
                let agent = Rc::downgrade(self);
                move |_, _| {
                    let Some(agent) = agent.upgrade() else {
                        return;
                    };
                    agent.owner.take();
                    agent.cancel();
                }
            },
        );
        std::mem::forget(id);
    }
}

fn extract_address(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or_default();
    name.strip_prefix("dev_")
        .map(|address| address.replace('_', ":"))
        .unwrap_or_else(|| name.to_owned())
}

fn reply(request: &Request, answer: &Answer) {
    let Some(invocation) = request.invocation.take() else {
        return;
    };
    match choose_reply(&request.prompt, answer) {
        Reply::Empty => invocation.return_value(None),
        Reply::PinCode(pin) => invocation.return_value(Some(&(pin,).to_variant())),
        Reply::Passkey(passkey) => invocation.return_value(Some(&(passkey,).to_variant())),
        Reply::Error(name) => {
            invocation.return_dbus_error(name, name.rsplit('.').next().unwrap_or(name))
        }
    }
}

fn register(system: &gio::DBusConnection) {
    let system = system.clone();
    glib::spawn_future_local(async move {
        let Ok(path) = glib::variant::ObjectPath::try_from(PATH.to_owned()) else {
            return;
        };
        let call = |method: &'static str, parameters: Variant| {
            system.call_future(
                Some(BUS),
                MANAGER_PATH,
                MANAGER_INTERFACE,
                method,
                Some(&parameters),
                None,
                gio::DBusCallFlags::NONE,
                REGISTER_TIMEOUT_MILLIS,
            )
        };
        for (method, parameters) in [
            ("RegisterAgent", (path.clone(), CAPABILITY).to_variant()),
            ("RequestDefaultAgent", (path,).to_variant()),
        ] {
            if let Err(error) = call(method, parameters).await {
                eprintln!("bluezagent: {method} failed: {error}");
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passkeys_show_six_zero_padded_digits() {
        assert_eq!(format_passkey(0), "000000");
        assert_eq!(format_passkey(4271), "004271");
        assert_eq!(format_passkey(999_999), "999999");
        assert_eq!(Prompt::Confirmation(42).code().as_deref(), Some("000042"));
    }

    #[test]
    fn pins_are_one_to_sixteen_letters_or_digits() {
        let cases = [
            ("", false),
            ("0000", true),
            ("abcDEF123", true),
            ("1234567890123456", true),
            ("12345678901234567", false),
            ("12 34", false),
            ("пин", false),
        ];
        for (pin, is_valid) in cases {
            assert_eq!(is_valid_pin(pin), is_valid, "{pin:?}");
        }
    }

    #[test]
    fn passkeys_are_up_to_six_digits() {
        let cases = [
            ("", None),
            ("0", Some(0)),
            ("012345", Some(12345)),
            ("999999", Some(999_999)),
            ("1000000", None),
            ("+12", None),
            ("12a", None),
            ("123 456", Some(123_456)),
            ("1234 567", None),
        ];
        for (text, passkey) in cases {
            assert_eq!(parse_passkey(text), passkey, "{text:?}");
        }
    }

    #[test]
    fn each_answer_gets_its_reply() {
        let accept = |text: &str| Answer::Accept(text.to_owned());
        let cases = [
            (
                Prompt::PinCode,
                accept("0000"),
                Reply::PinCode("0000".to_owned()),
            ),
            (
                Prompt::PinCode,
                accept(""),
                Reply::Error(REJECTED_ERROR_NAME),
            ),
            (Prompt::Passkey, accept("012345"), Reply::Passkey(12345)),
            (
                Prompt::Passkey,
                accept("abc"),
                Reply::Error(REJECTED_ERROR_NAME),
            ),
            (Prompt::Confirmation(1), accept(""), Reply::Empty),
            (Prompt::Authorization, accept(""), Reply::Empty),
            (Prompt::Service("x".to_owned()), accept(""), Reply::Empty),
            (
                Prompt::Confirmation(1),
                Answer::Reject,
                Reply::Error(REJECTED_ERROR_NAME),
            ),
            (
                Prompt::Service("x".to_owned()),
                Answer::Reject,
                Reply::Error(REJECTED_ERROR_NAME),
            ),
            (
                Prompt::PinCode,
                Answer::Cancel,
                Reply::Error(CANCELED_ERROR_NAME),
            ),
            (
                Prompt::Passkey,
                Answer::Cancel,
                Reply::Error(CANCELED_ERROR_NAME),
            ),
        ];
        for (prompt, answer, reply) in cases {
            assert_eq!(
                choose_reply(&prompt, &answer),
                reply,
                "{prompt:?} {answer:?}"
            );
        }
    }

    #[test]
    fn an_unlisted_device_is_named_by_its_address() {
        assert_eq!(
            extract_address("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_0F"),
            "AA:BB:CC:DD:EE:0F"
        );
        assert_eq!(extract_address("/org/bluez/hci0/other"), "other");
    }

    #[test]
    fn a_refusal_cancels_an_input_and_rejects_a_question() {
        assert_eq!(Prompt::PinCode.refusal(), Answer::Cancel);
        assert_eq!(Prompt::Passkey.refusal(), Answer::Cancel);
        assert_eq!(Prompt::Confirmation(1).refusal(), Answer::Reject);
        assert_eq!(Prompt::Authorization.refusal(), Answer::Reject);
    }
}
