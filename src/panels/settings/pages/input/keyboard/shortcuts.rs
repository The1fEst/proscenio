use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{self, Context, Page};
use crate::panels::settings::pages::input::keyboard::round_button;
use crate::panels::settings::pages::quick::key;
use crate::platform::hyprland::keybinds::{self, Shortcut};
use crate::ui::theme::rounding;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

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
    changed_descriptions: RefCell<Vec<String>>,
    columns: [gtk4::SizeGroup; 2],
    tips: RefCell<Vec<Box<dyn Any>>>,
}

impl Editor {
    pub fn new(context: &Context, page: &Rc<Page>) -> Rc<Self> {
        Rc::new(Editor {
            page: Rc::downgrade(page),
            present: Box::new(context.dialog_presenter()),
            shortcuts: RefCell::new(keybinds::load()),
            changed_descriptions: RefCell::new(keybinds::read_changed_descriptions()),
            columns: [
                gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal),
                gtk4::SizeGroup::new(gtk4::SizeGroupMode::Horizontal),
            ],
            tips: RefCell::new(Vec::new()),
        })
    }

    pub fn reload(&self) {
        self.shortcuts.replace(keybinds::load());
        self.changed_descriptions
            .replace(keybinds::read_changed_descriptions());
    }

    pub fn clear_tips(&self) {
        self.tips.borrow_mut().clear();
    }

    pub fn shortcuts(&self) -> Vec<Shortcut> {
        let mut shortcuts = self.shortcuts.borrow().clone();
        shortcuts.sort_by_key(|shortcut| natural_sort_key(shortcut.label()));
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
            let category_index = CATEGORIES.iter().position(|known| known == category);
            (
                category_index.unwrap_or(CATEGORIES.len()),
                category.to_lowercase(),
            )
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
        let label_box = Centered::filling_width(&label);
        label_box.set_hexpand(true);
        row.append(&label_box);
        let is_changed = self
            .changed_descriptions
            .borrow()
            .contains(&shortcut.description);
        let reset_button = round_button(&page, "undo", true);
        reset_button.set_opacity(if is_changed { 1.0 } else { 0.0 });
        reset_button.set_can_target(is_changed);
        let tip = page.unkept_tip(&reset_button, &tr("Reset to default"));
        self.tips.borrow_mut().push(Box::new(tip));
        reset_button.connect_clicked({
            let description = shortcut.description.clone();
            move |_| {
                let _ = keybinds::store(&[(description.clone(), None)]);
            }
        });
        row.append(&reset_button);
        for (slot, column) in self.columns.iter().enumerate() {
            let button = self.slot(&page, shortcut, slot);
            column.add_widget(&button);
            row.append(&button);
        }
        content::show_changed(&row, is_changed);
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
                let symbol = text::symbol("add", EMPTY_ICON);
                text::set_color(&symbol, "colSubtext");
                face.append(&Centered::integral(&symbol));
            }
        }
        button.set_content(&face, SLOT_PADDING, 0);
        button.set_sensitive(!shortcut.is_mouse);
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
        let action_label = text::styled(shortcut.label());
        text::set_color(&action_label, "colOnSurfaceVariant");
        action_label.set_wrap(true);
        action_label.set_xalign(0.0);
        dialog.column.add(&action_label, Place::wide());
        let keys_box = gtk4::Box::new(gtk4::Orientation::Horizontal, KEY_SPACING);
        keys_box.set_halign(gtk4::Align::Center);
        keys_box.set_size_request(-1, SLOT_HEIGHT);
        dialog.column.add(&keys_box, Place::wide());
        let conflict_label = text::styled("");
        text::set_color(&conflict_label, "colError");
        conflict_label.set_wrap(true);
        conflict_label.set_xalign(0.0);
        conflict_label.set_visible(false);
        dialog.column.add(&conflict_label, Place::wide());

        let (buttons, place) = windowdialog::button_row();
        let clear_button = windowdialog::button(theme, &tr("Clear"));
        clear_button.set_sensitive(shortcut.keys[slot].is_some());
        buttons.append(&clear_button);
        buttons.append(&windowdialog::spacer());
        let cancel_button = windowdialog::button(theme, &tr("Cancel"));
        buttons.append(&cancel_button);
        let set_button = windowdialog::button(theme, &tr("Set"));
        set_button.set_sensitive(false);
        buttons.append(&set_button);
        dialog.column.add(&buttons, place);

        let waiting_placeholder = || {
            let label = text::styled(&tr("Waiting for keys…"));
            text::set_color(&label, "colSubtext");
            Centered::new(&label).upcast::<gtk4::Widget>()
        };
        keys_box.append(&waiting_placeholder());
        let pending_keys: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let editor = Rc::downgrade(self);
            let pending_keys = pending_keys.clone();
            let keys_box = keys_box.clone();
            let conflict_label = conflict_label.clone();
            let set_button = set_button.clone();
            let description = description.to_owned();
            move |controller, keyval, keycode, state| {
                let Some(editor) = editor.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let mut held_modifiers = modifiers(state);
                let name = keyval
                    .name()
                    .map(|name| name.to_string())
                    .unwrap_or_default();
                if let Some((_, modifier)) = MODIFIER_KEYS.iter().find(|(key, _)| *key == name) {
                    if !held_modifiers.contains(modifier) {
                        held_modifiers.push(modifier);
                    }
                    held_modifiers
                        .sort_by_key(|modifier| ORDER.iter().position(|known| known == modifier));
                    clear_box(&keys_box);
                    fill_keys(&keys_box, &format!("{} + …", held_modifiers.join(" + ")));
                    return glib::Propagation::Stop;
                }
                let Some(name) = key_name(controller, keyval, keycode) else {
                    return glib::Propagation::Stop;
                };
                let combination = held_modifiers
                    .iter()
                    .copied()
                    .chain(std::iter::once(name.as_str()))
                    .collect::<Vec<_>>()
                    .join(" + ");
                clear_box(&keys_box);
                fill_keys(&keys_box, &combination);
                let owner_label = editor
                    .shortcuts
                    .borrow()
                    .iter()
                    .find(|other| {
                        other.description != description
                            && other
                                .keys
                                .iter()
                                .flatten()
                                .any(|keys| keybinds::are_same_keys(keys, &combination))
                    })
                    .map(|other| other.label().to_owned());
                match owner_label {
                    Some(owner_label) => {
                        conflict_label
                            .set_text(&trf("Used by “%1”, which loses it", &[&owner_label]));
                        conflict_label.set_visible(true);
                    }
                    None => conflict_label.set_visible(false),
                }
                pending_keys.replace(Some(combination));
                set_button.set_sensitive(true);
                glib::Propagation::Stop
            }
        });
        dialog.root.add_controller(key_controller);

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

        cancel_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        clear_button.connect_clicked({
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
        set_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let editor = Rc::downgrade(self);
            let description = description.to_owned();
            move |_| {
                let keys = pending_keys.borrow().clone();
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
            let other_slot = 1 - slot;
            if slots[other_slot]
                .as_deref()
                .is_some_and(|bound_keys| keybinds::are_same_keys(bound_keys, keys))
            {
                slots[other_slot] = None;
            }
            for owner in shortcuts
                .iter()
                .filter(|owner| owner.description != description)
            {
                if !owner
                    .keys
                    .iter()
                    .flatten()
                    .any(|bound_keys| keybinds::are_same_keys(bound_keys, keys))
                {
                    continue;
                }
                let mut remaining_keys = owner.keys.clone();
                for slot_keys in remaining_keys.iter_mut() {
                    if slot_keys
                        .as_deref()
                        .is_some_and(|bound_keys| keybinds::are_same_keys(bound_keys, keys))
                    {
                        *slot_keys = None;
                    }
                }
                changes.push((owner.description.clone(), Some(remaining_keys)));
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
    let unshifted_key = controller
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
    let name = unshifted_key.name()?.to_string();
    let mut chars = name.chars();
    Some(match (chars.next(), chars.next()) {
        (Some(letter), None) => letter.to_uppercase().to_string(),
        _ => name,
    })
}

fn natural_sort_key(label: &str) -> Vec<(String, u64)> {
    let mut parts = Vec::new();
    let mut letters = String::new();
    let mut digits = String::new();
    for character in label.to_lowercase().chars() {
        if character.is_ascii_digit() {
            digits.push(character);
            continue;
        }
        if !digits.is_empty() {
            parts.push((std::mem::take(&mut letters), digits.parse().unwrap_or(0)));
            digits.clear();
        }
        letters.push(character);
    }
    parts.push((letters, digits.parse().unwrap_or(0)));
    parts
}

fn display_name(part: &str) -> String {
    KEYPAD
        .iter()
        .chain(MODIFIER_NAMES.iter())
        .chain(MOUSE_NAMES.iter())
        .find(|(written, _)| *written == part)
        .map_or_else(|| part.to_owned(), |(_, name)| (*name).to_owned())
}

fn fill_keys(face: &gtk4::Box, keys: &str) {
    for part in keys.split(" + ") {
        face.append(&key(&display_name(part)));
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
            editor.clear_tips();
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
