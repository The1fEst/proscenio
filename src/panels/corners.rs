use gtk4::cairo;
use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::panels::sidebar::Sidebar;
use crate::services::Services;
use crate::services::states::Osd;

const NAMESPACE: &str = "proscenio:screenCorners";
const EDGE_REACH: f64 = 2.0;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    fn top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }

    fn left(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::BottomLeft)
    }

    fn opens_sidebar(self) -> bool {
        matches!(self, Corner::TopRight | Corner::BottomRight)
    }
}

pub fn open(
    app: &gtk4::Application,
    config: &Rc<Config>,
    services: &Rc<Services>,
    sidebar: &Rc<Sidebar>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> Vec<gtk4::ApplicationWindow> {
    if config.fake_rounding == 0 {
        return Vec::new();
    }

    let corners = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];
    let windows: Vec<gtk4::ApplicationWindow> = corners
        .iter()
        .map(|corner| one(app, config, services, sidebar, monitor, *corner))
        .collect();

    let connector: String = monitor.connector().map(Into::into).unwrap_or_default();
    let over_fullscreen = config.fake_rounding == 1;
    let show = {
        let windows = windows.clone();
        let fullscreen = services.fullscreen.clone();
        move || {
            let covered = !over_fullscreen && fullscreen.covers(&connector);
            for window in &windows {
                window.set_visible(!covered);
            }
        }
    };
    show();
    scope.keep(services.fullscreen.subscribe(show));

    windows
}

fn one(
    app: &gtk4::Application,
    config: &Rc<Config>,
    services: &Rc<Services>,
    sidebar: &Rc<Sidebar>,
    monitor: &gdk::Monitor,
    corner: Corner,
) -> gtk4::ApplicationWindow {
    let size = config.hug_rounding().max(1);
    let hot = config.corner_open && config.corner_bottom != corner.top();
    let (width, height) = if hot {
        (
            size.max(config.corner_width),
            size.max(config.corner_height),
        )
    } else {
        (size, size)
    };

    let beyond_x = i32::from(config.dead_pixel && !corner.left());
    let beyond_y = i32::from(config.dead_pixel && !corner.top());
    let base = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    base.set_size_request(width + beyond_x, height + beyond_y);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&base));

    let shape = paint(corner, size);
    shape.set_margin_end(beyond_x);
    shape.set_margin_bottom(beyond_y);
    overlay.add_overlay(&shape);
    if hot && config.corner_visualize {
        let mark = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        mark.add_css_class("corner-region");
        mark.set_size_request(config.corner_width, config.corner_height);
        mark.set_halign(side(corner.left()));
        mark.set_valign(end(corner.top()));
        mark.set_margin_end(beyond_x);
        mark.set_margin_bottom(beyond_y);
        overlay.add_overlay(&mark);
    }

    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&overlay)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Overlay);
    window.set_anchor(
        if corner.top() {
            Edge::Top
        } else {
            Edge::Bottom
        },
        true,
    );
    window.set_anchor(
        if corner.left() {
            Edge::Left
        } else {
            Edge::Right
        },
        true,
    );
    window.set_margin(Edge::Right, -beyond_x);
    window.set_margin(Edge::Bottom, -beyond_y);
    window.set_exclusive_zone(-1);

    let region = if hot {
        gtk4::cairo::RectangleInt::new(
            if corner.left() {
                0
            } else {
                width - config.corner_width
            },
            if corner.top() {
                0
            } else {
                height - config.corner_height
            },
            config.corner_width,
            config.corner_height,
        )
    } else {
        gtk4::cairo::RectangleInt::new(0, 0, 0, 0)
    };
    window.connect_realize(move |window| {
        if let Some(surface) = window.surface() {
            surface.set_input_region(Some(&cairo::Region::create_rectangle(&region)));
        }
    });

    if hot {
        reach(&overlay, config, services, sidebar, corner);
    }

    window.present();
    window
}

fn reach(
    overlay: &gtk4::Overlay,
    config: &Rc<Config>,
    services: &Rc<Services>,
    sidebar: &Rc<Sidebar>,
    corner: Corner,
) {
    let at_end = Rc::new(Cell::new(false));
    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let sidebar = sidebar.clone();
        let config = config.clone();
        let at_end = at_end.clone();
        move |controller, x, y| {
            if config.corner_clickless {
                if corner.opens_sidebar() {
                    sidebar.open();
                }
                return;
            }
            corner_end(controller, &config, &sidebar, &at_end, corner, x, y);
        }
    });
    hover.connect_motion({
        let sidebar = sidebar.clone();
        let config = config.clone();
        let at_end = at_end.clone();
        move |controller, x, y| {
            corner_end(controller, &config, &sidebar, &at_end, corner, x, y);
        }
    });
    hover.connect_leave({
        let at_end = at_end.clone();
        move |_| at_end.set(false)
    });
    overlay.add_controller(hover);

    let press = gtk4::GestureClick::new();
    press.set_button(gdk::BUTTON_PRIMARY);
    press.connect_pressed({
        let sidebar = sidebar.clone();
        move |_, _, _, _| {
            if corner.opens_sidebar() {
                sidebar.toggle();
            }
        }
    });
    overlay.add_controller(press);

    if !config.corner_value_scroll {
        return;
    }
    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll({
        let services = services.clone();
        move |_, _, delta| {
            if corner.left() {
                if delta < 0.0 {
                    services.light.raise();
                } else {
                    services.light.lower();
                }
            } else if let Some(audio) = &services.audio {
                let level = audio.sink_volume.get();
                let step = if level < 0.1 { 0.01 } else { 0.02 };
                audio.set_sink_volume(if delta < 0.0 {
                    (level + step).min(1.0)
                } else {
                    level - step
                });
            }
            glib::Propagation::Stop
        }
    });
    overlay.add_controller(wheel);
    let kind = if corner.left() {
        Osd::Brightness
    } else {
        Osd::Volume
    };
    crate::panels::bar::watch_moved_away(overlay, &services.states, kind);
}

fn corner_end(
    controller: &gtk4::EventControllerMotion,
    config: &Config,
    sidebar: &Rc<Sidebar>,
    at_end: &Rc<Cell<bool>>,
    corner: Corner,
    x: f64,
    y: f64,
) {
    if !config.corner_clickless_end {
        return;
    }
    let Some(widget) = controller.widget() else {
        return;
    };
    let offset = config.corner_vertical_offset as f64;
    let along = if corner.left() {
        x <= EDGE_REACH
    } else {
        x >= widget.width() as f64 - EDGE_REACH
    };
    let across = if corner.top() {
        y > offset
    } else {
        y < widget.height() as f64 - offset
    };
    let reached = along && across;
    if reached && !at_end.get() && corner.opens_sidebar() {
        sidebar.open();
    }
    at_end.set(reached);
}

fn paint(corner: Corner, size: i32) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_halign(side(corner.left()));
    area.set_valign(end(corner.top()));
    area.set_draw_func(move |_, cr, width, height| {
        let radius = width.min(height) as f64;
        let (width, height) = (width as f64, height as f64);
        cr.set_source_rgba(0.0, 0.0, 0.0, 1.0);
        match corner {
            Corner::TopLeft => {
                cr.move_to(0.0, 0.0);
                cr.arc(radius, radius, radius, PI, 1.5 * PI);
            }
            Corner::TopRight => {
                cr.move_to(width, 0.0);
                cr.arc(width - radius, radius, radius, 1.5 * PI, 2.0 * PI);
            }
            Corner::BottomLeft => {
                cr.move_to(0.0, height);
                cr.arc(radius, height - radius, radius, 0.5 * PI, PI);
            }
            Corner::BottomRight => {
                cr.move_to(width, height);
                cr.arc(width - radius, height - radius, radius, 0.0, 0.5 * PI);
            }
        }
        cr.close_path();
        let _ = cr.fill();
    });
    area
}

fn side(left: bool) -> gtk4::Align {
    if left {
        gtk4::Align::Start
    } else {
        gtk4::Align::End
    }
}

fn end(top: bool) -> gtk4::Align {
    if top {
        gtk4::Align::Start
    } else {
        gtk4::Align::End
    }
}
