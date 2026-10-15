use gtk4::gio;
use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

const NAME: &str = "org.kde.StatusNotifierWatcher";
const PATH: &str = "/StatusNotifierWatcher";
const INTERFACE: &str = "org.kde.StatusNotifierWatcher";
const ITEM_PATH: &str = "/StatusNotifierItem";
const FAILED_ERROR_NAME: &str = "org.freedesktop.DBus.Error.Failed";

const INTROSPECTION: &str = r#"<node>
  <interface name="org.kde.StatusNotifierWatcher">
    <method name="RegisterStatusNotifierItem">
      <arg name="service" type="s" direction="in"/>
    </method>
    <method name="RegisterStatusNotifierHost">
      <arg name="service" type="s" direction="in"/>
    </method>
    <property name="RegisteredStatusNotifierItems" type="as" access="read"/>
    <property name="IsStatusNotifierHostRegistered" type="b" access="read"/>
    <property name="ProtocolVersion" type="i" access="read"/>
    <signal name="StatusNotifierItemRegistered">
      <arg type="s"/>
    </signal>
    <signal name="StatusNotifierItemUnregistered">
      <arg type="s"/>
    </signal>
    <signal name="StatusNotifierHostRegistered"/>
    <signal name="StatusNotifierHostUnregistered"/>
  </interface>
</node>"#;

type Unwatch = Box<dyn FnOnce()>;

pub struct StatusNotifierWatcher {
    connection: gio::DBusConnection,
    registered_items: RefCell<Vec<String>>,
    name_watches: RefCell<HashMap<String, Unwatch>>,
}

impl StatusNotifierWatcher {
    pub fn start(session: Option<&gio::DBusConnection>) -> Option<Rc<Self>> {
        let watcher = Rc::new(StatusNotifierWatcher {
            connection: session?.clone(),
            registered_items: RefCell::new(Vec::new()),
            name_watches: RefCell::new(HashMap::new()),
        });
        watcher.export();
        Some(watcher)
    }

    fn export(self: &Rc<Self>) {
        let Ok(node) = gio::DBusNodeInfo::for_xml(INTROSPECTION) else {
            return;
        };
        let Some(interface) = node.lookup_interface(INTERFACE) else {
            return;
        };
        let watcher_for_calls = Rc::downgrade(self);
        let watcher_for_properties = Rc::downgrade(self);
        let registration_result = self
            .connection
            .register_object(PATH, &interface)
            .method_call(move |_, sender, _, _, method, parameters, invocation| {
                let Some(watcher) = watcher_for_calls.upgrade() else {
                    invocation.return_dbus_error(FAILED_ERROR_NAME, "The watcher is gone");
                    return;
                };
                let argument = parameters
                    .child_value(0)
                    .str()
                    .unwrap_or_default()
                    .to_owned();
                match method {
                    "RegisterStatusNotifierItem" => {
                        watcher.register_item(sender.unwrap_or_default(), &argument, invocation)
                    }
                    "RegisterStatusNotifierHost" => invocation.return_value(None),
                    _ => invocation.return_dbus_error(FAILED_ERROR_NAME, "Unknown method"),
                }
            })
            .property(move |_, _, _, _, name| match name {
                "IsStatusNotifierHostRegistered" => true.to_variant(),
                "ProtocolVersion" => 0i32.to_variant(),
                _ => watcher_for_properties
                    .upgrade()
                    .map(|watcher| watcher.registered_items.borrow().clone())
                    .unwrap_or_default()
                    .to_variant(),
            })
            .build();
        let Ok(registration) = registration_result else {
            return;
        };
        std::mem::forget(registration);
        gio::bus_own_name_on_connection(
            &self.connection,
            NAME,
            gio::BusNameOwnerFlags::REPLACE,
            |_, _| {},
            |_, _| {},
        );
    }

    fn register_item(
        self: &Rc<Self>,
        sender: &str,
        service_or_path: &str,
        invocation: gio::DBusMethodInvocation,
    ) {
        let (service, path) = if service_or_path.starts_with('/') {
            (sender.to_owned(), service_or_path.to_owned())
        } else {
            (service_or_path.to_owned(), ITEM_PATH.to_owned())
        };
        let id = format!("{service}{path}");
        if self.registered_items.borrow().contains(&id) {
            invocation.return_value(None);
            return;
        }
        self.watch(&service);
        let watcher = Rc::downgrade(self);
        let connection = self.connection.clone();
        glib::spawn_future_local(async move {
            let has_owner = connection
                .call_future(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    "org.freedesktop.DBus",
                    "NameHasOwner",
                    Some(&(service.as_str(),).to_variant()),
                    None,
                    gio::DBusCallFlags::NONE,
                    -1,
                )
                .await
                .ok()
                .and_then(|reply| reply.child_value(0).get::<bool>())
                .unwrap_or(false);
            if let Some(watcher) = watcher.upgrade() {
                if has_owner && !watcher.registered_items.borrow().contains(&id) {
                    watcher.registered_items.borrow_mut().push(id.clone());
                    watcher.emit("StatusNotifierItemRegistered", &id);
                } else if !has_owner {
                    watcher.unwatch(&service);
                }
            }
            invocation.return_value(None);
        });
    }

    fn watch(self: &Rc<Self>, service: &str) {
        if self.name_watches.borrow().contains_key(service) {
            return;
        }
        let watcher = Rc::downgrade(self);
        let id = gio::bus_watch_name_on_connection(
            &self.connection,
            service,
            gio::BusNameWatcherFlags::NONE,
            |_, _, _| {},
            move |_, name| {
                if let Some(watcher) = watcher.upgrade() {
                    watcher.on_name_vanished(name);
                }
            },
        );
        self.name_watches.borrow_mut().insert(
            service.to_owned(),
            Box::new(move || gio::bus_unwatch_name(id)),
        );
    }

    fn unwatch(&self, service: &str) {
        let unwatch = self.name_watches.borrow_mut().remove(service);
        if let Some(unwatch) = unwatch {
            unwatch();
        }
    }

    fn on_name_vanished(&self, name: &str) {
        self.unwatch(name);
        let prefix = format!("{name}/");
        let mut removed_ids = Vec::new();
        self.registered_items.borrow_mut().retain(|id| {
            let should_keep = !id.starts_with(&prefix);
            if !should_keep {
                removed_ids.push(id.clone());
            }
            should_keep
        });
        for id in removed_ids {
            self.emit("StatusNotifierItemUnregistered", &id);
        }
    }

    fn emit(&self, signal: &str, id: &str) {
        let parameters: Variant = (id,).to_variant();
        let _ = self
            .connection
            .emit_signal(None, PATH, INTERFACE, signal, Some(&parameters));
    }
}
