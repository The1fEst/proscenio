use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::security::pam;

const INTERFACE: &str = "org.freedesktop.PolicyKit1.AuthenticationAgent";
const PATH: &str = "/dev/fEst/Proscenio/PolkitAgent";
const AUTHORITY: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const AUTHORITY_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";
const HELPER_SOCKET: &str = "/run/polkit/agent-helper.socket";
const HELPER: &str = "/usr/lib/polkit-1/polkit-agent-helper-1";
const CANCELLED_ERROR_NAME: &str = "org.freedesktop.PolicyKit1.Error.Cancelled";
const FAILED_ERROR_NAME: &str = "org.freedesktop.PolicyKit1.Error.Failed";

const INTROSPECTION: &str = r#"<node>
  <interface name="org.freedesktop.PolicyKit1.AuthenticationAgent">
    <method name="BeginAuthentication">
      <arg type="s" name="action_id" direction="in"/>
      <arg type="s" name="message" direction="in"/>
      <arg type="s" name="icon_name" direction="in"/>
      <arg type="a{ss}" name="details" direction="in"/>
      <arg type="s" name="cookie" direction="in"/>
      <arg type="a(sa{sv})" name="identities" direction="in"/>
    </method>
    <method name="CancelAuthentication">
      <arg type="s" name="cookie" direction="in"/>
    </method>
  </interface>
</node>"#;

pub struct Flow {
    pub message: String,
    prompt: RefCell<String>,
    is_response_visible: Cell<bool>,
    is_interaction_available: Cell<bool>,
    cookie: String,
    user: String,
    invocation: RefCell<Option<gio::DBusMethodInvocation>>,
    input: RefCell<Option<gio::OutputStream>>,
    session: RefCell<Option<glib::Object>>,
    cancellable: RefCell<gio::Cancellable>,
}

impl Flow {
    pub fn prompt(&self) -> String {
        self.prompt.borrow().clone()
    }

    pub fn is_response_visible(&self) -> bool {
        self.is_response_visible.get()
    }

    pub fn is_interaction_available(&self) -> bool {
        self.is_interaction_available.get()
    }
}

#[derive(Default)]
pub struct Polkit {
    flow: RefCell<Option<Rc<Flow>>>,
    listeners: Listeners,
}

impl Polkit {
    pub fn new(system: Option<gio::DBusConnection>) -> Rc<Self> {
        let polkit = Rc::new(Polkit::default());
        if let Some(system) = system {
            polkit.export(&system);
        }
        polkit
    }

    pub fn flow(&self) -> Option<Rc<Flow>> {
        self.flow.borrow().clone()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn submit(self: &Rc<Self>, response: String) {
        let target = self.flow().and_then(|flow| {
            let input = flow.input.borrow().clone()?;
            Some((flow, input))
        });
        let mut line = String::with_capacity(response.len() + 1);
        line.push_str(&response);
        line.push('\n');
        pam::wipe(&mut response.into_bytes());
        if let Some((flow, input)) = target {
            flow.is_interaction_available.set(false);
            self.announce();
            let _ = input.write_all(line.as_bytes(), gio::Cancellable::NONE);
        }
        pam::wipe(&mut line.into_bytes());
    }

    pub fn cancel(self: &Rc<Self>) {
        let Some(flow) = self.flow.take() else {
            return;
        };
        flow.cancellable.borrow().cancel();
        close(&flow);
        if let Some(invocation) = flow.invocation.take() {
            invocation.return_dbus_error(CANCELLED_ERROR_NAME, "Authentication was cancelled");
        }
        self.announce();
    }

    fn export(self: &Rc<Self>, system: &gio::DBusConnection) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        let Some(interface) = node.lookup_interface(INTERFACE) else {
            return;
        };
        let polkit = Rc::downgrade(self);
        let registration_result = system
            .register_object(PATH, &interface)
            .method_call(move |_, _, _, _, method, parameters, invocation| {
                let Some(polkit) = polkit.upgrade() else {
                    invocation.return_dbus_error(FAILED_ERROR_NAME, "The agent is gone");
                    return;
                };
                match method {
                    "BeginAuthentication" => polkit.begin(&parameters, invocation),
                    "CancelAuthentication" => {
                        let cookie = parameters
                            .child_value(0)
                            .get::<String>()
                            .unwrap_or_default();
                        let is_current = polkit.flow().is_some_and(|flow| flow.cookie == cookie);
                        if is_current {
                            polkit.cancel();
                        }
                        invocation.return_value(None);
                    }
                    _ => invocation.return_dbus_error(FAILED_ERROR_NAME, "Unknown method"),
                }
            })
            .build();
        if registration_result.is_err() {
            return;
        }
        register(system);
    }

    fn begin(self: &Rc<Self>, parameters: &glib::Variant, invocation: gio::DBusMethodInvocation) {
        if self.flow().is_some() {
            invocation
                .return_dbus_error(FAILED_ERROR_NAME, "Another authentication is in progress");
            return;
        }
        let message = parameters
            .child_value(1)
            .get::<String>()
            .unwrap_or_default();
        let cookie = parameters
            .child_value(4)
            .get::<String>()
            .unwrap_or_default();
        let Some(user) = choose_user(&parameters.child_value(5)) else {
            invocation
                .return_dbus_error(FAILED_ERROR_NAME, "No identity this agent can authenticate");
            return;
        };
        let flow = Rc::new(Flow {
            message,
            prompt: RefCell::new(String::new()),
            is_response_visible: Cell::new(false),
            is_interaction_available: Cell::new(false),
            cookie,
            user,
            invocation: RefCell::new(Some(invocation)),
            input: RefCell::new(None),
            session: RefCell::new(None),
            cancellable: RefCell::new(gio::Cancellable::new()),
        });
        self.flow.replace(Some(flow.clone()));
        self.announce();
        self.start(&flow);
    }

    fn start(self: &Rc<Self>, flow: &Rc<Flow>) {
        let cancellable = gio::Cancellable::new();
        flow.cancellable.replace(cancellable.clone());
        let polkit = Rc::downgrade(self);
        let flow = flow.clone();
        glib::spawn_future_local(async move {
            let Some((input, output)) = connect(&flow, &cancellable).await else {
                if let Some(polkit) = polkit.upgrade() {
                    polkit.finish(&flow, false);
                }
                return;
            };
            flow.input.replace(Some(input));
            let reader = gio::DataInputStream::new(&output);
            let mut was_asked = false;
            loop {
                let line = reader.read_line_utf8_future(glib::Priority::DEFAULT).await;
                let Some(polkit) = polkit.upgrade() else {
                    return;
                };
                if cancellable.is_cancelled() || !polkit.is_current(&flow) {
                    return;
                }
                let Ok(Some(line)) = line else {
                    polkit.finish(&flow, false);
                    return;
                };
                match decide(&line, &mut was_asked) {
                    Action::Ask(prompt, is_response_visible) => {
                        polkit.ask(&flow, &prompt, is_response_visible)
                    }
                    Action::Finish(is_success) => {
                        polkit.finish(&flow, is_success);
                        return;
                    }
                    Action::Retry => {
                        polkit.retry(&flow);
                        return;
                    }
                    Action::Wait => {}
                }
            }
        });
    }

    fn ask(&self, flow: &Rc<Flow>, prompt: &str, is_response_visible: bool) {
        flow.prompt.replace(prompt.to_owned());
        flow.is_response_visible.set(is_response_visible);
        flow.is_interaction_available.set(true);
        self.announce();
    }

    fn retry(self: &Rc<Self>, flow: &Rc<Flow>) {
        close(flow);
        self.start(flow);
    }

    fn finish(&self, flow: &Rc<Flow>, is_success: bool) {
        if !self.is_current(flow) {
            return;
        }
        close(flow);
        self.flow.replace(None);
        if let Some(invocation) = flow.invocation.take() {
            if is_success {
                invocation.return_value(None);
            } else {
                invocation.return_dbus_error(FAILED_ERROR_NAME, "Authentication failed");
            }
        }
        self.announce();
    }

    fn is_current(&self, flow: &Rc<Flow>) -> bool {
        self.flow()
            .is_some_and(|current| Rc::ptr_eq(&current, flow))
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

fn close(flow: &Flow) {
    flow.input.replace(None);
    let Some(session) = flow.session.take() else {
        return;
    };
    if let Some(connection) = session.downcast_ref::<gio::IOStream>() {
        let _ = connection.close(gio::Cancellable::NONE);
    } else if let Some(process) = session.downcast_ref::<gio::Subprocess>() {
        process.force_exit();
    }
}

async fn connect(
    flow: &Rc<Flow>,
    cancellable: &gio::Cancellable,
) -> Option<(gio::OutputStream, gio::InputStream)> {
    if std::path::Path::new(HELPER_SOCKET).exists() {
        let client = gio::SocketClient::new();
        let address = gio::UnixSocketAddress::new(std::path::Path::new(HELPER_SOCKET));
        let connection = client.connect_future(&address).await.ok()?;
        let input = connection.output_stream();
        let greeting = format!("{}\n{}\n", flow.user, flow.cookie);
        input
            .write_all(greeting.as_bytes(), Some(cancellable))
            .ok()?;
        let output = connection.input_stream();
        flow.session.replace(Some(connection.upcast()));
        return Some((input, output));
    }
    let process = gio::Subprocess::newv(
        &[
            std::ffi::OsStr::new(HELPER),
            std::ffi::OsStr::new(&flow.user),
        ],
        gio::SubprocessFlags::STDIN_PIPE
            | gio::SubprocessFlags::STDOUT_PIPE
            | gio::SubprocessFlags::STDERR_SILENCE,
    )
    .ok()?;
    let input = process.stdin_pipe()?;
    let output = process.stdout_pipe()?;
    let greeting = format!("{}\n", flow.cookie);
    input
        .write_all(greeting.as_bytes(), Some(cancellable))
        .ok()?;
    flow.session.replace(Some(process.upcast()));
    Some((input, output))
}

fn register(system: &gio::DBusConnection) {
    let Some(login_session_id) = session_id(system) else {
        eprintln!("polkit: no login session to register the agent for");
        return;
    };
    let details = HashMap::from([("session-id".to_owned(), login_session_id.to_variant())]);
    let subject = ("unix-session", details).to_variant();
    let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".to_owned());
    let parameters =
        glib::Variant::tuple_from_iter([subject, locale.to_variant(), PATH.to_variant()]);
    system.call(
        Some(AUTHORITY),
        AUTHORITY_PATH,
        AUTHORITY_INTERFACE,
        "RegisterAuthenticationAgent",
        Some(&parameters),
        None,
        gio::DBusCallFlags::NONE,
        -1,
        gio::Cancellable::NONE,
        |result| {
            if let Err(error) = result {
                eprintln!("polkit: the agent could not register: {error}");
            }
        },
    );
}

fn session_id(system: &gio::DBusConnection) -> Option<String> {
    if let Ok(id) = std::env::var("XDG_SESSION_ID")
        && !id.is_empty()
    {
        return Some(id);
    }
    let reply = system
        .call_sync(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "GetSessionByPID",
            Some(&(std::process::id(),).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let path = reply.child_value(0).str()?.to_owned();
    let property = system
        .call_sync(
            Some("org.freedesktop.login1"),
            &path,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&("org.freedesktop.login1.Session", "Id").to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .ok()?;
    property.child_value(0).as_variant()?.get::<String>()
}

fn choose_user(identities: &glib::Variant) -> Option<String> {
    let own_uid = std::fs::metadata("/proc/self").ok()?.uid();
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    let groups = std::fs::read_to_string("/etc/group").unwrap_or_default();
    let mut users = Vec::new();
    for identity in identities.iter() {
        let kind = identity.child_value(0).get::<String>().unwrap_or_default();
        let details = glib::VariantDict::new(Some(&identity.child_value(1)));
        let number = |key: &str| {
            details
                .lookup_value(key, None)
                .and_then(|value| value.get::<u32>())
        };
        match kind.as_str() {
            "unix-user" => {
                if let Some(uid) = number("uid") {
                    users.push(uid);
                }
            }
            "unix-group" => {
                if let Some(gid) = number("gid") {
                    users.extend(group_members(&passwd, &groups, gid));
                }
            }
            _ => {}
        }
    }
    let uid = users
        .iter()
        .copied()
        .find(|uid| *uid == own_uid)
        .or_else(|| users.first().copied())?;
    user_name(&passwd, uid)
}

fn user_name(passwd: &str, uid: u32) -> Option<String> {
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        (fields.get(2)?.parse::<u32>().ok()? == uid).then(|| fields[0].to_owned())
    })
}

fn group_members(passwd: &str, groups: &str, gid: u32) -> Vec<u32> {
    let uid_of = |name: &str| {
        passwd.lines().find_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            (fields.first() == Some(&name))
                .then(|| fields.get(2)?.parse::<u32>().ok())
                .flatten()
        })
    };
    let mut members: Vec<u32> = groups
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            (fields.get(2)?.parse::<u32>().ok()? == gid).then(|| fields.get(3).copied())?
        })
        .flat_map(|list| list.split(','))
        .filter(|name| !name.is_empty())
        .filter_map(uid_of)
        .collect();
    members.extend(passwd.lines().filter_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        (fields.get(3)?.parse::<u32>().ok()? == gid).then(|| fields.get(2)?.parse::<u32>().ok())?
    }));
    members
}

#[derive(Debug, PartialEq)]
enum Action {
    Ask(String, bool),
    Finish(bool),
    Retry,
    Wait,
}

fn decide(line: &str, was_asked: &mut bool) -> Action {
    if let Some(prompt) = line.strip_prefix("PAM_PROMPT_ECHO_OFF ") {
        *was_asked = true;
        Action::Ask(unescape(prompt), false)
    } else if let Some(prompt) = line.strip_prefix("PAM_PROMPT_ECHO_ON ") {
        *was_asked = true;
        Action::Ask(unescape(prompt), true)
    } else if line.starts_with("PAM_TEXT_INFO ") {
        *was_asked = true;
        Action::Wait
    } else if line.starts_with("SUCCESS") {
        Action::Finish(true)
    } else if line.starts_with("FAILURE") {
        if *was_asked {
            Action::Retry
        } else {
            Action::Finish(false)
        }
    } else {
        Action::Wait
    }
}

fn unescape(text: &str) -> String {
    let mut output = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte != b'\\' || index >= bytes.len() {
            output.push(byte);
            continue;
        }
        let escape_code = bytes[index];
        index += 1;
        match escape_code {
            b'0'..=b'7' => {
                let mut value = (escape_code - b'0') as u32;
                for _ in 0..2 {
                    match bytes.get(index) {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + (digit - b'0') as u32;
                            index += 1;
                        }
                        _ => break,
                    }
                }
                output.push(value as u8);
            }
            b'b' => output.push(8),
            b'f' => output.push(12),
            b'n' => output.push(b'\n'),
            b'r' => output.push(b'\r'),
            b't' => output.push(b'\t'),
            b'v' => output.push(11),
            other => output.push(other),
        }
    }
    String::from_utf8_lossy(&output).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unescape_undoes_the_helpers_escapes() {
        let cases = [
            ("Password: ", "Password: "),
            ("Line\\none", "Line\none"),
            ("tab\\there", "tab\there"),
            ("quote \\\"x\\\"", "quote \"x\""),
            ("slash \\\\", "slash \\"),
            ("\\303\\251t\\303\\251", "été"),
        ];
        for (input, expected) in cases {
            assert_eq!(unescape(input), expected, "{input}");
        }
    }

    #[test]
    fn a_failure_starts_the_helper_over_only_after_the_user_was_asked_something() {
        let replies = |lines: &[&str]| {
            let mut was_asked = false;
            lines
                .iter()
                .map(|line| decide(line, &mut was_asked))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            replies(&["PAM_PROMPT_ECHO_OFF Password: ", "FAILURE"]),
            [Action::Ask("Password: ".to_owned(), false), Action::Retry]
        );
        assert_eq!(
            replies(&["PAM_TEXT_INFO Place your finger", "FAILURE"]),
            [Action::Wait, Action::Retry]
        );
        assert_eq!(
            replies(&["PAM_ERROR_MSG The account is locked", "FAILURE"]),
            [Action::Wait, Action::Finish(false)]
        );
        assert_eq!(replies(&["FAILURE"]), [Action::Finish(false)]);
        assert_eq!(replies(&["SUCCESS"]), [Action::Finish(true)]);
    }

    #[test]
    fn a_group_counts_its_listed_and_primary_members() {
        let passwd = "root:x:0:0::/root:/bin/bash\nfest:x:1000:1000::/home/fest:/bin/fish\nguest:x:1001:10::/home/guest:/bin/sh\n";
        let groups = "root:x:0:\nwheel:x:10:fest\nfest:x:1000:\n";
        assert_eq!(group_members(passwd, groups, 10), vec![1000, 1001]);
        assert_eq!(user_name(passwd, 1000).as_deref(), Some("fest"));
    }
}
