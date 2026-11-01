use gtk4::glib::{Variant, variant::ObjectPath};
use gtk4::prelude::*;
use std::collections::HashMap;

use super::model::{
    BUS, INTERFACES, Objects, Properties, ROOT, SETTINGS, SETTINGS_PATH, WIRELESS_DEVICE,
};

fn keep_known(interfaces: HashMap<String, Properties>) -> HashMap<String, Properties> {
    interfaces
        .into_iter()
        .filter(|(name, _)| INTERFACES.contains(&name.as_str()))
        .collect()
}

pub fn read_managed_objects(objects: &Variant) -> Objects {
    let mut known_objects = Objects::new();
    for entry in objects.iter() {
        let Some(path) = entry.child_value(0).get::<ObjectPath>() else {
            continue;
        };
        let Some(interfaces) = entry.child_value(1).get::<HashMap<String, Properties>>() else {
            continue;
        };
        let interfaces = keep_known(interfaces);
        if !interfaces.is_empty() {
            known_objects.insert(path.as_str().to_owned(), interfaces);
        }
    }
    known_objects
}

pub fn apply_properties_changed(objects: &mut Objects, path: &str, parameters: &Variant) -> bool {
    let Some((interface, changed, invalidated)) =
        parameters.get::<(String, Properties, Vec<String>)>()
    else {
        return false;
    };
    let Some(properties) = objects
        .get_mut(path)
        .and_then(|interfaces| interfaces.get_mut(&interface))
    else {
        return false;
    };
    properties.extend(changed);
    for name in invalidated {
        properties.remove(&name);
    }
    true
}

pub fn apply_interfaces_added(objects: &mut Objects, parameters: &Variant) -> Option<String> {
    let (path, interfaces) = parameters.get::<(ObjectPath, HashMap<String, Properties>)>()?;
    let interfaces = keep_known(interfaces);
    if interfaces.is_empty() {
        return None;
    }
    let path = path.as_str().to_owned();
    objects.entry(path.clone()).or_default().extend(interfaces);
    Some(path)
}

fn edit_access_points(
    objects: &mut Objects,
    device: &str,
    parameters: &Variant,
    edit: impl FnOnce(&mut Vec<ObjectPath>, ObjectPath) -> bool,
) -> bool {
    let Some((point,)) = parameters.get::<(ObjectPath,)>() else {
        return false;
    };
    let Some(wireless) = objects
        .get_mut(device)
        .and_then(|interfaces| interfaces.get_mut(WIRELESS_DEVICE))
    else {
        return false;
    };
    let mut points: Vec<ObjectPath> = wireless
        .get("AccessPoints")
        .and_then(Variant::get)
        .unwrap_or_default();
    if !edit(&mut points, point) {
        return false;
    }
    wireless.insert("AccessPoints".to_owned(), points.to_variant());
    true
}

pub fn apply_access_point_added(objects: &mut Objects, device: &str, parameters: &Variant) -> bool {
    edit_access_points(objects, device, parameters, |points, point| {
        if points.contains(&point) {
            return false;
        }
        points.push(point);
        true
    })
}

pub fn apply_access_point_removed(
    objects: &mut Objects,
    device: &str,
    parameters: &Variant,
) -> bool {
    edit_access_points(objects, device, parameters, |points, point| {
        let count = points.len();
        points.retain(|known| *known != point);
        points.len() != count
    })
}

pub fn list_missing_objects(objects: &Objects) -> Vec<String> {
    let references = [
        (ROOT, BUS, "Devices"),
        (ROOT, BUS, "ActiveConnections"),
        (SETTINGS_PATH, SETTINGS, "Connections"),
    ]
    .into_iter()
    .filter_map(|(path, interface, name)| objects.get(path)?.get(interface)?.get(name))
    .chain(
        objects
            .values()
            .filter_map(|interfaces| interfaces.get(WIRELESS_DEVICE)?.get("AccessPoints")),
    );
    let mut missing: Vec<String> = references
        .filter_map(|paths| paths.get::<Vec<ObjectPath>>())
        .flatten()
        .map(|path| path.as_str().to_owned())
        .filter(|path| !objects.contains_key(path))
        .collect();
    missing.sort();
    missing.dedup();
    missing
}

pub fn apply_interfaces_removed(objects: &mut Objects, parameters: &Variant) -> Option<String> {
    let (path, names) = parameters.get::<(ObjectPath, Vec<String>)>()?;
    let interfaces = objects.get_mut(path.as_str())?;
    let count = interfaces.len();
    for name in names {
        interfaces.remove(&name);
    }
    let has_changed = interfaces.len() != count;
    if interfaces.is_empty() {
        objects.remove(path.as_str());
    }
    has_changed.then(|| path.as_str().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::network::networkmanager::model::tests::new_paths;
    use crate::services::network::networkmanager::model::tests::{
        insert, new_path, new_properties,
    };
    use crate::services::network::networkmanager::model::{
        ACCESS_POINT, ACTIVE_CONNECTION, DEVICE, SETTINGS_CONNECTION, Snapshot,
    };

    const POINT: &str = "/org/freedesktop/NetworkManager/AccessPoint/4";

    fn new_point_interfaces(strength: u8) -> HashMap<String, Properties> {
        HashMap::from([
            (
                ACCESS_POINT.to_owned(),
                new_properties(&[("Strength", strength.to_variant())]),
            ),
            ("org.freedesktop.DBus.Peer".to_owned(), Properties::new()),
        ])
    }

    #[test]
    fn managed_objects_keep_only_the_known_interfaces() {
        let reply = HashMap::from([
            (
                ObjectPath::try_from(POINT.to_owned()).unwrap(),
                new_point_interfaces(60),
            ),
            (
                ObjectPath::try_from("/org/freedesktop/NetworkManager/IP4Config/1".to_owned())
                    .unwrap(),
                HashMap::from([(
                    "org.freedesktop.NetworkManager.IP4Config".to_owned(),
                    Properties::new(),
                )]),
            ),
        ]);
        let objects = read_managed_objects(&reply.to_variant());
        assert_eq!(objects.keys().collect::<Vec<_>>(), [POINT]);
        assert_eq!(objects[POINT].keys().collect::<Vec<_>>(), [ACCESS_POINT]);
    }

    #[test]
    fn changed_properties_merge_into_a_known_object_only() {
        let mut objects = Objects::new();
        insert(
            &mut objects,
            ROOT,
            BUS,
            &[
                ("WirelessEnabled", false.to_variant()),
                ("State", 70u32.to_variant()),
            ],
        );
        let new_change = |interface: &str, is_enabled: bool| {
            (
                interface,
                new_properties(&[("WirelessEnabled", is_enabled.to_variant())]),
                vec!["State".to_owned()],
            )
                .to_variant()
        };
        assert!(apply_properties_changed(
            &mut objects,
            ROOT,
            &new_change(BUS, true)
        ));
        assert_eq!(
            objects[ROOT][BUS],
            new_properties(&[("WirelessEnabled", true.to_variant())])
        );
        assert!(!apply_properties_changed(
            &mut objects,
            ROOT,
            &new_change(DEVICE, false)
        ));
        assert!(!apply_properties_changed(
            &mut objects,
            POINT,
            &new_change(BUS, false)
        ));
        assert!(!apply_properties_changed(
            &mut objects,
            ROOT,
            &true.to_variant()
        ));
        assert_eq!(objects.len(), 1);
    }

    #[test]
    fn objects_come_and_go_with_their_interfaces() {
        let mut objects = Objects::new();
        let new_point_path = || ObjectPath::try_from(POINT.to_owned()).unwrap();
        assert_eq!(
            apply_interfaces_added(
                &mut objects,
                &(new_point_path(), new_point_interfaces(30)).to_variant()
            ),
            Some(POINT.to_owned())
        );
        assert_eq!(
            objects[POINT][ACCESS_POINT],
            new_properties(&[("Strength", 30u8.to_variant())])
        );
        let peer_interfaces =
            HashMap::from([("org.freedesktop.DBus.Peer".to_owned(), Properties::new())]);
        assert_eq!(
            apply_interfaces_added(
                &mut objects,
                &(new_point_path(), peer_interfaces).to_variant()
            ),
            None
        );
        let settings = "/org/freedesktop/NetworkManager/Settings/2";
        insert(
            &mut objects,
            settings,
            SETTINGS_CONNECTION,
            &[("Unsaved", false.to_variant())],
        );
        let new_removal = |object: &str, names: &[&str]| {
            (
                ObjectPath::try_from(object.to_owned()).unwrap(),
                names
                    .iter()
                    .map(|name| (*name).to_owned())
                    .collect::<Vec<_>>(),
            )
                .to_variant()
        };
        assert_eq!(
            apply_interfaces_removed(
                &mut objects,
                &new_removal(POINT, &["org.freedesktop.DBus.Peer"])
            ),
            None
        );
        assert_eq!(
            apply_interfaces_removed(&mut objects, &new_removal(POINT, &[ACCESS_POINT])),
            Some(POINT.to_owned())
        );
        assert_eq!(objects.keys().collect::<Vec<_>>(), [settings]);
        assert_eq!(
            apply_interfaces_removed(&mut objects, &new_path(POINT)),
            None
        );
    }

    #[test]
    fn an_access_point_found_after_the_load_joins_its_device() {
        let device = "/org/freedesktop/NetworkManager/Devices/3";
        let mut objects = Objects::new();
        insert(
            &mut objects,
            device,
            DEVICE,
            &[("DeviceType", 2u32.to_variant())],
        );
        insert(
            &mut objects,
            device,
            WIRELESS_DEVICE,
            &[("AccessPoints", new_paths(&[]))],
        );
        let interfaces = HashMap::from([(
            ACCESS_POINT.to_owned(),
            new_properties(&[
                ("Ssid", b"Testbed Open".to_variant()),
                ("Strength", 70u8.to_variant()),
            ]),
        )]);
        let point = || ObjectPath::try_from(POINT.to_owned()).unwrap();
        apply_interfaces_added(&mut objects, &(point(), interfaces).to_variant());
        assert!(apply_access_point_added(
            &mut objects,
            device,
            &(point(),).to_variant()
        ));
        assert!(!apply_access_point_added(
            &mut objects,
            device,
            &(point(),).to_variant()
        ));
        let read_names = |objects: &Objects| -> Vec<String> {
            Snapshot::new(objects, &HashMap::new()).devices[0]
                .access_points
                .iter()
                .map(|point| point.ssid.clone())
                .collect()
        };
        assert_eq!(read_names(&objects), ["Testbed Open"]);
        assert!(apply_access_point_removed(
            &mut objects,
            device,
            &(point(),).to_variant()
        ));
        assert!(read_names(&objects).is_empty());
    }

    #[test]
    fn referenced_objects_the_cache_lacks_are_missing() {
        let device = "/org/freedesktop/NetworkManager/Devices/2";
        let mut objects = Objects::new();
        insert(
            &mut objects,
            ROOT,
            BUS,
            &[
                (
                    "Devices",
                    new_paths(&[device, "/org/freedesktop/NetworkManager/Devices/9"]),
                ),
                (
                    "ActiveConnections",
                    new_paths(&["/org/freedesktop/NetworkManager/ActiveConnection/3"]),
                ),
            ],
        );
        insert(
            &mut objects,
            SETTINGS_PATH,
            SETTINGS,
            &[(
                "Connections",
                new_paths(&["/org/freedesktop/NetworkManager/Settings/4"]),
            )],
        );
        insert(&mut objects, device, DEVICE, &[]);
        insert(
            &mut objects,
            device,
            WIRELESS_DEVICE,
            &[("AccessPoints", new_paths(&[POINT]))],
        );
        assert_eq!(
            list_missing_objects(&objects),
            [
                "/org/freedesktop/NetworkManager/AccessPoint/4",
                "/org/freedesktop/NetworkManager/ActiveConnection/3",
                "/org/freedesktop/NetworkManager/Devices/9",
                "/org/freedesktop/NetworkManager/Settings/4",
            ]
        );
        insert(&mut objects, POINT, ACCESS_POINT, &[]);
        for (path, interface) in [
            (
                "/org/freedesktop/NetworkManager/ActiveConnection/3",
                ACTIVE_CONNECTION,
            ),
            ("/org/freedesktop/NetworkManager/Devices/9", DEVICE),
            (
                "/org/freedesktop/NetworkManager/Settings/4",
                SETTINGS_CONNECTION,
            ),
        ] {
            insert(&mut objects, path, interface, &[]);
        }
        assert!(list_missing_objects(&objects).is_empty());
    }
}
