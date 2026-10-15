use gtk4::prelude::*;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf, trn};
use crate::panels::settings::content::Page;
use crate::platform::hyprland::gestures::{self, Gesture, GestureEntry};
use crate::platform::hyprland::hypr;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ComboBox;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

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
    format!(
        "{mods}{} · {}",
        trn("%1 finger", "%1 fingers", u64::from(gesture.fingers)),
        label_of(&DIRECTIONS, &gesture.direction)
    )
}

fn action_label(gesture: &Gesture) -> String {
    if gesture.action.is_empty() {
        tr("Custom action")
    } else {
        label_of(&ACTIONS, &gesture.action)
    }
}

fn format_write_error(error: &std::io::Error) -> String {
    trf("mouse.lua could not be written: %1", &[&error.to_string()])
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
    list_section: gtk4::Box,
    empty_notice: Centered,
    status: gtk4::Label,
    remove_tips: RefCell<Vec<Box<dyn Any>>>,
}

impl Gestures {
    fn say(&self, message: &str) {
        self.status.set_text(message);
        self.status.set_visible(!message.is_empty());
    }

    fn read_settings(&self) -> (String, Option<String>) {
        let (defaults, settings) = gestures::read();
        let settings = settings
            .inspect_err(|error| {
                self.say(&trf(
                    "mouse.lua could not be read: %1",
                    &[&error.to_string()],
                ));
            })
            .ok();
        (defaults, settings)
    }

    fn show_write_error(&self, error: &std::io::Error) {
        self.say(&format_write_error(error));
    }

    fn apply(self: &Rc<Self>, previous_settings: &str, settings: String) {
        let previous_errors = gesture_errors();
        if let Err(error) = gestures::write(&settings) {
            self.show_write_error(&error);
            return;
        }
        hypr::request("reload");
        let new_errors: Vec<String> = gesture_errors()
            .into_iter()
            .filter(|error| !previous_errors.contains(error))
            .collect();
        if let Some(error) = new_errors.first() {
            let restore_result = gestures::write(previous_settings);
            hypr::request("reload");
            let refusal = match restore_result {
                Ok(()) => error.clone(),
                Err(restore_error) => format!("{error}; {}", format_write_error(&restore_error)),
            };
            self.say(&trf("Hyprland refused it: %1", &[&refusal]));
        } else {
            self.say("");
        }
        self.reload();
    }

    fn remove(self: &Rc<Self>, entry: &GestureEntry) {
        let (_, Some(settings)) = self.read_settings() else {
            return;
        };
        self.apply(&settings, gestures::with_gesture_removed(&settings, entry));
    }

    fn add(self: &Rc<Self>, gesture: &Gesture) {
        let (defaults, Some(settings)) = self.read_settings() else {
            return;
        };
        let list = gestures::parse_entries(&defaults, &settings);
        if let Some(conflicting) = gestures::find_conflict(&list, gesture) {
            self.say(&trf(
                "%1 already covers this (%2). Remove it first.",
                &[&title(conflicting), &action_label(conflicting)],
            ));
            return;
        }
        self.apply(
            &settings,
            gestures::with_gesture_added(&settings, &defaults, gesture),
        );
    }

    fn reload(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let (defaults, settings) = self.read_settings();
        let settings = settings.unwrap_or_default();
        let list = gestures::parse_entries(&defaults, &settings);
        self.empty_notice.set_visible(list.is_empty());
        let mut child = self.empty_notice.next_sibling();
        while let Some(widget) = child {
            child = widget.next_sibling();
            self.list_section.remove(&widget);
        }
        let mut remove_tips: Vec<Box<dyn Any>> = Vec::new();
        for entry in list {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, ROW_HEIGHT);
            let content = Row::new(ROW_SPACING);
            content.set_margin_start(ROW_START);
            content.set_margin_end(ROW_END);
            content.set_hexpand(true);
            let name_label = text::styled(&title(&entry.gesture));
            text::set_color(&name_label, "colOnLayer2");
            name_label.set_xalign(0.0);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            let name_box = Centered::filling_width(&name_label);
            name_box.set_hexpand(true);
            content.append(&name_box);
            let description =
                text::styled_sized(&action_label(&entry.gesture), pixel_size::SMALLER);
            text::set_color(&description, "colSubtext");
            content.append(&Centered::new(&description));
            let remove_button = RippleButton::new(&page.theme);
            remove_button.set_radius(REMOVE_SIZE as f64 / 2.0);
            remove_button.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
            remove_button.set_valign(gtk4::Align::Center);
            let symbol = text::symbol("remove", REMOVE_ICON);
            text::set_color(&symbol, "colOnLayer2");
            remove_button.set_content(&Centered::integral(&symbol), 0, 0);
            remove_button.connect_clicked({
                let gestures = Rc::downgrade(self);
                let entry = entry.clone();
                move |_| {
                    if let Some(gestures) = gestures.upgrade() {
                        gestures.remove(&entry);
                    }
                }
            });
            content.append(&remove_button);
            remove_tips.push(Box::new(page.unkept_tip(&remove_button, &tr("Remove"))));
            card.append(&content);
            self.list_section.append(&card);
        }
        self.remove_tips.replace(remove_tips);
    }
}

fn picker(page: &Page, parent: &gtk4::Box, icon: &str, names: Vec<String>) -> Rc<Cell<usize>> {
    let selected_index = Rc::new(Cell::new(0usize));
    let combo = page.combo(parent, icon);
    combo.set_items(&names, 0);
    combo.connect_activated({
        let combo: Weak<ComboBox> = Rc::downgrade(&combo);
        let selected_index = selected_index.clone();
        move |index| {
            selected_index.set(index);
            if let Some(combo) = combo.upgrade() {
                combo.set_items(&names, index as i32);
            }
        }
    });
    selected_index
}

pub fn section(page: &Rc<Page>, parent: &gtk4::Box) {
    let list_section = page.subsection(
        parent,
        &tr("Gestures"),
        &tr("The defaults come from hyprland/general.lua. Removing one turns it off in mouse.lua"),
    );
    let empty_notice = text::styled(&tr("No gestures"));
    text::set_color(&empty_notice, "colSubtext");
    let empty_notice = Centered::new(&empty_notice);
    empty_notice.set_halign(gtk4::Align::Start);
    empty_notice.set_margin_start(EMPTY_START);
    list_section.append(&empty_notice);

    let add_section = page.subsection(parent, &tr("Add a gesture"), "");
    let fingers = picker(
        page,
        &add_section,
        "touch_app",
        FINGERS
            .iter()
            .map(|count| trn("%1 finger", "%1 fingers", u64::from(*count)))
            .collect(),
    );
    let direction = picker(
        page,
        &add_section,
        "swipe",
        DIRECTIONS.iter().map(|(_, label)| tr(label)).collect(),
    );
    let action = picker(
        page,
        &add_section,
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
        list_section,
        empty_notice,
        status: status.clone(),
        remove_tips: RefCell::new(Vec::new()),
    });
    gestures.reload();

    let (add_button, _) = page.icon_button("add", true, &tr("Add gesture"), {
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
    add_section.append(&add_button);
    add_section.append(&status);
    page.keep(gestures);
}
