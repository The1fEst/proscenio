use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::platform::hyprland::hypr;
use crate::platform::security::pam;
use crate::services::overlay::polkit::Polkit;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::textfield::{Style, TextField};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:polkit";
const WIDTH: f64 = 450.0;
const ICON: f64 = 26.0;

struct Surface {
    window: gtk4::ApplicationWindow,
    field: Rc<TextField>,
    ok_button: gtk4::Widget,
    _dialog: Rc<WindowDialog>,
}

pub struct PolkitWindows {
    app: gtk4::Application,
    theme: SharedTheme,
    polkit: Rc<Polkit>,
    surfaces: RefCell<Vec<Surface>>,
    response_buffer: gtk4::EntryBuffer,
    is_interaction_available: std::cell::Cell<bool>,
}

impl PolkitWindows {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, polkit: &Rc<Polkit>) -> Rc<Self> {
        let windows = Rc::new(PolkitWindows {
            app: app.clone(),
            theme: theme.clone(),
            polkit: polkit.clone(),
            surfaces: RefCell::new(Vec::new()),
            response_buffer: gtk4::PasswordEntryBuffer::new().upcast(),
            is_interaction_available: std::cell::Cell::new(false),
        });
        polkit
            .subscribe({
                let windows = Rc::downgrade(&windows);
                move || {
                    if let Some(windows) = windows.upgrade() {
                        windows.sync();
                    }
                }
            })
            .detach();
        windows
    }

    fn sync(self: &Rc<Self>) {
        let Some(flow) = self.polkit.flow() else {
            for surface in self.surfaces.take() {
                surface.window.destroy();
            }
            self.response_buffer.set_text("");
            self.is_interaction_available.set(false);
            return;
        };
        if self.surfaces.borrow().is_empty() {
            self.open(&flow.message);
        }
        let is_available = flow.is_interaction_available();
        let placeholder = clean_prompt(&flow.prompt(), flow.is_response_visible());
        let was_available = self.is_interaction_available.replace(is_available);
        let has_become_available = is_available && !was_available;
        if has_become_available {
            self.response_buffer.set_text("");
        }
        for surface in self.surfaces.borrow().iter() {
            surface.field.set_enabled(is_available);
            surface.ok_button.set_sensitive(is_available);
            surface.field.set_placeholder(&placeholder);
            surface.field.set_text_visible(flow.is_response_visible());
        }
        if has_become_available && let Some(surface) = self.surfaces.borrow().first() {
            surface.field.grab_focus();
        }
    }

    fn open(self: &Rc<Self>, message: &str) {
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let monitors = display.monitors();
        let message = message.strip_suffix('.').unwrap_or(message).to_owned();
        let mut monitors: Vec<gdk::Monitor> = monitors.iter::<gdk::Monitor>().flatten().collect();
        let focused_connector = hypr::focused_monitor();
        if let Some(index) = monitors
            .iter()
            .position(|monitor| monitor.connector().map(String::from) == focused_connector)
        {
            monitors[..=index].rotate_right(1);
        }
        let surfaces = monitors
            .iter()
            .enumerate()
            .map(|(index, monitor)| self.surface(monitor, &message, index == 0))
            .collect();
        self.surfaces.replace(surfaces);
    }

    fn surface(
        self: &Rc<Self>,
        monitor: &gdk::Monitor,
        message: &str,
        takes_keyboard: bool,
    ) -> Surface {
        let dialog = WindowDialog::new(&self.theme, None);
        dialog.set_background_width(WIDTH);
        dialog.set_scrim_radius(0.0);

        let icon = text::symbol("security", ICON);
        text::set_color(&icon, "colSecondary");
        dialog.column.add(&Centered::integral(&icon), Place::wide());

        let title = windowdialog::title(&tr("Authentication"));
        title.set_xalign(0.5);
        title.set_justify(gtk4::Justification::Center);
        dialog.column.add(&title, Place::wide());

        let paragraph = text::styled(message);
        text::set_color(&paragraph, "colOnSurfaceVariant");
        paragraph.set_wrap(true);
        paragraph.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        paragraph.set_xalign(0.0);
        dialog.column.add(&paragraph, Place::wide());

        let field =
            TextField::secret_with_buffer(&self.theme, Style::Outlined, "", &self.response_buffer);
        field.root.set_hexpand(true);
        field.set_enabled(false);
        dialog.column.add(&field.root, Place::wide());

        let (row, place) = windowdialog::button_row();
        let cancel_button = windowdialog::button(&self.theme, &tr("Cancel"));
        let ok_button = windowdialog::button(&self.theme, &tr("OK"));
        ok_button.set_sensitive(false);
        row.append(&windowdialog::spacer());
        row.append(&cancel_button);
        row.append(&ok_button);
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
        window.set_keyboard_mode(if takes_keyboard {
            KeyboardMode::Exclusive
        } else {
            KeyboardMode::None
        });

        cancel_button.connect_clicked({
            let polkit = self.polkit.clone();
            move |_| polkit.cancel()
        });
        ok_button.connect_clicked({
            let polkit = self.polkit.clone();
            let response_buffer = self.response_buffer.clone();
            move |_| polkit.submit(pam::copy_text(&response_buffer))
        });
        field.connect_accepted({
            let polkit = self.polkit.clone();
            let windows = Rc::downgrade(self);
            move || {
                if let Some(windows) = windows.upgrade()
                    && windows.is_interaction_available.get()
                {
                    polkit.submit(pam::copy_text(&windows.response_buffer));
                }
            }
        });
        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let polkit = self.polkit.clone();
            move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                polkit.cancel();
                glib::Propagation::Stop
            }
        });
        window.add_controller(key_controller);

        window.present();
        dialog.show(true, || {});
        Surface {
            window,
            field,
            ok_button: ok_button.upcast(),
            _dialog: dialog,
        }
    }
}

fn clean_prompt(prompt: &str, is_visible: bool) -> String {
    let prompt_text = prompt.trim();
    let placeholder = prompt_text.strip_suffix(':').unwrap_or(prompt_text);
    if !placeholder.is_empty() {
        return placeholder.to_owned();
    }
    tr(if is_visible { "Input" } else { "Password" })
}
