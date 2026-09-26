use gtk4::gio;
use gtk4::glib::Variant;
use gtk4::prelude::*;

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
    pub separator: bool,
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
    parse(&reply.child_value(1))
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

pub async fn clicked(session: &gio::DBusConnection, service: &str, path: &str, id: i32) {
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
        separator: text(&properties, "type") == "separator",
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
        .map(|value| value.iter().filter_map(|byte| byte.get::<u8>()).collect())
        .unwrap_or_default()
}

fn strip_mnemonics(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(current) = chars.next() {
        if current != '_' {
            out.push(current);
            continue;
        }
        if chars.peek() == Some(&'_') {
            out.push('_');
            chars.next();
        }
    }
    out
}
