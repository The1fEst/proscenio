use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::privacy::{Indicator, build_disc, list_active_indicators};
use crate::core::config::Config;
use crate::core::listeners::Subscription;
use crate::services::media::deviceusage::DeviceUsage;
use crate::ui::theme::rounding;

const NAMESPACE: &str = "proscenio:privacyDots";
const DOT_SIZE: i32 = 10;
const DOT_SPACING: i32 = 6;

pub struct PrivacyDots {
    window: gtk4::ApplicationWindow,
    row: gtk4::Box,
    usage: DeviceUsage,
    is_covered: Cell<bool>,
    shown_indicators: RefCell<Vec<Indicator>>,
    subscription: Cell<Option<Subscription>>,
}

impl PrivacyDots {
    pub fn new(
        app: &gtk4::Application,
        monitor: &gdk::Monitor,
        config: &Config,
        usage: &DeviceUsage,
    ) -> Rc<Self> {
        let orientation = if config.vertical {
            gtk4::Orientation::Vertical
        } else {
            gtk4::Orientation::Horizontal
        };
        let row = gtk4::Box::new(orientation, DOT_SPACING);
        let window = gtk4::ApplicationWindow::builder()
            .application(app)
            .child(&row)
            .build();
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Overlay);
        window.set_exclusive_zone(-1);
        let inset = (config.bar_thickness() - DOT_SIZE) / 2;
        if config.vertical {
            let side = if config.bottom {
                Edge::Right
            } else {
                Edge::Left
            };
            window.set_anchor(Edge::Top, true);
            window.set_anchor(side, true);
            window.set_margin(Edge::Top, rounding::SCREEN_ROUNDING);
            window.set_margin(side, inset);
        } else {
            let side = if config.bottom {
                Edge::Bottom
            } else {
                Edge::Top
            };
            window.set_anchor(Edge::Left, true);
            window.set_anchor(side, true);
            window.set_margin(Edge::Left, rounding::SCREEN_ROUNDING);
            window.set_margin(side, inset);
        }
        window.connect_realize(|window| {
            if let Some(surface) = window.surface() {
                surface.set_input_region(Some(&gtk4::cairo::Region::create()));
            }
        });

        let dots = Rc::new(PrivacyDots {
            window,
            row,
            usage: usage.clone(),
            is_covered: Cell::new(false),
            shown_indicators: RefCell::default(),
            subscription: Cell::new(None),
        });
        let weak_dots = Rc::downgrade(&dots);
        dots.subscription.set(Some(usage.subscribe(move || {
            if let Some(dots) = weak_dots.upgrade() {
                dots.refresh();
            }
        })));
        dots
    }

    pub fn set_covered(&self, is_covered: bool) {
        if self.is_covered.replace(is_covered) != is_covered {
            self.refresh();
        }
    }

    fn refresh(&self) {
        let active = list_active_indicators(&self.usage.usage());
        if !self.is_covered.get() || active.is_empty() {
            self.window.set_visible(false);
            self.shown_indicators.take();
            return;
        }
        if *self.shown_indicators.borrow() == active && self.window.is_visible() {
            return;
        }
        while let Some(child) = self.row.first_child() {
            self.row.remove(&child);
        }
        for indicator in &active {
            self.row.append(&build_disc(*indicator, DOT_SIZE));
        }
        self.shown_indicators.replace(active);
        self.window.present();
    }
}

impl Drop for PrivacyDots {
    fn drop(&mut self) {
        self.window.destroy();
    }
}
