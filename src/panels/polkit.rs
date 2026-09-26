use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

use crate::platform::hypr;
use crate::services::polkit::Polkit;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::{Style, TextField};
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:polkit";
const WIDTH: f64 = 450.0;
const ICON: f64 = 26.0;
const BUTTON_ROW_BOTTOM: f64 = 10.0;

struct Surface {
    window: gtk4::ApplicationWindow,
    field: Rc<TextField>,
    ok: gtk4::Widget,
    _dialog: Rc<WindowDialog>,
}

pub struct PolkitWindows {
    app: gtk4::Application,
    theme: SharedTheme,
    polkit: Rc<Polkit>,
    surfaces: RefCell<Vec<Surface>>,
    response: gtk4::EntryBuffer,
    interaction: std::cell::Cell<bool>,
}

impl PolkitWindows {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, polkit: &Rc<Polkit>) -> Rc<Self> {
        let windows = Rc::new(PolkitWindows {
            app: app.clone(),
            theme: theme.clone(),
            polkit: polkit.clone(),
            surfaces: RefCell::new(Vec::new()),
            response: gtk4::PasswordEntryBuffer::new().upcast(),
            interaction: std::cell::Cell::new(false),
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
            .forever();
        windows
    }

    fn sync(self: &Rc<Self>) {
        let Some(flow) = self.polkit.flow() else {
            for surface in self.surfaces.take() {
                surface.window.destroy();
            }
            self.response.set_text("");
            self.interaction.set(false);
            return;
        };
        if self.surfaces.borrow().is_empty() {
            self.open(&flow.message);
        }
        let available = flow.interaction_available();
        let prompt = clean_prompt(&flow.prompt(), flow.response_visible());
        let was_available = self.interaction.replace(available);
        let became_available = available && !was_available;
        if became_available {
            self.response.set_text("");
        }
        for surface in self.surfaces.borrow().iter() {
            surface.field.set_enabled(available);
            surface.ok.set_sensitive(available);
            surface.field.set_placeholder(&prompt);
            surface.field.set_text_visible(flow.response_visible());
        }
        if became_available && let Some(surface) = self.surfaces.borrow().first() {
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
            .map(|(index, monitor)| self.surface(monitor, &message, index == 0))
            .collect();
        self.surfaces.replace(surfaces);
    }

    fn surface(self: &Rc<Self>, monitor: &gdk::Monitor, message: &str, typing: bool) -> Surface {
        let dialog = WindowDialog::new(&self.theme, None);
        dialog.set_background_width(WIDTH);
        dialog.set_scrim_radius(0.0);

        let icon = text::symbol("security", ICON);
        text::set_color(&icon, "colSecondary");
        dialog.column.add(&Centred::integral(&icon), Place::wide());

        let title = windowdialog::title("Authentication");
        title.set_xalign(0.5);
        title.set_justify(gtk4::Justification::Center);
        dialog.column.add(&title, Place::wide());

        let paragraph = text::styled(message);
        text::set_color(&paragraph, "colOnSurfaceVariant");
        paragraph.set_wrap(true);
        paragraph.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
        paragraph.set_xalign(0.0);
        dialog.column.add(&paragraph, Place::wide());

        let field = TextField::secret_with_buffer(&self.theme, Style::Outlined, "", &self.response);
        field.root.set_hexpand(true);
        field.set_enabled(false);
        dialog.column.add(&field.root, Place::wide());

        let (row, mut place) = windowdialog::button_row();
        place.bottom = BUTTON_ROW_BOTTOM;
        let cancel = windowdialog::button(&self.theme, "Cancel");
        let ok = windowdialog::button(&self.theme, "OK");
        ok.set_sensitive(false);
        row.append(&windowdialog::spacer());
        row.append(&cancel);
        row.append(&ok);
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
            let polkit = self.polkit.clone();
            move |_| polkit.cancel()
        });
        ok.connect_clicked({
            let polkit = self.polkit.clone();
            let response = self.response.clone();
            move |_| polkit.submit(&response.text())
        });
        field.connect_accepted({
            let polkit = self.polkit.clone();
            let windows = Rc::downgrade(self);
            move || {
                if let Some(windows) = windows.upgrade()
                    && windows.interaction.get()
                {
                    polkit.submit(&windows.response.text());
                }
            }
        });
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let polkit = self.polkit.clone();
            move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                polkit.cancel();
                glib::Propagation::Stop
            }
        });
        window.add_controller(keys);

        window.present();
        dialog.show(true, || {});
        Surface {
            window,
            field,
            ok: ok.upcast(),
            _dialog: dialog,
        }
    }
}

fn clean_prompt(prompt: &str, visible: bool) -> String {
    let trimmed = prompt.trim();
    let cleaned = trimmed.strip_suffix(':').unwrap_or(trimmed);
    if !cleaned.is_empty() {
        return cleaned.to_owned();
    }
    if visible { "Input" } else { "Password" }.to_owned()
}
