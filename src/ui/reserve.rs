use gtk4::gdk;
use gtk4::graphene;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use crate::ui::widgets::paint::Paint;

pub struct Reserve {
    window: gtk4::ApplicationWindow,
}

impl Reserve {
    pub fn new(
        app: &gtk4::Application,
        monitor: &gdk::Monitor,
        edge: Edge,
        namespace: &str,
    ) -> Self {
        let dot = Paint::new(|snapshot, width, height| {
            snapshot.append_color(
                &gdk::RGBA::new(0.0, 0.0, 0.0, 1.0 / 255.0),
                &graphene::Rect::new(0.0, 0.0, width, height),
            );
        });
        dot.set_size_request(1, 1);
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .default_width(1)
            .default_height(1)
            .child(&dot)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(namespace));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Background);
        window.set_anchor(edge, true);
        window.connect_realize(|window| {
            if let Some(surface) = window.surface() {
                surface.set_input_region(Some(&gtk4::cairo::Region::create()));
            }
        });
        Reserve { window }
    }

    pub fn hold(&self, zone: i32) {
        if zone <= 0 {
            self.release();
            return;
        }
        self.window.set_exclusive_zone(zone);
        if !self.window.is_visible() {
            self.window.present();
        }
    }

    pub fn release(&self) {
        self.window.set_visible(false);
    }
}

impl Drop for Reserve {
    fn drop(&mut self) {
        self.window.destroy();
    }
}
