use gtk4::gio;
use gtk4::glib::Variant;
use gtk4::prelude::*;
use std::collections::HashMap;

pub const INTERFACE: &str = "com.canonical.dbusmenu";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Toggle {
    None,
    Check(bool),
    Radio(bool),
}

pub struct Entry {
    pub id: i32,
    pub label: String,
    pub is_separator: bool,
    pub has_children: bool,
    pub icon_name: String,
    pub icon_data: Vec<u8>,
    pub toggle: Toggle,
    pub children: Vec<Entry>,
}

pub async fn layout(
    session: &gio::DBusConnection,
    service: &str,
    path: &str,
    parent: i32,
) -> Option<Entry> {
    let reply = session
        .call_future(
            Some(service),
            path,
            INTERFACE,
            "GetLayout",
            Some(&(parent, -1i32, Vec::<String>::new()).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
        .ok()?;
    parse(&reply.try_child_value(1)?)
}

pub async fn about_to_show(session: &gio::DBusConnection, service: &str, path: &str, id: i32) {
    let _ = session
        .call_future(
            Some(service),
            path,
            INTERFACE,
            "AboutToShow",
            Some(&(id,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await;
}

pub async fn click(session: &gio::DBusConnection, service: &str, path: &str, id: i32) {
    let data = Variant::from_variant(&0i32.to_variant());
    let arguments = Variant::tuple_from_iter([
        id.to_variant(),
        "clicked".to_variant(),
        data,
        0u32.to_variant(),
    ]);
    let _ = session
        .call_future(
            Some(service),
            path,
            INTERFACE,
            "Event",
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await;
}

fn parse(node: &Variant) -> Option<Entry> {
    if !node.is::<(i32, HashMap<String, Variant>, Vec<Variant>)>() {
        return None;
    }
    let id = node.child_value(0).get::<i32>()?;
    let properties = node.child_value(1);
    let children = node
        .child_value(2)
        .iter()
        .filter_map(|child| parse(&child.as_variant()?))
        .collect::<Vec<_>>();

    if !boolean(&properties, "visible", true) {
        return None;
    }

    let toggle_state = integer(&properties, "toggle-state", 0);
    Some(Entry {
        id,
        label: strip_mnemonics(&text(&properties, "label")),
        is_separator: text(&properties, "type") == "separator",
        has_children: text(&properties, "children-display") == "submenu",
        icon_name: text(&properties, "icon-name"),
        icon_data: bytes(&properties, "icon-data"),
        toggle: match text(&properties, "toggle-type").as_str() {
            "checkmark" => Toggle::Check(toggle_state == 1),
            "radio" => Toggle::Radio(toggle_state == 1),
            _ => Toggle::None,
        },
        children,
    })
}

fn lookup(properties: &Variant, key: &str) -> Option<Variant> {
    properties
        .iter()
        .find(|entry| entry.child_value(0).str() == Some(key))?
        .child_value(1)
        .as_variant()
}

fn text(properties: &Variant, key: &str) -> String {
    lookup(properties, key)
        .and_then(|value| value.str().map(str::to_owned))
        .unwrap_or_default()
}

fn boolean(properties: &Variant, key: &str, default: bool) -> bool {
    lookup(properties, key)
        .and_then(|value| value.get::<bool>())
        .unwrap_or(default)
}

fn integer(properties: &Variant, key: &str, default: i32) -> i32 {
    lookup(properties, key)
        .and_then(|value| value.get::<i32>())
        .unwrap_or(default)
}

fn bytes(properties: &Variant, key: &str) -> Vec<u8> {
    lookup(properties, key)
        .and_then(|value| value.fixed_array::<u8>().ok().map(<[u8]>::to_vec))
        .unwrap_or_default()
}

fn strip_mnemonics(label: &str) -> String {
    let mut output = String::with_capacity(label.len());
    let mut characters = label.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '_' {
            output.push(character);
            continue;
        }
        if characters.peek() == Some(&'_') {
            output.push('_');
            characters.next();
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: i32, properties: &[(&str, Variant)], children: Vec<Variant>) -> Variant {
        let properties: HashMap<String, Variant> = properties
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect();
        (id, properties, children).to_variant()
    }

    #[test]
    fn children_that_are_not_menu_nodes_are_skipped() {
        let valid_node = node(2, &[("label", "_Quit".to_variant())], Vec::new());
        let root = node(
            0,
            &[],
            vec!["oops".to_variant(), 7u32.to_variant(), valid_node],
        );
        let root = parse(&root).unwrap();
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].label, "Quit");
    }

    #[test]
    fn icon_data_that_is_not_bytes_is_ignored() {
        let root = node(0, &[("icon-data", "png".to_variant())], Vec::new());
        assert!(parse(&root).unwrap().icon_data.is_empty());
        let root = node(0, &[("icon-data", vec![1u8, 2].to_variant())], Vec::new());
        assert_eq!(parse(&root).unwrap().icon_data, vec![1, 2]);
    }
}
