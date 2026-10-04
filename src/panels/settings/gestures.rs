use gtk4::prelude::*;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::Page;
use crate::platform::gestures::{self, Gesture, Listed};
use crate::platform::hypr;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::ComboBox;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;

const ROW_HEIGHT: i32 = 48;
const ROW_START: i32 = 12;
const ROW_END: i32 = 6;
const ROW_SPACING: i32 = 10;
const REMOVE_SIZE: i32 = 32;
const REMOVE_ICON: f64 = 20.0;
const EMPTY_START: i32 = 8;

const FINGERS: [u32; 3] = [3, 4, 5];
const DIRECTIONS: [(&str, &str); 10] = [
    ("swipe", "Swipe any way"),
    ("horizontal", "Swipe left or right"),
    ("vertical", "Swipe up or down"),
    ("left", "Swipe left"),
    ("right", "Swipe right"),
    ("up", "Swipe up"),
    ("down", "Swipe down"),
    ("pinch", "Pinch in or out"),
    ("pinchin", "Pinch in"),
    ("pinchout", "Pinch out"),
];
const ACTIONS: [(&str, &str); 8] = [
    ("workspace", "Switch workspace"),
    ("move", "Move the window"),
    ("resize", "Resize the window"),
    ("close", "Close the window"),
    ("float", "Toggle floating"),
    ("fullscreen", "Toggle fullscreen"),
    ("special", "Toggle the special workspace"),
    ("scroll_move", "Scroll the layout"),
];

fn label_of(pairs: &[(&str, &str)], key: &str) -> String {
    pairs
        .iter()
        .find(|(known, _)| *known == key)
        .map_or_else(|| key.to_owned(), |(_, label)| tr(label))
}

fn title(gesture: &Gesture) -> String {
    let mods = if gesture.mods.is_empty() {
        String::new()
    } else {
        format!("{} + ", gesture.mods)
    };
    let described = trf(
        "%1 fingers · %2",
        &[
            &gesture.fingers.to_string(),
            &label_of(&DIRECTIONS, &gesture.direction),
        ],
    );
    format!("{mods}{described}")
}

fn action_label(gesture: &Gesture) -> String {
    if gesture.action.is_empty() {
        tr("Custom action")
    } else {
        label_of(&ACTIONS, &gesture.action)
    }
}

fn gesture_errors() -> Vec<String> {
    hypr::request("configerrors")
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains("hl.gesture"))
        .map(str::to_owned)
        .collect()
}

struct Gestures {
    page: Weak<Page>,
    list: gtk4::Box,
    empty: Centred,
    status: gtk4::Label,
    rows: RefCell<Vec<Box<dyn Any>>>,
}

impl Gestures {
    fn say(&self, message: &str) {
        self.status.set_text(message);
        self.status.set_visible(!message.is_empty());
    }

    fn apply(self: &Rc<Self>, settings: String) {
        let before_errors = gesture_errors();
        let (_, before) = gestures::read();
        if let Err(error) = gestures::write(&settings) {
            self.say(&trf(
                "mouse.lua could not be written: %1",
                &[&error.to_string()],
            ));
            return;
        }
        hypr::request("reload");
        let fresh: Vec<String> = gesture_errors()
            .into_iter()
            .filter(|error| !before_errors.contains(error))
            .collect();
        if let Some(error) = fresh.first() {
            let _ = gestures::write(&before);
            hypr::request("reload");
            self.say(&trf("Hyprland refused it: %1", &[error]));
        } else {
            self.say("");
        }
        self.reload();
    }

    fn remove(self: &Rc<Self>, listed: &Listed) {
        let (_, settings) = gestures::read();
        self.apply(gestures::removed(&settings, listed));
    }

    fn add(self: &Rc<Self>, gesture: &Gesture) {
        let (defaults, settings) = gestures::read();
        let list = gestures::effective(&defaults, &settings);
        if let Some(taken) = gestures::conflict(&list, gesture) {
            self.say(&trf(
                "%1 already covers this (%2). Remove it first.",
                &[&title(taken), &action_label(taken)],
            ));
            return;
        }
        self.apply(gestures::added(&settings, &defaults, gesture));
    }

    fn reload(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let (defaults, settings) = gestures::read();
        let list = gestures::effective(&defaults, &settings);
        self.empty.set_visible(list.is_empty());
        let mut child = self.empty.next_sibling();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.list.remove(&widget);
        }
        let mut rows: Vec<Box<dyn Any>> = Vec::new();
        for listed in list {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, ROW_HEIGHT);
            let inside = Row::new(ROW_SPACING);
            inside.set_margin_start(ROW_START);
            inside.set_margin_end(ROW_END);
            inside.set_hexpand(true);
            let name = text::styled(&title(&listed.gesture));
            text::set_color(&name, "colOnLayer2");
            name.set_xalign(0.0);
            name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let name = Centred::filling_width(&name);
            name.set_hexpand(true);
            inside.append(&name);
            let described = text::styled_sized(&action_label(&listed.gesture), pixel_size::SMALLER);
            text::set_color(&described, "colSubtext");
            inside.append(&Centred::new(&described));
            let remove = RippleButton::new(&page.theme);
            remove.set_radius(REMOVE_SIZE as f64 / 2.0);
            remove.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
            remove.set_valign(gtk4::Align::Center);
            let symbol = text::symbol("remove", REMOVE_ICON);
            text::set_color(&symbol, "colOnLayer2");
            remove.set_content(&Centred::integral(&symbol), 0, 0);
            remove.connect_clicked({
                let gestures = Rc::downgrade(self);
                let listed = listed.clone();
                move |_| {
                    if let Some(gestures) = gestures.upgrade() {
                        gestures.remove(&listed);
                    }
                }
            });
            inside.append(&remove);
            rows.push(Box::new(page.unkept_tip(&remove, &tr("Remove"))));
            card.append(&inside);
            self.list.append(&card);
        }
        self.rows.replace(rows);
    }
}

fn picker(page: &Page, parent: &gtk4::Box, icon: &str, names: Vec<String>) -> Rc<Cell<usize>> {
    let chosen = Rc::new(Cell::new(0usize));
    let combo = page.combo(parent, icon);
    combo.set_items(&names, 0);
    combo.connect_activated({
        let combo: Weak<ComboBox> = Rc::downgrade(&combo);
        let chosen = chosen.clone();
        move |index| {
            chosen.set(index);
            if let Some(combo) = combo.upgrade() {
                combo.set_items(&names, index as i32);
            }
        }
    });
    chosen
}

pub fn section(page: &Rc<Page>, parent: &gtk4::Box) {
    let listed = page.subsection(
        parent,
        &tr("Gestures"),
        &tr("The defaults come from hyprland/general.lua. Removing one turns it off in mouse.lua"),
    );
    let empty = text::styled(&tr("No gestures"));
    text::set_color(&empty, "colSubtext");
    let empty = Centred::new(&empty);
    empty.set_halign(gtk4::Align::Start);
    empty.set_margin_start(EMPTY_START);
    listed.append(&empty);

    let adding = page.subsection(parent, &tr("Add a gesture"), "");
    let fingers = picker(
        page,
        &adding,
        "touch_app",
        FINGERS
            .iter()
            .map(|count| trf("%1 fingers", &[&count.to_string()]))
            .collect(),
    );
    let direction = picker(
        page,
        &adding,
        "swipe",
        DIRECTIONS.iter().map(|(_, label)| tr(label)).collect(),
    );
    let action = picker(
        page,
        &adding,
        "bolt",
        ACTIONS.iter().map(|(_, label)| tr(label)).collect(),
    );
    let status = text::styled("");
    text::set_color(&status, "colError");
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_margin_start(EMPTY_START);
    status.set_visible(false);

    let gestures = Rc::new(Gestures {
        page: Rc::downgrade(page),
        list: listed,
        empty,
        status: status.clone(),
        rows: RefCell::new(Vec::new()),
    });
    gestures.reload();

    let (add, _) = page.icon_button("add", true, &tr("Add gesture"), {
        let gestures = Rc::downgrade(&gestures);
        move || {
            if let Some(gestures) = gestures.upgrade() {
                gestures.add(&Gesture {
                    fingers: FINGERS[fingers.get()],
                    direction: DIRECTIONS[direction.get()].0.to_owned(),
                    action: ACTIONS[action.get()].0.to_owned(),
                    mods: String::new(),
                });
            }
        }
    });
    adding.append(&add);
    adding.append(&status);
    page.keep(gestures);
}
