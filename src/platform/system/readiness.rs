use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::io;
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixDatagram};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const NOTIFY_SOCKET: &str = "NOTIFY_SOCKET";
const READY_MESSAGE: &[u8] = b"READY=1";
const WAIT_TIMEOUT_MILLIS: u64 = 10_000;
const SEND_TIMEOUT_MILLIS: u64 = 1000;

#[derive(Debug, PartialEq)]
enum NotifyAddress {
    Path(PathBuf),
    Abstract(Vec<u8>),
}

fn parse_address(value: &str) -> Option<NotifyAddress> {
    if value.starts_with('/') {
        return Some(NotifyAddress::Path(PathBuf::from(value)));
    }
    let name = value.strip_prefix('@')?;
    (!name.is_empty()).then(|| NotifyAddress::Abstract(name.as_bytes().to_vec()))
}

fn send(address: &NotifyAddress, message: &[u8]) -> io::Result<()> {
    let socket = UnixDatagram::unbound()?;
    socket.set_write_timeout(Some(Duration::from_millis(SEND_TIMEOUT_MILLIS)))?;
    match address {
        NotifyAddress::Path(path) => socket.send_to(message, path)?,
        NotifyAddress::Abstract(name) => {
            socket.send_to_addr(message, &SocketAddr::from_abstract_name(name)?)?
        }
    };
    Ok(())
}

fn find_missing<'a>(
    names: &'a [String],
    owners: &HashMap<String, String>,
    own_name: &str,
) -> Vec<&'a str> {
    names
        .iter()
        .filter(|name| owners.get(*name).is_none_or(|owner| owner != own_name))
        .map(String::as_str)
        .collect()
}

struct Readiness {
    session: gio::DBusConnection,
    address: NotifyAddress,
    names: Vec<String>,
    owners: RefCell<HashMap<String, String>>,
    unwatchers: RefCell<Vec<Box<dyn FnOnce()>>>,
    is_sent: Cell<bool>,
}

pub fn report_when_ready(session: Option<&gio::DBusConnection>, names: &[&str]) {
    let Ok(value) = std::env::var(NOTIFY_SOCKET) else {
        return;
    };
    let Some(address) = parse_address(&value) else {
        eprintln!("Readiness: {NOTIFY_SOCKET} is not a socket address: {value}");
        return;
    };
    let Some(session) = session else {
        notify(&address);
        return;
    };
    let readiness = Rc::new(Readiness {
        session: session.clone(),
        address,
        names: names.iter().map(|name| (*name).to_owned()).collect(),
        owners: RefCell::new(HashMap::new()),
        unwatchers: RefCell::new(Vec::new()),
        is_sent: Cell::new(false),
    });
    for name in names {
        let readiness_for_appeared = readiness.clone();
        let readiness_for_vanished = readiness.clone();
        let id = gio::bus_watch_name_on_connection(
            session,
            name,
            gio::BusNameWatcherFlags::NONE,
            move |_, name, owner| {
                readiness_for_appeared
                    .owners
                    .borrow_mut()
                    .insert(name.to_owned(), owner.to_owned());
                readiness_for_appeared.send_if_ready();
            },
            move |_, name| {
                readiness_for_vanished.owners.borrow_mut().remove(name);
            },
        );
        readiness
            .unwatchers
            .borrow_mut()
            .push(Box::new(move || gio::bus_unwatch_name(id)));
    }
    glib::timeout_add_local_once(Duration::from_millis(WAIT_TIMEOUT_MILLIS), move || {
        if readiness.is_sent.get() {
            return;
        }
        readiness.is_sent.set(true);
        glib::spawn_future_local(async move {
            readiness.warn_missing().await;
            readiness.finish();
        });
    });
}

fn notify(address: &NotifyAddress) {
    if let Err(error) = send(address, READY_MESSAGE) {
        eprintln!("Readiness: cannot notify systemd: {error}");
    }
}

impl Readiness {
    fn send_if_ready(&self) {
        if self.is_sent.get() {
            return;
        }
        let own_name = self.own_name();
        if !find_missing(&self.names, &self.owners.borrow(), &own_name).is_empty() {
            return;
        }
        self.is_sent.set(true);
        self.finish();
    }

    fn finish(&self) {
        notify(&self.address);
        let unwatchers = self.unwatchers.take();
        glib::idle_add_local_once(move || {
            for unwatch in unwatchers {
                unwatch();
            }
        });
    }

    fn own_name(&self) -> String {
        self.session
            .unique_name()
            .map(|name| name.to_string())
            .unwrap_or_default()
    }

    async fn warn_missing(&self) {
        let own_name = self.own_name();
        let missing: Vec<String> = find_missing(&self.names, &self.owners.borrow(), &own_name)
            .into_iter()
            .map(str::to_owned)
            .collect();
        for name in missing {
            let owner = self.owners.borrow().get(&name).cloned();
            let state = match owner {
                Some(owner) => self.describe_owner(&owner).await,
                None => "has no owner".to_owned(),
            };
            eprintln!(
                "Readiness: {name} {state}; reporting ready after {WAIT_TIMEOUT_MILLIS} ms anyway"
            );
        }
    }

    async fn describe_owner(&self, owner: &str) -> String {
        let reply = self
            .session
            .call_future(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "GetConnectionUnixProcessID",
                Some(&(owner,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                2000,
            )
            .await;
        let Some(process_id) = reply
            .ok()
            .and_then(|reply| reply.child_value(0).get::<u32>())
        else {
            return format!("is held by {owner}");
        };
        let command = std::fs::read_to_string(format!("/proc/{process_id}/comm"))
            .map(|command| command.trim().to_owned())
            .unwrap_or_default();
        format!("is held by {command} (pid {process_id})")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_owners(entries: &[(&str, &str)]) -> HashMap<String, String> {
        entries
            .iter()
            .map(|(name, owner)| ((*name).to_owned(), (*owner).to_owned()))
            .collect()
    }

    #[test]
    fn notify_socket_values_become_path_or_abstract_addresses() {
        assert_eq!(
            parse_address("/run/user/1000/systemd/notify"),
            Some(NotifyAddress::Path(PathBuf::from(
                "/run/user/1000/systemd/notify"
            )))
        );
        assert_eq!(
            parse_address("@abstract/notify"),
            Some(NotifyAddress::Abstract(b"abstract/notify".to_vec()))
        );
        for value in ["", "@", "notify", "vsock:2:1234", "unix:/run/notify"] {
            assert_eq!(parse_address(value), None, "{value}");
        }
    }

    #[test]
    fn ready_is_sent_to_a_path_socket() {
        let path = std::env::temp_dir().join(format!("proscenio-notify-{}", std::process::id()));
        let receiver = UnixDatagram::bind(&path).unwrap();
        send(&NotifyAddress::Path(path.clone()), READY_MESSAGE).unwrap();
        let mut buffer = [0u8; 16];
        let length = receiver.recv(&mut buffer).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(&buffer[..length], READY_MESSAGE);
    }

    #[test]
    fn ready_is_sent_to_an_abstract_socket() {
        let name = format!("proscenio-notify-{}", std::process::id());
        let receiver =
            UnixDatagram::bind_addr(&SocketAddr::from_abstract_name(&name).unwrap()).unwrap();
        send(&parse_address(&format!("@{name}")).unwrap(), READY_MESSAGE).unwrap();
        let mut buffer = [0u8; 16];
        let length = receiver.recv(&mut buffer).unwrap();
        assert_eq!(&buffer[..length], READY_MESSAGE);
    }

    #[test]
    fn a_name_is_missing_unless_this_connection_owns_it() {
        let names = ["a".to_owned(), "b".to_owned(), "c".to_owned()];
        let owners = make_owners(&[("a", ":1.5"), ("b", ":1.9")]);
        assert_eq!(find_missing(&names, &owners, ":1.5"), ["b", "c"]);
        assert!(find_missing(&names[..1], &owners, ":1.5").is_empty());
    }
}
