use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::Cell;
use std::rc::Rc;

use super::{BUTTON_SPACING, Editor, InvalidText, NOTE_START};
use crate::core::i18n::tr;
use crate::core::{process, tools};
use crate::panels::settings::content::{Choice, Page, Style};
use crate::platform::network::nmprofile::Profile;
use crate::platform::security::pam;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::{ConfigSwitch, icon_button};
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::spinbox::SpinBox;
use crate::ui::widgets::input::textfield::TextField;

pub(super) fn note() -> gtk4::Label {
    let label = text::styled_sized("", pixel_size::SMALLER);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_margin_start(NOTE_START);
    label.set_visible(false);
    label
}

pub(super) fn show_note(label: &gtk4::Label, message: Option<&str>, color: &str) {
    label.set_visible(message.is_some());
    label.set_text(message.unwrap_or_default());
    text::set_color(label, color);
}

fn pick_secret_hint(is_agent_owned: bool, are_secrets_read: bool, value: &str) -> Option<String> {
    if is_agent_owned {
        return Some(tr(
            "Kept by a secret agent, such as the keyring, not by NetworkManager",
        ));
    }
    (!are_secrets_read && value.is_empty())
        .then(|| tr("Stored and hidden. Type to replace it, or press the eye to show it"))
}

fn make_field_key(field_keys: &[String], label: &str) -> String {
    let prefix = format!("{label}\0");
    let occurrence = field_keys
        .iter()
        .filter(|key| key.starts_with(&prefix))
        .count();
    format!("{prefix}{occurrence}")
}

impl Editor {
    pub(in super::super) fn field(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        label: &str,
        value: &str,
        apply: impl Fn(&str, &mut Profile) -> Result<(), String> + 'static,
    ) -> Rc<TextField> {
        let field = TextField::new(&self.theme, Style::Outlined, label);
        self.watch(parent, field.clone(), label, value, false, apply);
        field
    }

    pub(in super::super) fn secret_field(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        label: &str,
        value: &str,
        is_agent_owned: bool,
        apply: impl Fn(&str, &mut Profile) -> Result<(), String> + 'static,
    ) -> Rc<TextField> {
        let field = TextField::secret(&self.theme, Style::Outlined, label);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
        let is_shown = Cell::new(false);
        let (reveal_button, _) = icon_button(&self.theme, "visibility", false, "");
        reveal_button.connect_clicked({
            let editor = Rc::downgrade(self);
            let field = Rc::downgrade(&field);
            move |_| {
                let (Some(editor), Some(field)) = (editor.upgrade(), field.upgrade()) else {
                    return;
                };
                if editor.are_secrets_read.get() {
                    is_shown.set(!is_shown.get());
                    field.set_text_visible(is_shown.get());
                } else {
                    editor.reveal_secrets();
                }
            }
        });
        let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        holder.set_hexpand(true);
        row.append(&holder);
        row.append(&reveal_button);
        parent.append(&row);
        self.watch(&holder, field.clone(), label, value, true, apply);
        if let Some(text) = pick_secret_hint(is_agent_owned, self.are_secrets_read.get(), value) {
            let hint = note();
            show_note(&hint, Some(&text), "colSubtext");
            parent.append(&hint);
        }
        field
    }

    fn watch(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        field: Rc<TextField>,
        label: &str,
        value: &str,
        is_secret: bool,
        apply: impl Fn(&str, &mut Profile) -> Result<(), String> + 'static,
    ) {
        field.root.set_hexpand(true);
        field.set_text(value);
        parent.append(&field.root);
        let problem = note();
        parent.append(&problem);
        let key = make_field_key(&self.field_keys.borrow(), label);
        self.field_keys.borrow_mut().push(key.clone());
        if let Some(invalid_text) = self.errors.borrow().get(&key) {
            field.set_text(&invalid_text.text);
            show_note(&problem, Some(&invalid_text.message), "colError");
        }
        field.connect_changed({
            let editor = Rc::downgrade(self);
            let field = Rc::downgrade(&field);
            move || {
                let (Some(editor), Some(field)) = (editor.upgrade(), field.upgrade()) else {
                    return;
                };
                let text = field.text();
                match editor.edit(|profile| apply(&text, profile)) {
                    Ok(()) => {
                        editor.errors.borrow_mut().remove(&key);
                        show_note(&problem, None, "colError");
                    }
                    Err(message) => {
                        show_note(&problem, Some(&message), "colError");
                        let invalid_text = InvalidText {
                            message,
                            text: text.clone(),
                        };
                        editor.errors.borrow_mut().insert(key.clone(), invalid_text);
                    }
                }
                if is_secret {
                    editor.are_secrets_edited.set(true);
                    pam::wipe(&mut text.into_bytes());
                }
                editor.refresh();
            }
        });
        self.hold(field);
    }

    pub(in super::super) fn switch(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        icon: &str,
        label: &str,
        read: impl Fn(&Profile) -> bool + 'static,
        write: impl Fn(&mut Profile, bool) + 'static,
    ) -> Rc<ConfigSwitch> {
        let switch = ConfigSwitch::new(&self.theme, icon, label, {
            let editor = Rc::downgrade(self);
            move |is_on| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        write(profile, is_on);
                        Ok(())
                    });
                    editor.queue_rebuild();
                }
            }
        });
        switch.button.set_hexpand(true);
        switch.bind({
            let editor = Rc::downgrade(self);
            move || {
                editor
                    .upgrade()
                    .is_some_and(|editor| read(&editor.draft.borrow()))
            }
        });
        parent.append(&switch.button);
        self.hold(switch.clone());
        switch
    }

    pub(in super::super) fn choose(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        options: Vec<(String, Value)>,
        current: Value,
        write: impl Fn(&mut Profile, &Value) + 'static,
    ) {
        let choices = options
            .into_iter()
            .map(|(label, value)| Choice {
                label,
                icon: "",
                value,
            })
            .collect();
        let selection = Selection::new(&self.theme, choices, {
            let editor = Rc::downgrade(self);
            move |value| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        write(profile, &value);
                        Ok(())
                    });
                    editor.queue_rebuild();
                }
            }
        });
        selection.set_current(&current);
        parent.append(&selection.root);
        self.hold(selection);
    }

    pub(super) fn spin(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        icon: &str,
        label: &str,
        range: (i64, i64),
        value: i64,
        write: impl Fn(&mut Profile, i64) + 'static,
    ) {
        let spin = SpinBox::new(&self.theme, range.0, range.1, 1, 0);
        spin.set_value(value);
        spin.connect_changed({
            let editor = Rc::downgrade(self);
            move |value| {
                if let Some(editor) = editor.upgrade() {
                    let _ = editor.edit(|profile| {
                        write(profile, value);
                        Ok(())
                    });
                    editor.refresh();
                }
            }
        });
        Page::unkept_spin_row(parent, icon, label, &spin);
        self.hold(spin);
    }

    pub(in super::super) fn certificate_field(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        label: &str,
        value: &str,
        apply: impl Fn(&mut Profile, String) + 'static,
    ) {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
        let holder = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        holder.set_hexpand(true);
        row.append(&holder);
        let (pick_button, _) = icon_button(&self.theme, "folder_open", false, "");
        row.append(&pick_button);
        parent.append(&row);
        let field = TextField::new(&self.theme, Style::Outlined, label);
        self.watch(
            &holder,
            field.clone(),
            label,
            value,
            false,
            move |text, profile| {
                apply(profile, text.trim().to_owned());
                Ok(())
            },
        );
        if !tools::find_missing(&[&tools::KDIALOG]).is_empty() {
            pick_button.set_sensitive(false);
            return;
        }
        let title = label.to_owned();
        let field = Rc::downgrade(&field);
        pick_button.connect_clicked(move |_| {
            let field = field.clone();
            let title = title.clone();
            glib::spawn_future_local(async move {
                let home = glib::home_dir().to_string_lossy().into_owned();
                let picker = process::command(&[
                    "kdialog",
                    "--getopenfilename",
                    &home,
                    &format!(
                        "*.pem *.crt *.cer *.der *.key *.p12 *.pfx|{}",
                        tr("Certificates and keys")
                    ),
                    "--title",
                    &title,
                ]);
                let selected_path = process::capture_text(picker)
                    .await
                    .map(|path| path.trim().to_owned())
                    .filter(|path| !path.is_empty());
                if let (Some(path), Some(field)) = (selected_path, field.upgrade()) {
                    field.set_text(&path);
                }
            });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_secret_held_by_an_agent_says_so_and_an_unread_stored_one_says_it_is_hidden() {
        let agent_hint = Some(tr(
            "Kept by a secret agent, such as the keyring, not by NetworkManager",
        ));
        let hidden_hint = Some(tr(
            "Stored and hidden. Type to replace it, or press the eye to show it",
        ));
        for (are_secrets_read, value) in [(false, ""), (true, ""), (true, "secret")] {
            assert_eq!(pick_secret_hint(true, are_secrets_read, value), agent_hint);
        }
        assert_eq!(pick_secret_hint(false, false, ""), hidden_hint);
        assert_eq!(pick_secret_hint(false, false, "secret"), None);
        assert_eq!(pick_secret_hint(false, true, ""), None);
    }

    #[test]
    fn fields_with_the_same_label_are_numbered_in_order() {
        let mut field_keys = Vec::new();
        for label in ["Public key", "Endpoint", "Public key", "Endpoint", "Name"] {
            let key = make_field_key(&field_keys, label);
            field_keys.push(key);
        }
        assert_eq!(
            field_keys,
            [
                "Public key\u{0}0",
                "Endpoint\u{0}0",
                "Public key\u{0}1",
                "Endpoint\u{0}1",
                "Name\u{0}0"
            ]
        );
    }
}
