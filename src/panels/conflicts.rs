use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::{tr, trf};
use crate::core::process::{self, detach};
use crate::platform::hypr;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

const NAMESPACE: &str = "proscenio:conflicts";
const WIDTH: f64 = 400.0;
const ICON: f64 = 26.0;
const AUTO_KILL: &str = "/conflictKiller/autoKillNotificationDaemons";
const NOTIFICATION_DAEMONS: &str = "mako|dunst";

pub fn check(app: &gtk4::Application, theme: &SharedTheme) {
    let app = app.clone();
    let theme = theme.clone();
    process::read(&["pgrep", "-l", "-x", NOTIFICATION_DAEMONS], move |found| {
        let mut running: Vec<String> = found
            .lines()
            .filter_map(|line| line.split_whitespace().nth(1))
            .map(str::to_owned)
            .collect();
        running.sort();
        running.dedup();
        if running.is_empty() {
            return;
        }
        if config::value_bool(AUTO_KILL, false) {
            kill(&running);
            return;
        }
        ask(&app, &theme, running);
    });
}

fn kill(running: &[String]) {
    for name in running {
        detach(&["pkill", "-x", name]);
    }
}

fn ask(app: &gtk4::Application, theme: &SharedTheme, running: Vec<String>) {
    let dialog = WindowDialog::new(theme, None);
    dialog.set_background_width(WIDTH);

    let icon = text::symbol("notifications_off", ICON);
    text::set_color(&icon, "colSecondary");
    dialog.column.add(&Centred::integral(&icon), Place::wide());

    let title = windowdialog::title(&trf("Stop %1?", &[&running.join(" and ")]));
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
    let always = windowdialog::button(theme, &tr("Always"));
    let no = windowdialog::button(theme, &tr("No"));
    let yes = windowdialog::button(theme, &tr("Yes"));
    row.append(&always);
    row.append(&windowdialog::spacer());
    row.append(&no);
    row.append(&yes);
    dialog.column.add(&row, place);
    let running = Rc::new(running);

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&dialog.root)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    let focused = hypr::focused_monitor();
    let monitor = gdk::Display::default().and_then(|display| {
        display
            .monitors()
            .iter::<gdk::Monitor>()
            .flatten()
            .find(|monitor| monitor.connector().map(String::from) == focused)
    });
    window.set_monitor(monitor.as_ref());
    window.set_layer(Layer::Overlay);
    for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(-1);
    window.set_keyboard_mode(KeyboardMode::Exclusive);

    let dialog = Rc::new(dialog);
    let held = Rc::new(RefCell::new(Some(dialog.clone())));
    let close: Rc<dyn Fn()> = Rc::new({
        let dialog = Rc::downgrade(&dialog);
        let window = window.downgrade();
        move || {
            let Some(dialog) = dialog.upgrade() else {
                return;
            };
            let window = window.clone();
            let held = held.clone();
            dialog.show(false, move || {
                if let Some(window) = window.upgrade() {
                    window.destroy();
                }
                held.take();
            });
        }
    });
    always.connect_clicked({
        let close = close.clone();
        let running = running.clone();
        move |_| {
            kill(&running);
            config::store_value(AUTO_KILL, Value::Bool(true));
            close();
        }
    });
    yes.connect_clicked({
        let close = close.clone();
        move |_| {
            kill(&running);
            close();
        }
    });
    no.connect_clicked({
        let close = close.clone();
        move |_| close()
    });
    dialog.connect_dismiss(move || close());

    window.present();
    dialog.show(true, || {});
}
