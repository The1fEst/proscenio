use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config;
use crate::core::listeners::Subscription;
use crate::platform::hypr;
use crate::services::Services;

pub const FLASH: &str = "/accessibility/flashOnBell";
const NAMESPACE: &str = "proscenio:bell";
const STRENGTH: f64 = 0.35;
const LENGTH: f64 = 300_000.0;

pub fn watch(app: &gtk4::Application, services: &Rc<Services>) -> Subscription {
    let app = app.clone();
    let flashing = Rc::new(Cell::new(false));
    services.events.subscribe(move |event, _| {
        if event == "bell" && config::value_bool(FLASH, false) && !flashing.replace(true) {
            flash(&app, flashing.clone());
        }
    })
}

fn flash(app: &gtk4::Application, flashing: Rc<Cell<bool>>) {
    let area = gtk4::DrawingArea::new();
    area.set_draw_func(|_, cr, width, height| {
        cr.set_source_rgb(1.0, 1.0, 1.0);
        cr.rectangle(0.0, 0.0, width as f64, height as f64);
        let _ = cr.fill();
    });
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&area)
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
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_opacity(STRENGTH);
    window.connect_realize(|window| {
        if let Some(surface) = window.surface() {
            surface.set_input_region(Some(&gtk4::cairo::Region::create()));
        }
    });
    let start = Cell::new(0);
    window.add_tick_callback(move |window, clock| {
        let now = clock.frame_time();
        if start.get() == 0 {
            start.set(now);
        }
        let part = ((now - start.get()) as f64 / LENGTH).min(1.0);
        window.set_opacity(STRENGTH * (1.0 - part));
        if part < 1.0 {
            return glib::ControlFlow::Continue;
        }
        window.destroy();
        flashing.set(false);
        glib::ControlFlow::Break
    });
    window.present();
}
