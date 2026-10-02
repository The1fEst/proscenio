use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{self, Context, Page};
use crate::panels::settings::pages::keyboard::round_button;
use crate::panels::settings::pages::quick::key;
use crate::platform::keybinds::{self, Shortcut};
use crate::ui::theme::rounding;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

const ROW_SIDE: i32 = 8;
const ROW_SPACING: i32 = 8;
const SLOT_HEIGHT: i32 = 34;
const SLOT_PADDING: i32 = 6;
const KEY_SPACING: i32 = 3;
const EMPTY_ICON: f64 = 18.0;
const DIALOG_WIDTH: f64 = 420.0;
const KEYPAD: [(&str, &str); 12] = [
    ("code:79", "KP 7"),
    ("code:80", "KP 8"),
    ("code:81", "KP 9"),
    ("code:82", "KP −"),
    ("code:83", "KP 4"),
    ("code:84", "KP 5"),
    ("code:85", "KP 6"),
    ("code:86", "KP +"),
    ("code:87", "KP 1"),
    ("code:88", "KP 2"),
    ("code:89", "KP 3"),
    ("code:90", "KP 0"),
];
const MODIFIER_KEYS: [(&str, &str); 8] = [
    ("Super_L", "SUPER"),
    ("Super_R", "SUPER"),
    ("Control_L", "CTRL"),
    ("Control_R", "CTRL"),
    ("Alt_L", "ALT"),
    ("Alt_R", "ALT"),
    ("Shift_L", "SHIFT"),
    ("Shift_R", "SHIFT"),
];
const ORDER: [&str; 4] = ["SUPER", "CTRL", "ALT", "SHIFT"];
const MOUSE_NAMES: [(&str, &str); 3] = [
    ("mouse:272", "LMB"),
    ("mouse:273", "RMB"),
    ("mouse:274", "MMB"),
];
const CATEGORIES: [&str; 9] = [
    "Shell",
    "App",
    "Window",
    "Workspace",
    "Media",
    "Utilities",
    "Screen",
    "Input",
    "Session",
];
const MODIFIER_NAMES: [(&str, &str); 4] = [
    ("SUPER", "Super"),
    ("CTRL", "Ctrl"),
    ("ALT", "Alt"),
    ("SHIFT", "Shift"),
];

pub struct Editor {
    page: Weak<Page>,
    present: Box<dyn Fn(Rc<WindowDialog>)>,
    shortcuts: RefCell<Vec<Shortcut>>,
    changed: RefCell<Vec<String>>,
    columns: [gtk4::SizeGroup; 2],
    held: RefCell<Vec<Box<dyn Any>>>,
}

impl Editor {
    pub fn new(context: &Context, page: &Rc<Page>) -> Rc<Self> {
        Rc::new(Editor {
            page: Rc::downgrade(page),
            present: Box::new(context.dialog_presenter()),
            shortcuts: RefCell::new(keybinds::load()),
            changed: RefCell::new(keybinds::changed()),
            columns: [
                gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal),
                gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal),
            ],
            held: RefCell::new(Vec::new()),
        })
    }

    pub fn reload(&self) {
        self.shortcuts.replace(keybinds::load());
        self.changed.replace(keybinds::changed());
        self.held.borrow_mut().clear();
    }

    pub fn shortcuts(&self) -> Vec<Shortcut> {
        let mut shortcuts = self.shortcuts.borrow().clone();
        shortcuts.sort_by_key(|shortcut| natural(shortcut.label()));
        shortcuts
    }

    pub fn categories(&self) -> Vec<String> {
        let mut categories: Vec<String> = Vec::new();
        for shortcut in self.shortcuts.borrow().iter() {
            let category = shortcut.category();
            if !category.is_empty() && !categories.iter().any(|known| known == category) {
                categories.push(category.to_owned());
            }
        }
        categories.sort_by_key(|category| {
            let known = CATEGORIES.iter().position(|known| known == category);
            (known.unwrap_or(CATEGORIES.len()), category.to_lowercase())
        });
        categories
    }

    pub fn row(self: &Rc<Self>, shortcut: &Shortcut) -> Row {
        let Some(page) = self.page.upgrade() else {
            return Row::new(ROW_SPACING);
        };
        let row = Row::new(ROW_SPACING);
        row.set_margin_start(ROW_SIDE);
        row.set_margin_end(ROW_SIDE);
        let label = text::styled(shortcut.label());
        text::set_color(&label, "colOnLayer1");
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let label = Centred::filling_width(&label);
        label.set_hexpand(true);
        row.append(&label);
        let changed = self.changed.borrow().contains(&shortcut.description);
        let reset = round_button(&page, "undo", true);
        reset.set_opacity(if changed { 1.0 } else { 0.0 });
        reset.set_can_target(changed);
        let tip = page.unkept_tip(&reset, &tr("Reset to default"));
        self.held.borrow_mut().push(Box::new(tip));
        reset.connect_clicked({
            let description = shortcut.description.clone();
            move |_| {
                let _ = keybinds::store(&[(description.clone(), None)]);
            }
        });
        row.append(&reset);
        for (slot, column) in self.columns.iter().enumerate() {
            let button = self.slot(&page, shortcut, slot);
            column.add_widget(&button);
            row.append(&button);
        }
        content::show_changed(&row, changed);
        row
    }

    fn slot(self: &Rc<Self>, page: &Page, shortcut: &Shortcut, slot: usize) -> RippleButton {
        let button = RippleButton::new(&page.theme);
        button.set_radius(rounding::SMALL as f64);
        button.set_size_request(-1, SLOT_HEIGHT);
        button.set_valign(gtk4::Align::Center);
        let face = gtk4::Box::new(gtk4::Orientation::Horizontal, KEY_SPACING);
        face.set_halign(gtk4::Align::End);
        match &shortcut.keys[slot] {
            Some(keys) => fill_keys(&face, keys),
            None => {
                let empty = text::symbol("add", EMPTY_ICON);
                text::set_color(&empty, "colSubtext");
                face.append(&Centred::integral(&empty));
            }
        }
        button.set_content(&face, SLOT_PADDING, 0);
        button.set_sensitive(!shortcut.mouse);
        button.connect_clicked({
            let editor = Rc::downgrade(self);
            let description = shortcut.description.clone();
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.capture(&description, slot);
                }
            }
        });
        button
    }

    fn capture(self: &Rc<Self>, description: &str, slot: usize) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let Some(shortcut) = self
            .shortcuts
            .borrow()
            .iter()
            .find(|shortcut| shortcut.description == description)
            .cloned()
        else {
            return;
        };
        let theme = &page.theme;
        let dialog = WindowDialog::new(theme, None);
        dialog.set_background_width(DIALOG_WIDTH);
        dialog.column.add(
            &windowdialog::title(&tr("Press the new shortcut")),
            Place::wide(),
        );
        let action = text::styled(shortcut.label());
        text::set_color(&action, "colOnSurfaceVariant");
        action.set_wrap(true);
        action.set_xalign(0.0);
        dialog.column.add(&action, Place::wide());
        let keys = gtk4::Box::new(gtk4::Orientation::Horizontal, KEY_SPACING);
        keys.set_halign(gtk4::Align::Center);
        keys.set_size_request(-1, SLOT_HEIGHT);
        dialog.column.add(&keys, Place::wide());
        let conflict = text::styled("");
        text::set_color(&conflict, "colError");
        conflict.set_wrap(true);
        conflict.set_xalign(0.0);
        conflict.set_visible(false);
        dialog.column.add(&conflict, Place::wide());

        let (buttons, place) = windowdialog::button_row();
        let clear = windowdialog::button(theme, &tr("Clear"));
        clear.set_sensitive(shortcut.keys[slot].is_some());
        buttons.append(&clear);
        buttons.append(&windowdialog::spacer());
        let cancel = windowdialog::button(theme, &tr("Cancel"));
        buttons.append(&cancel);
        let set = windowdialog::button(theme, &tr("Set"));
        set.set_sensitive(false);
        buttons.append(&set);
        dialog.column.add(&buttons, place);

        let waiting = || {
            let label = text::styled(&tr("Waiting for keys…"));
            text::set_color(&label, "colSubtext");
            Centred::new(&label).upcast::<gtk4::Widget>()
        };
        keys.append(&waiting());
        let pending: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        let presses = gtk4::EventControllerKey::new();
        presses.set_propagation_phase(gtk4::PropagationPhase::Capture);
        presses.connect_key_pressed({
            let editor = Rc::downgrade(self);
            let pending = pending.clone();
            let keys = keys.clone();
            let conflict = conflict.clone();
            let set = set.clone();
            let description = description.to_owned();
            move |controller, keyval, keycode, state| {
                let Some(editor) = editor.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let mut held = modifiers(state);
                let name = keyval
                    .name()
                    .map(|name| name.to_string())
                    .unwrap_or_default();
                if let Some((_, modifier)) = MODIFIER_KEYS.iter().find(|(key, _)| *key == name) {
                    if !held.contains(modifier) {
                        held.push(modifier);
                    }
                    held.sort_by_key(|name| ORDER.iter().position(|known| known == name));
                    clear_box(&keys);
                    fill_keys(&keys, &format!("{} + …", held.join(" + ")));
                    return glib::Propagation::Stop;
                }
                let Some(name) = key_name(controller, keyval, keycode) else {
                    return glib::Propagation::Stop;
                };
                let combination = held
                    .iter()
                    .copied()
                    .chain(std::iter::once(name.as_str()))
                    .collect::<Vec<_>>()
                    .join(" + ");
                clear_box(&keys);
                fill_keys(&keys, &combination);
                let owner = editor
                    .shortcuts
                    .borrow()
                    .iter()
                    .find(|other| {
                        other.description != description
                            && other
                                .keys
                                .iter()
                                .flatten()
                                .any(|keys| keybinds::same(keys, &combination))
                    })
                    .map(|other| other.label().to_owned());
                match owner {
                    Some(owner) => {
                        conflict.set_text(&trf("Used by “%1”, which loses it", &[&owner]));
                        conflict.set_visible(true);
                    }
                    None => conflict.set_visible(false),
                }
                pending.replace(Some(combination));
                set.set_sensitive(true);
                glib::Propagation::Stop
            }
        });
        dialog.root.add_controller(presses);

        let toplevel = page
            .root
            .native()
            .and_then(|native| native.surface())
            .and_then(|surface| surface.downcast::<gdk::Toplevel>().ok());
        if let Some(toplevel) = &toplevel {
            toplevel.inhibit_system_shortcuts(None::<&gdk::Event>);
        }
        dialog.connect_closed(move || {
            if let Some(toplevel) = &toplevel {
                toplevel.restore_system_shortcuts();
            }
        });

        cancel.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        clear.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let editor = Rc::downgrade(self);
            let description = description.to_owned();
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.assign(&description, slot, None);
                }
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        set.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let editor = Rc::downgrade(self);
            let description = description.to_owned();
            move |_| {
                let keys = pending.borrow().clone();
                if let (Some(editor), Some(keys)) = (editor.upgrade(), keys) {
                    editor.assign(&description, slot, Some(keys));
                }
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        (self.present)(dialog);
    }

    fn assign(&self, description: &str, slot: usize, keys: Option<String>) {
        let shortcuts = self.shortcuts.borrow();
        let Some(shortcut) = shortcuts
            .iter()
            .find(|shortcut| shortcut.description == description)
        else {
            return;
        };
        let mut slots = shortcut.keys.clone();
        slots[slot] = keys.clone();
        let mut changes = Vec::new();
        if let Some(keys) = &keys {
            let other = 1 - slot;
            if slots[other]
                .as_deref()
                .is_some_and(|held| keybinds::same(held, keys))
            {
                slots[other] = None;
            }
            for owner in shortcuts
                .iter()
                .filter(|owner| owner.description != description)
            {
                if !owner
                    .keys
                    .iter()
                    .flatten()
                    .any(|held| keybinds::same(held, keys))
                {
                    continue;
                }
                let mut left = owner.keys.clone();
                for held in left.iter_mut() {
                    if held
                        .as_deref()
                        .is_some_and(|held| keybinds::same(held, keys))
                    {
                        *held = None;
                    }
                }
                changes.push((owner.description.clone(), Some(left)));
            }
        }
        changes.insert(0, (description.to_owned(), Some(slots)));
        drop(shortcuts);
        let _ = keybinds::store(&changes);
    }
}

fn modifiers(state: gdk::ModifierType) -> Vec<&'static str> {
    [
        (gdk::ModifierType::SUPER_MASK, "SUPER"),
        (gdk::ModifierType::CONTROL_MASK, "CTRL"),
        (gdk::ModifierType::ALT_MASK, "ALT"),
        (gdk::ModifierType::SHIFT_MASK, "SHIFT"),
    ]
    .iter()
    .filter(|(mask, _)| state.contains(*mask))
    .map(|(_, name)| *name)
    .collect()
}

fn key_name(
    controller: &gtk4::EventControllerKey,
    keyval: gdk::Key,
    keycode: u32,
) -> Option<String> {
    let unshifted = controller
        .widget()
        .and_then(|widget| {
            widget.display().translate_key(
                keycode,
                gdk::ModifierType::empty(),
                controller.group() as i32,
            )
        })
        .map(|(key, ..)| key)
        .unwrap_or(keyval);
    let name = unshifted.name()?.to_string();
    let mut chars = name.chars();
    Some(match (chars.next(), chars.next()) {
        (Some(letter), None) => letter.to_uppercase().to_string(),
        _ => name,
    })
}

fn natural(label: &str) -> Vec<(String, u64)> {
    let mut parts = Vec::new();
    let mut letters = String::new();
    let mut digits = String::new();
    for char in label.to_lowercase().chars() {
        if char.is_ascii_digit() {
            digits.push(char);
            continue;
        }
        if !digits.is_empty() {
            parts.push((std::mem::take(&mut letters), digits.parse().unwrap_or(0)));
            digits.clear();
        }
        letters.push(char);
    }
    parts.push((letters, digits.parse().unwrap_or(0)));
    parts
}

fn shown(part: &str) -> String {
    KEYPAD
        .iter()
        .chain(MODIFIER_NAMES.iter())
        .chain(MOUSE_NAMES.iter())
        .find(|(written, _)| *written == part)
        .map_or_else(|| part.to_owned(), |(_, name)| (*name).to_owned())
}

fn fill_keys(face: &gtk4::Box, keys: &str) {
    for part in keys.split(" + ") {
        face.append(&key(&shown(part)));
    }
}

fn clear_box(face: &gtk4::Box) {
    while let Some(child) = face.first_child() {
        face.remove(&child);
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let category = context.argument.borrow().clone().unwrap_or_default();
    if let Some(heading) = context.heading.upgrade() {
        heading.set_text(&category);
    }
    let section = page.section("", "");
    let (list, _) = page.unkept_subsection(&section, "", "");
    let editor = Editor::new(context, &page);
    let fill = {
        let editor = editor.clone();
        move || {
            clear_box(&list);
            for shortcut in editor
                .shortcuts()
                .iter()
                .filter(|shortcut| shortcut.category() == category)
            {
                list.append(&editor.row(shortcut));
            }
        }
    };
    fill();
    page.keep(context.services.events.subscribe({
        let editor = Rc::downgrade(&editor);
        move |event, _| {
            if event != "configreloaded" {
                return;
            }
            if let Some(editor) = editor.upgrade() {
                editor.reload();
                fill();
            }
        }
    }));
    page.keep(editor);
    page
}
