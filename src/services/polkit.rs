use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::os::unix::fs::MetadataExt;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};

const INTERFACE: &str = "org.freedesktop.PolicyKit1.AuthenticationAgent";
const PATH: &str = "/dev/fEst/Proscenio/PolkitAgent";
const AUTHORITY: &str = "org.freedesktop.PolicyKit1";
const AUTHORITY_PATH: &str = "/org/freedesktop/PolicyKit1/Authority";
const AUTHORITY_INTERFACE: &str = "org.freedesktop.PolicyKit1.Authority";
const HELPER_SOCKET: &str = "/run/polkit/agent-helper.socket";
const HELPER: &str = "/usr/lib/polkit-1/polkit-agent-helper-1";
const CANCELLED: &str = "org.freedesktop.PolicyKit1.Error.Cancelled";
const FAILED: &str = "org.freedesktop.PolicyKit1.Error.Failed";

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
    echo: Cell<bool>,
    interaction: Cell<bool>,
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

    pub fn response_visible(&self) -> bool {
        self.echo.get()
    }

    pub fn interaction_available(&self) -> bool {
        self.interaction.get()
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

    pub fn submit(self: &Rc<Self>, response: &str) {
        let Some(flow) = self.flow() else {
            return;
        };
        let Some(input) = flow.input.borrow().clone() else {
            return;
        };
        flow.interaction.set(false);
        self.announce();
        let line = format!("{response}\n");
        let _ = input.write_all(line.as_bytes(), gio::Cancellable::NONE);
    }

    pub fn cancel(self: &Rc<Self>) {
        let Some(flow) = self.flow.take() else {
            return;
        };
        flow.cancellable.borrow().cancel();
        close(&flow);
        if let Some(invocation) = flow.invocation.take() {
            invocation.return_dbus_error(CANCELLED, "Authentication was cancelled");
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
        let registered = system
            .register_object(PATH, &interface)
            .method_call(move |_, _, _, _, method, parameters, invocation| {
                let Some(polkit) = polkit.upgrade() else {
                    invocation.return_dbus_error(FAILED, "The agent is gone");
                    return;
                };
                match method {
                    "BeginAuthentication" => polkit.begin(&parameters, invocation),
                    "CancelAuthentication" => {
                        let cookie = parameters
                            .child_value(0)
                            .get::<String>()
                            .unwrap_or_default();
                        let current = polkit.flow().is_some_and(|flow| flow.cookie == cookie);
                        if current {
                            polkit.cancel();
                        }
                        invocation.return_value(None);
                    }
                    _ => invocation.return_dbus_error(FAILED, "Unknown method"),
                }
            })
            .build();
        if registered.is_err() {
            return;
        }
        register(system);
    }

    fn begin(self: &Rc<Self>, parameters: &glib::Variant, invocation: gio::DBusMethodInvocation) {
        if self.flow().is_some() {
            invocation.return_dbus_error(FAILED, "Another authentication is in progress");
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
            invocation.return_dbus_error(FAILED, "No identity this agent can authenticate");
            return;
        };
        let flow = Rc::new(Flow {
            message,
            prompt: RefCell::new(String::new()),
            echo: Cell::new(false),
            interaction: Cell::new(false),
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
                let line = line.to_string();
                if let Some(prompt) = line.strip_prefix("PAM_PROMPT_ECHO_OFF ") {
                    polkit.ask(&flow, &compress(prompt), false);
                } else if let Some(prompt) = line.strip_prefix("PAM_PROMPT_ECHO_ON ") {
                    polkit.ask(&flow, &compress(prompt), true);
                } else if line.starts_with("SUCCESS") {
                    polkit.finish(&flow, true);
                    return;
                } else if line.starts_with("FAILURE") {
                    polkit.retry(&flow);
                    return;
                }
            }
        });
    }

    fn ask(&self, flow: &Rc<Flow>, prompt: &str, echo: bool) {
        flow.prompt.replace(prompt.to_owned());
        flow.echo.set(echo);
        flow.interaction.set(true);
        self.announce();
    }

    fn retry(self: &Rc<Self>, flow: &Rc<Flow>) {
        close(flow);
        self.start(flow);
    }

    fn finish(&self, flow: &Rc<Flow>, success: bool) {
        if !self.is_current(flow) {
            return;
        }
        close(flow);
        self.flow.replace(None);
        if let Some(invocation) = flow.invocation.take() {
            if success {
                invocation.return_value(None);
            } else {
                invocation.return_dbus_error(FAILED, "Authentication failed");
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
    let Some(session) = session_id(system) else {
        eprintln!("polkit: no login session to register the agent for");
        return;
    };
    let details = HashMap::from([("session-id".to_owned(), session.to_variant())]);
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
    let own = std::fs::metadata("/proc/self").ok()?.uid();
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
        .find(|uid| *uid == own)
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

fn compress(text: &str) -> String {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        index += 1;
        if byte != b'\\' || index >= bytes.len() {
            out.push(byte);
            continue;
        }
        let escaped = bytes[index];
        index += 1;
        match escaped {
            b'0'..=b'7' => {
                let mut value = (escaped - b'0') as u32;
                for _ in 0..2 {
                    match bytes.get(index) {
                        Some(digit @ b'0'..=b'7') => {
                            value = value * 8 + (digit - b'0') as u32;
                            index += 1;
                        }
                        _ => break,
                    }
                }
                out.push(value as u8);
            }
            b'b' => out.push(8),
            b'f' => out.push(12),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b't' => out.push(b'\t'),
            b'v' => out.push(11),
            other => out.push(other),
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compress_undoes_the_helpers_escapes() {
        let cases = [
            ("Password: ", "Password: "),
            ("Line\\none", "Line\none"),
            ("tab\\there", "tab\there"),
            ("quote \\\"x\\\"", "quote \"x\""),
            ("slash \\\\", "slash \\"),
            ("\\303\\251t\\303\\251", "été"),
        ];
        for (escaped, expected) in cases {
            assert_eq!(compress(escaped), expected, "{escaped}");
        }
    }

    #[test]
    fn a_group_counts_its_listed_and_primary_members() {
        let passwd = "root:x:0:0::/root:/bin/bash\nfest:x:1000:1000::/home/fest:/bin/fish\nguest:x:1001:10::/home/guest:/bin/sh\n";
        let groups = "root:x:0:\nwheel:x:10:fest\nfest:x:1000:\n";
        assert_eq!(group_members(passwd, groups, 10), vec![1000, 1001]);
        assert_eq!(user_name(passwd, 1000).as_deref(), Some("fest"));
    }
}
