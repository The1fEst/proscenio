use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::tr;
use crate::platform::grab;
use crate::ui::widgets::flow::Flow;

const NAMESPACE: &str = "proscenio:cheatsheet";
const PADDING: i32 = 20;
const CATEGORY_SPACING: i32 = 10;
const CLOSE_SIZE: i32 = 40;
const COLUMN_SPACING: i32 = 40;
const TITLE_SPACING: i32 = 7;

const BLACKLIST: [&str; 2] = ["SUPER_L", "SUPER_R"];

pub struct Cheatsheet {
    pub window: gtk4::ApplicationWindow,
    grab: Option<Rc<grab::Grab>>,
    config: Rc<Config>,
    scroll: gtk4::ScrolledWindow,
}

impl Cheatsheet {
    pub fn toggle(self: &Rc<Self>) {
        if self.window.is_visible() {
            self.hide();
            return;
        }
        self.open();
    }

    pub fn open(self: &Rc<Self>) {
        if self.window.is_visible() {
            return;
        }
        self.scroll.set_child(Some(&keybinds(&self.config)));
        self.window.set_visible(true);
        let (Some(grab), Some(surface)) = (self.grab.as_ref(), self.window.surface()) else {
            return;
        };
        let sheet = self.clone();
        grab.hold(&surface, move || sheet.hide());
    }

    pub fn hide(&self) {
        if let Some(grab) = self.grab.as_ref() {
            grab.release();
        }
        self.window.set_visible(false);
    }
}

pub fn build(
    app: &gtk4::Application,
    config: &Rc<Config>,
    monitor: &gdk::Monitor,
) -> Rc<Cheatsheet> {
    let geometry = monitor.geometry();

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_size_request(geometry.width() * 7 / 10, geometry.height() * 7 / 10);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.set_halign(gtk4::Align::Center);
    column.set_valign(gtk4::Align::Center);
    column.append(&scroll);

    let close = gtk4::Button::new();
    close.add_css_class("cheatsheet-close");
    let cross = gtk4::Label::new(Some("close"));
    cross.add_css_class("icon");
    cross.add_css_class("cheatsheet-close-icon");
    close.set_child(Some(&cross));
    close.set_size_request(CLOSE_SIZE, CLOSE_SIZE);
    close.set_halign(gtk4::Align::End);
    close.set_valign(gtk4::Align::Start);
    close.set_margin_top(PADDING);
    close.set_margin_end(PADDING);

    let card = gtk4::Overlay::new();
    card.add_css_class("cheatsheet");
    card.set_halign(gtk4::Align::Center);
    card.set_valign(gtk4::Align::Center);
    card.set_child(Some(&column));
    card.add_overlay(&close);
    column.set_margin_top(PADDING);
    column.set_margin_bottom(PADDING);
    column.set_margin_start(PADDING);
    column.set_margin_end(PADDING);

    let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    holder.set_valign(gtk4::Align::Center);
    holder.set_halign(gtk4::Align::Center);
    holder.append(&card);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&holder)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::OnDemand);
    window.set_visible(false);

    let sheet = Rc::new(Cheatsheet {
        window: window.clone(),
        grab: grab::Grab::new(&monitor.display()),
        config: config.clone(),
        scroll,
    });

    close.connect_clicked({
        let sheet = Rc::downgrade(&sheet);
        move |_| {
            if let Some(sheet) = sheet.upgrade() {
                sheet.hide();
            }
        }
    });
    let escape = gtk4::EventControllerKey::new();
    escape.connect_key_pressed({
        let sheet = Rc::downgrade(&sheet);
        move |_, key, _, _| {
            let Some(sheet) = sheet.upgrade().filter(|_| key == gdk::Key::Escape) else {
                return glib::Propagation::Proceed;
            };
            sheet.hide();
            glib::Propagation::Stop
        }
    });
    window.add_controller(escape);

    sheet
}

fn keybinds(config: &Rc<Config>) -> Flow {
    let flow = Flow::new(CATEGORY_SPACING);
    flow.set_valign(gtk4::Align::Start);

    for (name, binds) in grouped() {
        flow.append(&category(config, &name, &binds));
    }
    flow
}

struct Bind {
    modmask: i64,
    key: String,
    description: String,
}

fn grouped() -> Vec<(String, Vec<Bind>)> {
    let binds = crate::platform::hypr::json("binds")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();

    let mut groups: Vec<(String, Vec<Bind>)> = Vec::new();
    for entry in &binds {
        let description = entry
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if description.is_empty() {
            continue;
        }
        let key = entry
            .get("key")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if repeated(&key) {
            continue;
        }
        let name = match description.find(':') {
            Some(end) => description[..end].to_owned(),
            None => String::new(),
        };
        let bind = Bind {
            modmask: entry.get("modmask").and_then(Value::as_i64).unwrap_or(0),
            key,
            description,
        };
        match groups.iter_mut().find(|(held, _)| *held == name) {
            Some((_, list)) => list.push(bind),
            None => groups.push((name, vec![bind])),
        }
    }
    groups.sort_by_key(|(name, _)| name.is_empty());
    groups
}

fn category(config: &Rc<Config>, name: &str, binds: &[Bind]) -> gtk4::Widget {
    let title = gtk4::Label::new(Some(&if name.is_empty() {
        tr("Uncategorized")
    } else {
        name.to_owned()
    }));
    title.add_css_class("cheatsheet-title");
    title.set_xalign(0.0);

    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    let keys = gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal);
    for bind in binds {
        rows.append(&line(config, name, bind, &keys));
    }

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, TITLE_SPACING);
    column.append(&title);
    column.append(&rows);
    column.upcast()
}

fn line(config: &Rc<Config>, name: &str, bind: &Bind, keys: &gtk4::SizeGroup) -> gtk4::Widget {
    let caps = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    caps.set_valign(gtk4::Align::Center);
    let mods = modifiers(bind.modmask);
    let shown: Vec<String> = if config.cheatsheet_split {
        mods.iter().map(|name| substitute(config, name)).collect()
    } else if mods.is_empty() {
        Vec::new()
    } else {
        vec![
            mods.iter()
                .map(|name| substitute(config, name))
                .collect::<Vec<_>>()
                .join(" "),
        ]
    };
    for cap in &shown {
        caps.append(&key(config, cap));
    }
    let hidden = BLACKLIST.contains(&bind.key.as_str());
    if !hidden && bind.modmask > 0 {
        let plus = gtk4::Label::new(Some("+"));
        plus.add_css_class("cheatsheet-plus");
        caps.append(&plus);
    }
    if !hidden {
        caps.append(&key(config, &substitute(config, &bind.key)));
    }
    caps.set_halign(gtk4::Align::Start);
    let column = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    column.set_hexpand(false);
    column.append(&caps);
    keys.add_widget(&column);

    let comment = gtk4::Label::new(Some(&described(name, bind)));
    comment.add_css_class("cheatsheet-comment");
    comment.set_attributes(Some(&sized(config.cheatsheet_comment_size as f64)));
    comment.set_xalign(0.0);
    comment.set_valign(gtk4::Align::Center);
    comment.set_margin_end(COLUMN_SPACING);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 16);
    row.append(&column);
    row.append(&comment);
    row.upcast()
}

fn key(config: &Rc<Config>, text: &str) -> gtk4::Widget {
    let label = gtk4::Label::new(Some(text));
    label.add_css_class("cheatsheet-key-text");
    label.set_attributes(Some(&sized(config.cheatsheet_key_size as f64)));

    let face = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    face.add_css_class("cheatsheet-key-face");
    face.append(&label);
    label.set_hexpand(true);

    let cap = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    cap.add_css_class("cheatsheet-key");
    cap.set_valign(gtk4::Align::Center);
    cap.append(&face);
    cap.upcast()
}

fn sized(points: f64) -> gtk4::pango::AttrList {
    let attributes = gtk4::pango::AttrList::new();
    attributes.insert(gtk4::pango::AttrSize::new_size_absolute(
        (points * gtk4::pango::SCALE as f64).round() as i32,
    ));
    attributes
}

fn modifiers(modmask: i64) -> Vec<&'static str> {
    let mut names = Vec::new();
    for (bit, name) in [
        (2, "Ctrl"),
        (6, "Super"),
        (0, "Shift"),
        (3, "Alt"),
        (1, "Caps"),
        (4, "Mod2"),
        (5, "Mod3"),
        (7, "Mod5"),
    ] {
        if modmask & (1 << bit) != 0 {
            names.push(name);
        }
    }
    names
}

fn substitute(config: &Rc<Config>, name: &str) -> String {
    let replaced = mouse(name)
        .filter(|_| config.cheatsheet_mouse_symbols)
        .or_else(|| function(name).filter(|_| config.cheatsheet_fn_symbols))
        .or_else(|| mac(name).filter(|_| config.cheatsheet_mac_symbols))
        .unwrap_or_else(|| match name {
            "Super" if !config.cheatsheet_super.is_empty() => config.cheatsheet_super.clone(),
            "Super" => String::new(),
            "mouse_up" => "Scroll ↓".to_owned(),
            "mouse_down" => "Scroll ↑".to_owned(),
            "mouse:272" => "LMB".to_owned(),
            "mouse:273" => "RMB".to_owned(),
            "mouse:275" => "MouseBack".to_owned(),
            "Slash" => "/".to_owned(),
            "Hash" => "#".to_owned(),
            "Return" => "Enter".to_owned(),
            other => other.to_owned(),
        });
    replaced
        .replacen('1', "<Number>", 1)
        .replacen("Left", "<Direction>", 1)
}

fn mouse(name: &str) -> Option<String> {
    let symbol = match name {
        "mouse_up" => "\u{f1550}",
        "mouse_down" => "\u{f1551}",
        "mouse:272" => "L\u{f037d}",
        "mouse:273" => "R\u{f037d}",
        "Scroll ↑/↓" => "\u{f1552}",
        "Page_↑/↓" => "⇞/⇟",
        _ => return None,
    };
    Some(symbol.to_owned())
}

fn mac(name: &str) -> Option<String> {
    let symbol = match name {
        "Ctrl" => "󰘴",
        "Alt" => "󰘵",
        "Shift" => "󰘶",
        "Space" => "󱁐",
        "Tab" => "↹",
        "Equal" => "󰇼",
        "Minus" => "\u{f068}",
        "Print" => "\u{f125}",
        "BackSpace" => "󰭜",
        "Delete" => "⌦",
        "Return" => "󰌑",
        "Period" => ".",
        "Escape" => "⎋",
        _ => return None,
    };
    Some(symbol.to_owned())
}

fn function(name: &str) -> Option<String> {
    const KEYS: [(&str, &str); 12] = [
        ("F1", "󱊫"),
        ("F2", "󱊬"),
        ("F3", "󱊭"),
        ("F4", "󱊮"),
        ("F5", "󱊯"),
        ("F6", "󱊰"),
        ("F7", "󱊱"),
        ("F8", "󱊲"),
        ("F9", "󱊳"),
        ("F10", "󱊴"),
        ("F11", "󱊵"),
        ("F12", "󱊶"),
    ];
    KEYS.iter()
        .find(|(held, _)| *held == name)
        .map(|(_, symbol)| (*symbol).to_owned())
}

fn repeated(key: &str) -> bool {
    let lower = key.to_lowercase();
    if lower.contains("mouse") || lower.contains("page") {
        return false;
    }
    if key.chars().any(|letter| letter.is_ascii_digit()) && !key.contains('1') {
        return true;
    }
    ["right", "up", "down"]
        .iter()
        .any(|name| lower.starts_with(name))
}

fn described(name: &str, bind: &Bind) -> String {
    let text = match bind.description.find(':') {
        Some(end) if bind.description[..end] == *name => bind.description[end + 1..].trim(),
        _ => bind.description.as_str(),
    }
    .to_owned();

    let lower = bind.key.to_lowercase();
    if !(bind.key.contains('1') || lower.contains("left")) {
        return text;
    }
    let text = text.replacen('1', "<Number>", 1);
    for name in [" left", " right", " up", " down"] {
        if let Some(at) = text.to_lowercase().find(name) {
            let mut out = text.clone();
            out.replace_range(at..at + name.len(), " <Direction>");
            return out;
        }
    }
    text
}
