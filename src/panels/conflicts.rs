use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::{tr, trf};
use crate::core::process::{self, detach};
use crate::platform::hyprland::hypr;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:conflicts";
const WIDTH: f64 = 400.0;
const ICON: f64 = 26.0;
const AUTO_KILL: &str = "/conflictKiller/autoKillNotificationDaemons";
const NOTIFICATION_DAEMONS: &str = "mako|dunst";

pub fn check(app: &gtk4::Application, theme: &SharedTheme) {
    let app = app.clone();
    let theme = theme.clone();
    process::read(
        &["pgrep", "-l", "-x", NOTIFICATION_DAEMONS],
        move |output| {
            let mut daemon_names: Vec<String> = output
                .lines()
                .filter_map(|line| line.split_whitespace().nth(1))
                .map(str::to_owned)
                .collect();
            daemon_names.sort();
            daemon_names.dedup();
            if daemon_names.is_empty() {
                return;
            }
            if config::value_bool(AUTO_KILL, false) {
                kill(&daemon_names);
                return;
            }
            ask(&app, &theme, daemon_names);
        },
    );
}

fn kill(daemon_names: &[String]) {
    for name in daemon_names {
        detach(&["pkill", "-x", name]);
    }
}

fn ask(app: &gtk4::Application, theme: &SharedTheme, daemon_names: Vec<String>) {
    let dialog = WindowDialog::new(theme, None);
    dialog.set_background_width(WIDTH);

    let icon = text::symbol("notifications_off", ICON);
    text::set_color(&icon, "colSecondary");
    dialog.column.add(&Centered::integral(&icon), Place::wide());

    let title = windowdialog::title(&trf("Stop %1?", &[&daemon_names.join(" and ")]));
    title.set_xalign(0.5);
    title.set_justify(gtk4::Justification::Center);
    dialog.column.add(&title, Place::wide());

    let description = text::styled(&format!(
        "{}.",
        tr("Conflicts with the shell's notification implementation")
    ));
    text::set_color(&description, "colOnSurfaceVariant");
    description.set_wrap(true);
    description.set_xalign(0.5);
    description.set_justify(gtk4::Justification::Center);
    dialog.column.add(&description, Place::wide());

    let (row, place) = windowdialog::button_row();
    let always_button = windowdialog::button(theme, &tr("Always"));
    let no_button = windowdialog::button(theme, &tr("No"));
    let yes_button = windowdialog::button(theme, &tr("Yes"));
    row.append(&always_button);
    row.append(&windowdialog::spacer());
    row.append(&no_button);
    row.append(&yes_button);
    dialog.column.add(&row, place);
    let daemon_names = Rc::new(daemon_names);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
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
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let dialog = Rc::new(dialog);
    let owned_dialog = Rc::new(RefCell::new(Some(dialog.clone())));
    let close: Rc<dyn Fn()> = Rc::new({
        let dialog = Rc::downgrade(&dialog);
        let window = window.downgrade();
        move || {
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            let window = window.clone();
            let owned_dialog = owned_dialog.clone();
            dialog.show(false, move || {
                if let Some(window) = window.upgrade() {
                    window.destroy();
                }
                owned_dialog.take();
            });
        }
    });
    always_button.connect_clicked({
        let close = close.clone();
        let daemon_names = daemon_names.clone();
        move |_| {
            kill(&daemon_names);
            config::store_value(AUTO_KILL, Value::Bool(true));
            close();
        }
    });
    yes_button.connect_clicked({
        let close = close.clone();
        move |_| {
            kill(&daemon_names);
            close();
        }
    });
    no_button.connect_clicked({
        let close = close.clone();
        move |_| close()
    });
    dialog.connect_dismiss(move || close());

    window.present();
    dialog.show(true, || {});
}
