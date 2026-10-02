use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::platform::hypr;
use crate::services::prompter::{Answer, Kind, Prompt, Prompter};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::Switch;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::{Style, TextField};
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:keyring";
const WIDTH: f64 = 450.0;
const ICON: f64 = 26.0;
const CHOICE_SPACING: i32 = 12;

struct Surface {
    window: gtk4::ApplicationWindow,
    _dialog: Rc<WindowDialog>,
    _fields: Vec<Rc<TextField>>,
    choice: Option<Rc<Switch>>,
}

pub struct KeyringPromptWindows {
    app: gtk4::Application,
    theme: SharedTheme,
    prompter: Rc<Prompter>,
    shown: RefCell<Option<Rc<Prompt>>>,
    surfaces: RefCell<Vec<Surface>>,
    password: gtk4::EntryBuffer,
    repeated: gtk4::EntryBuffer,
    choice: std::cell::Cell<bool>,
}

impl KeyringPromptWindows {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, prompter: &Rc<Prompter>) -> Rc<Self> {
        let windows = Rc::new(KeyringPromptWindows {
            app: app.clone(),
            theme: theme.clone(),
            prompter: prompter.clone(),
            shown: RefCell::new(None),
            surfaces: RefCell::new(Vec::new()),
            password: gtk4::PasswordEntryBuffer::new().upcast(),
            repeated: gtk4::PasswordEntryBuffer::new().upcast(),
            choice: std::cell::Cell::new(false),
        });
        prompter
            .subscribe({
                let windows = Rc::downgrade(&windows);
                move || {
                    if let Some(windows) = windows.upgrade() {
                        windows.sync();
                    }
                }
            })
            .forever();
        windows
    }

    fn sync(self: &Rc<Self>) {
        let current = self.prompter.current();
        let same = match (&current, &*self.shown.borrow()) {
            (Some(current), Some(shown)) => Rc::ptr_eq(current, shown),
            (None, None) => true,
            _ => false,
        };
        if same {
            return;
        }
        for surface in self.surfaces.take() {
            surface.window.destroy();
        }
        self.password.set_text("");
        self.repeated.set_text("");
        self.shown.replace(current.clone());
        if let Some(prompt) = current {
            self.open(&prompt);
        }
    }

    fn open(self: &Rc<Self>, prompt: &Rc<Prompt>) {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        self.choice.set(prompt.flag("choice-chosen"));
        let mut monitors: Vec<gdk::Monitor> = display
            .monitors()
            .iter::<gdk::Monitor>()
            .flatten()
            .collect();
        let focused = hypr::focused_monitor();
        if let Some(index) = monitors
            .iter()
            .position(|monitor| monitor.connector().map(String::from) == focused)
        {
            monitors[..=index].rotate_right(1);
        }
        let surfaces = monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| self.surface(prompt, monitor, index == 0))
            .collect();
        self.surfaces.replace(surfaces);
    }

    fn surface(
        self: &Rc<Self>,
        prompt: &Rc<Prompt>,
        monitor: &gdk::Monitor,
        typing: bool,
    ) -> Surface {
        let dialog = WindowDialog::new(&self.theme, None);
        dialog.set_background_width(WIDTH);
        dialog.set_scrim_radius(0.0);

        let icon = text::symbol("key", ICON);
        text::set_color(&icon, "colSecondary");
        dialog.column.add(&Centred::integral(&icon), Place::wide());

        let heading = [prompt.text("message"), prompt.text("title")]
            .into_iter()
            .find(|text| !text.is_empty())
            .unwrap_or_else(|| tr("Authentication"));
        let title = windowdialog::title(&heading);
        title.set_xalign(0.5);
        title.set_justify(gtk4::Justification::Center);
        dialog.column.add(&title, Place::wide());

        for (name, color) in [
            ("description", "colOnSurfaceVariant"),
            ("warning", "colError"),
        ] {
            let content = prompt.text(name);
            if content.is_empty() {
                continue;
            }
            let paragraph = text::styled(&content);
            text::set_color(&paragraph, color);
            paragraph.set_wrap(true);
            paragraph.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
            paragraph.set_xalign(0.0);
            dialog.column.add(&paragraph, Place::wide());
        }

        let mut fields = Vec::new();
        if prompt.kind() == Kind::Password {
            fields.push(TextField::secret_with_buffer(
                &self.theme,
                Style::Outlined,
                &tr("Password"),
                &self.password,
            ));
            if prompt.flag("password-new") {
                fields.push(TextField::secret_with_buffer(
                    &self.theme,
                    Style::Outlined,
                    &tr("Repeat password"),
                    &self.repeated,
                ));
            }
        }
        for field in &fields {
            field.root.set_hexpand(true);
            dialog.column.add(&field.root, Place::wide());
        }

        let choice_label = prompt.text("choice-label");
        let choice = (!choice_label.is_empty()).then(|| {
            let row = gtk4::Box::new(gtk4::Orientation::Horizontal, CHOICE_SPACING);
            let label = text::styled(&without_mnemonic(&choice_label));
            text::set_color(&label, "colOnSurfaceVariant");
            label.set_wrap(true);
            label.set_xalign(0.0);
            label.set_hexpand(true);
            let switch = Switch::new(&self.theme);
            switch.set(self.choice.get());
            switch.connect_clicked({
                let windows = Rc::downgrade(self);
                move || {
                    if let Some(windows) = windows.upgrade() {
                        windows.choice.set(!windows.choice.get());
                        for surface in windows.surfaces.borrow().iter() {
                            if let Some(choice) = &surface.choice {
                                choice.set(windows.choice.get());
                            }
                        }
                    }
                }
            });
            row.append(&label);
            row.append(&switch.area);
            dialog.column.add(&row, Place::wide());
            switch
        });

        let (row, place) = windowdialog::button_row();
        let cancel_label = without_mnemonic(&prompt.text("cancel-label"));
        let continue_label = without_mnemonic(&prompt.text("continue-label"));
        let cancel = windowdialog::button(
            &self.theme,
            &if cancel_label.is_empty() {
                tr("Cancel")
            } else {
                cancel_label
            },
        );
        let proceed = windowdialog::button(
            &self.theme,
            &if continue_label.is_empty() {
                tr("OK")
            } else {
                continue_label
            },
        );
        row.append(&windowdialog::spacer());
        row.append(&cancel);
        row.append(&proceed);
        dialog.column.add(&row, place);

        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .child(&dialog.root)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Overlay);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(if typing {
            KeyboardMode::Exclusive
        } else {
            KeyboardMode::None
        });

        cancel.connect_clicked({
            let windows = Rc::downgrade(self);
            move |_| {
                if let Some(windows) = windows.upgrade() {
                    windows.finish(false);
                }
            }
        });
        proceed.connect_clicked({
            let windows = Rc::downgrade(self);
            move |_| {
                if let Some(windows) = windows.upgrade() {
                    windows.finish(true);
                }
            }
        });
        for field in &fields {
            field.connect_accepted({
                let windows = Rc::downgrade(self);
                move || {
                    if let Some(windows) = windows.upgrade() {
                        windows.finish(true);
                    }
                }
            });
        }
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let windows = Rc::downgrade(self);
            move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                if let Some(windows) = windows.upgrade() {
                    windows.finish(false);
                }
                glib::Propagation::Stop
            }
        });
        window.add_controller(keys);

        window.present();
        dialog.show(true, || {});
        if typing && let Some(field) = fields.first() {
            field.grab_focus();
        }
        Surface {
            window,
            _dialog: dialog,
            _fields: fields,
            choice,
        }
    }

    fn finish(&self, proceed: bool) {
        let Some(prompt) = self.shown.borrow().clone() else {
            return;
        };
        let password = self.password.text().to_string();
        if proceed && prompt.flag("password-new") && password != self.repeated.text().as_str() {
            return;
        }
        let answer = if proceed {
            Answer::Continue {
                password,
                choice: self.choice.get(),
            }
        } else {
            Answer::Cancel
        };
        self.prompter.answer(&prompt, answer);
    }
}

fn without_mnemonic(label: &str) -> String {
    let mut plain = String::with_capacity(label.len());
    let mut chars = label.chars().peekable();
    while let Some(char) = chars.next() {
        if char != '_' {
            plain.push(char);
            continue;
        }
        if chars.peek() == Some(&'_') {
            plain.push('_');
            chars.next();
        }
    }
    plain
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mnemonic_underscore_goes_and_a_doubled_one_stays() {
        assert_eq!(without_mnemonic("_Unlock"), "Unlock");
        assert_eq!(without_mnemonic("snake__case"), "snake_case");
        assert_eq!(without_mnemonic("Continue"), "Continue");
    }
}
