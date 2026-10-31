use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::platform::hyprland::hypr;
use crate::services::network::bluezagent::{Answer, BluezAgent, Prompt, Request, is_valid_answer};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text::{self, Family};
use crate::ui::widgets::input::textfield::{Style, TextField};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:bluetoothagent";
const WIDTH: f64 = 400.0;
const ICON: f64 = 26.0;
const CODE_SIZE: f64 = 32.0;

struct Surface {
    request: Rc<Request>,
    window: gtk4::ApplicationWindow,
    dialog: Rc<WindowDialog>,
}

pub struct BluetoothAgentWindow {
    app: gtk4::Application,
    theme: SharedTheme,
    agent: Rc<BluezAgent>,
    surface: RefCell<Option<Surface>>,
}

impl BluetoothAgentWindow {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, agent: &Rc<BluezAgent>) -> Rc<Self> {
        let window = Rc::new(BluetoothAgentWindow {
            app: app.clone(),
            theme: theme.clone(),
            agent: agent.clone(),
            surface: RefCell::new(None),
        });
        agent
            .subscribe({
                let window = Rc::downgrade(&window);
                move || {
                    if let Some(window) = window.upgrade() {
                        window.sync();
                    }
                }
            })
            .detach();
        window.sync();
        window
    }

    fn sync(&self) {
        let request = self.agent.request();
        let is_shown = match (&request, self.surface.borrow().as_ref()) {
            (Some(request), Some(surface)) => Rc::ptr_eq(request, &surface.request),
            _ => false,
        };
        if is_shown {
            return;
        }
        if let Some(surface) = self.surface.take() {
            let dialog = surface.dialog.clone();
            dialog.show(false, move || surface.window.destroy());
        }
        if let Some(request) = request {
            self.surface.replace(Some(self.open(request)));
        }
    }

    fn open(&self, request: Rc<Request>) -> Surface {
        let prompt = request.prompt.clone();
        let dialog = WindowDialog::new(&self.theme, None);
        dialog.set_background_width(WIDTH);
        dialog.set_scrim_radius(0.0);

        let icon = text::symbol("bluetooth", ICON);
        text::set_color(&icon, "colSecondary");
        dialog.column.add(&Centered::integral(&icon), Place::wide());

        let title = windowdialog::title(&compose_title(&request));
        title.set_xalign(0.5);
        title.set_justify(gtk4::Justification::Center);
        dialog.column.add(&title, Place::wide());

        let description = text::styled(&compose_description(&prompt));
        text::set_color(&description, "colOnSurfaceVariant");
        description.set_wrap(true);
        description.set_xalign(0.5);
        description.set_justify(gtk4::Justification::Center);
        dialog.column.add(&description, Place::wide());

        if let Some(code) = prompt.code() {
            let code_label = text::styled(&code);
            text::set_font(&code_label, Family::Monospace, CODE_SIZE, "wght=500");
            code_label.set_selectable(true);
            dialog
                .column
                .add(&Centered::integral(&code_label), Place::wide());
        }

        let field = prompt.has_input().then(|| {
            let placeholder = tr(if prompt == Prompt::PinCode {
                "PIN code"
            } else {
                "Passkey"
            });
            let field = TextField::new(&self.theme, Style::Outlined, &placeholder);
            field.root.set_hexpand(true);
            dialog.column.add(&field.root, Place::wide());
            dialog.keep(field.clone());
            field
        });

        let (row, place) = windowdialog::button_row();
        let (decline_label, accept_label) = match &prompt {
            Prompt::PinCode | Prompt::Passkey => (Some("Cancel"), "OK"),
            Prompt::PinCodeDisplay(_) | Prompt::PasskeyDisplay(_) => (None, "Close"),
            Prompt::Confirmation(_) => (Some("Cancel"), "Pair"),
            Prompt::Authorization | Prompt::Service(_) => (Some("Deny"), "Allow"),
        };
        let send: Rc<dyn Fn(Answer)> = Rc::new({
            let agent = self.agent.clone();
            let request = Rc::downgrade(&request);
            move |answer| {
                if let Some(request) = request.upgrade() {
                    agent.answer(&request, answer);
                }
            }
        });
        let decline: Rc<dyn Fn()> = Rc::new({
            let send = send.clone();
            let answer = prompt.refusal();
            move || send(answer.clone())
        });
        row.append(&windowdialog::spacer());
        if let Some(label) = decline_label {
            let decline_button = windowdialog::button(&self.theme, &tr(label));
            decline_button.connect_clicked({
                let decline = decline.clone();
                move |_| decline()
            });
            row.append(&decline_button);
        }
        let accept_button = windowdialog::button(&self.theme, &tr(accept_label));
        row.append(&accept_button);
        dialog.column.add(&row, place);

        let accept: Rc<dyn Fn()> = Rc::new({
            let field = field.as_ref().map(Rc::downgrade);
            let prompt = prompt.clone();
            move || {
                let text = field
                    .as_ref()
                    .and_then(|field| field.upgrade())
                    .map(|field| field.text())
                    .unwrap_or_default();
                let text = text.trim();
                if is_valid_answer(&prompt, text) {
                    send(Answer::Accept(text.to_owned()));
                }
            }
        });
        if let Some(field) = &field {
            accept_button.set_sensitive(false);
            field.connect_changed({
                let field = Rc::downgrade(field);
                let accept_button = accept_button.clone();
                let prompt = prompt.clone();
                move || {
                    if let Some(field) = field.upgrade() {
                        accept_button.set_sensitive(is_valid_answer(&prompt, field.text().trim()));
                    }
                }
            });
            field.connect_accepted({
                let accept = accept.clone();
                move || accept()
            });
        }
        accept_button.connect_clicked(move |_| accept());
        dialog.connect_dismiss({
            let decline = decline.clone();
            move || decline()
        });

        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .child(&dialog.root)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        let focused_connector = hypr::focused_monitor();
        let monitor = gdk::Display::default().and_then(|display| {
            display
                .monitors()
                .iter::<gdk::Monitor>()
                .flatten()
                .find(|monitor| monitor.connector().map(String::from) == focused_connector)
        });
        window.set_monitor(monitor.as_ref());
        window.set_layer(Layer::Overlay);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(if prompt.is_display() {
            KeyboardMode::OnDemand
        } else {
            KeyboardMode::Exclusive
        });

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed(move |_, key, _, _| {
            if key != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            decline();
            glib::Propagation::Stop
        });
        window.add_controller(key_controller);

        window.present();
        dialog.show(true, || {});
        if let Some(field) = &field {
            field.grab_focus();
        }
        Surface {
            request,
            window,
            dialog,
        }
    }
}

fn compose_title(request: &Request) -> String {
    match &request.prompt {
        Prompt::Service(_) => trf("Allow %1 to connect?", &[&request.device_name]),
        _ => trf("Pair with %1", &[&request.device_name]),
    }
}

fn compose_description(prompt: &Prompt) -> String {
    match prompt {
        Prompt::PinCode => tr("Enter the PIN code for this device"),
        Prompt::Passkey => tr("Enter the passkey shown on the device"),
        Prompt::PinCodeDisplay(_) | Prompt::PasskeyDisplay(_) => {
            tr("Type this code on the device, then press Enter")
        }
        Prompt::Confirmation(_) => tr("Make sure the device shows the same passkey"),
        Prompt::Authorization => tr("Allow this device to pair?"),
        Prompt::Service(uuid) => trf("The device asks to use the service %1", &[uuid]),
    }
}
