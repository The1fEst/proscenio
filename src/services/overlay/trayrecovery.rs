use gtk4::gio;
use gtk4::glib::{self, Variant, VariantTy};
use gtk4::prelude::*;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

use super::statusnotifierwatcher::{ITEM_PATH, StatusNotifierWatcher};

const SCAN_DELAY_MILLIS: u64 = 3000;
const PROBE_TIMEOUT_MILLIS: i32 = 300;
const AYATANA_ROOT: &str = "/org/ayatana/NotificationItem";
const ITEM_INTERFACE: &str = "org.kde.StatusNotifierItem";
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";

pub fn schedule(watcher: &Rc<StatusNotifierWatcher>) {
    let watcher = Rc::downgrade(watcher);
    glib::timeout_add_local_once(Duration::from_millis(SCAN_DELAY_MILLIS), move || {
        if let Some(watcher) = watcher.upgrade() {
            glib::spawn_future_local(async move { scan(&watcher).await });
        }
    });
}

fn select_probe_names(names: &[String], own_name: &str, owners: &HashSet<String>) -> Vec<String> {
    names
        .iter()
        .filter(|name| name.starts_with(':') && *name != own_name && !owners.contains(*name))
        .cloned()
        .collect()
}

async fn scan(watcher: &Rc<StatusNotifierWatcher>) {
    let connection = watcher.connection().clone();
    let Some(names) = list_names(&connection).await else {
        return;
    };
    let own_name = connection
        .unique_name()
        .map(|name| name.to_string())
        .unwrap_or_default();
    let probes: Vec<_> = select_probe_names(&names, &own_name, &watcher.owners())
        .into_iter()
        .map(|name| {
            let connection = connection.clone();
            glib::spawn_future_local(async move {
                let paths = find_item_paths(&connection, &name).await;
                (name, paths)
            })
        })
        .collect();
    for probe in probes {
        let Ok((name, paths)) = probe.await else {
            continue;
        };
        for path in paths {
            if watcher.add_item(&name, &path).await {
                eprintln!("Tray: recovered {name}{path}");
            }
        }
    }
}

async fn list_names(connection: &gio::DBusConnection) -> Option<Vec<String>> {
    let reply = connection
        .call_future(
            Some(BUS),
            BUS_PATH,
            BUS,
            "ListNames",
            None,
            Some(VariantTy::new("(as)").unwrap()),
            gio::DBusCallFlags::NONE,
            PROBE_TIMEOUT_MILLIS,
        )
        .await
        .ok()?;
    Some(
        reply
            .child_value(0)
            .iter()
            .filter_map(|name| name.str().map(str::to_owned))
            .collect(),
    )
}

async fn find_item_paths(connection: &gio::DBusConnection, name: &str) -> Vec<String> {
    if has_item(connection, name, ITEM_PATH).await {
        return vec![ITEM_PATH.to_owned()];
    }
    let mut paths = Vec::new();
    for child in list_children(connection, name, AYATANA_ROOT).await {
        let path = format!("{AYATANA_ROOT}/{child}");
        if has_item(connection, name, &path).await {
            paths.push(path);
        }
    }
    paths
}

async fn list_children(connection: &gio::DBusConnection, name: &str, path: &str) -> Vec<String> {
    let reply = connection
        .call_future(
            Some(name),
            path,
            "org.freedesktop.DBus.Introspectable",
            "Introspect",
            None,
            Some(VariantTy::new("(s)").unwrap()),
            gio::DBusCallFlags::NONE,
            PROBE_TIMEOUT_MILLIS,
        )
        .await;
    let Some(xml) = reply
        .ok()
        .and_then(|reply| reply.child_value(0).str().map(str::to_owned))
    else {
        return Vec::new();
    };
    gio::DBusNodeInfo::for_xml(&xml)
        .map(|node| {
            node.nodes()
                .iter()
                .filter_map(|child| child.path().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

async fn has_item(connection: &gio::DBusConnection, name: &str, path: &str) -> bool {
    let reply = connection
        .call_future(
            Some(name),
            path,
            "org.freedesktop.DBus.Properties",
            "GetAll",
            Some(&(ITEM_INTERFACE,).to_variant()),
            Some(VariantTy::new("(a{sv})").unwrap()),
            gio::DBusCallFlags::NONE,
            PROBE_TIMEOUT_MILLIS,
        )
        .await;
    reply.is_ok_and(|reply| has_id(&reply.child_value(0)))
}

fn has_id(properties: &Variant) -> bool {
    glib::VariantDict::new(Some(properties)).contains("Id")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_strings(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| (*name).to_owned()).collect()
    }

    #[test]
    fn only_unregistered_foreign_connections_are_probed() {
        let owners = HashSet::from([":1.7".to_owned()]);
        assert_eq!(
            select_probe_names(
                &to_strings(&[
                    "org.freedesktop.DBus",
                    "org.kde.StatusNotifierWatcher",
                    ":1.1",
                    ":1.5",
                    ":1.7",
                    ":1.9",
                ]),
                ":1.5",
                &owners,
            ),
            to_strings(&[":1.1", ":1.9"])
        );
    }
}
