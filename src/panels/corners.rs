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
use crate::services::session::states::Osd;

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
    fn is_top(self) -> bool {
        matches!(self, Corner::TopLeft | Corner::TopRight)
    }

    fn is_left(self) -> bool {
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
        .map(|corner| corner_window(app, config, services, sidebar, monitor, *corner))
        .collect();

    let connector: String = monitor.connector().map(Into::into).unwrap_or_default();
    let shows_over_fullscreen = config.fake_rounding == 1;
    let show = {
        let windows = windows.clone();
        let fullscreen = services.fullscreen.clone();
        move || {
            let is_covered = !shows_over_fullscreen && fullscreen.covers(&connector);
            for window in &windows {
                window.set_visible(!is_covered);
            }
        }
    };
    show();
    scope.keep(services.fullscreen.subscribe(show));

    windows
}

fn corner_window(
    app: &gtk4::Application,
    config: &Rc<Config>,
    services: &Rc<Services>,
    sidebar: &Rc<Sidebar>,
    monitor: &gdk::Monitor,
    corner: Corner,
) -> gtk4::ApplicationWindow {
    let size = config.hug_rounding().max(1);
    let is_hot = config.corner_open && config.corner_bottom != corner.is_top();
    let (width, height) = if is_hot {
        (
            size.max(config.corner_width),
            size.max(config.corner_height),
        )
    } else {
        (size, size)
    };

    let beyond_x = i32::from(config.dead_pixel && !corner.is_left());
    let beyond_y = i32::from(config.dead_pixel && !corner.is_top());
    let base = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    base.set_size_request(width + beyond_x, height + beyond_y);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&base));

    let shape = paint(corner, size);
    shape.set_margin_end(beyond_x);
    shape.set_margin_bottom(beyond_y);
    overlay.add_overlay(&shape);
    if is_hot && config.corner_visualize {
        let region_mark = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        region_mark.add_css_class("corner-region");
        region_mark.set_size_request(config.corner_width, config.corner_height);
        region_mark.set_halign(horizontal_align(corner.is_left()));
        region_mark.set_valign(vertical_align(corner.is_top()));
        region_mark.set_margin_end(beyond_x);
        region_mark.set_margin_bottom(beyond_y);
        overlay.add_overlay(&region_mark);
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
        if corner.is_top() {
            Edge::Top
        } else {
            Edge::Bottom
        },
        true,
    );
    window.set_anchor(
        if corner.is_left() {
            Edge::Left
        } else {
            Edge::Right
        },
        true,
    );
    window.set_margin(Edge::Right, -beyond_x);
    window.set_margin(Edge::Bottom, -beyond_y);
    window.set_exclusive_zone(-1);

    let region = if is_hot {
        gtk4::cairo::RectangleInt::new(
            if corner.is_left() {
                0
            } else {
                width - config.corner_width
            },
            if corner.is_top() {
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

    if is_hot {
        connect_reach(&overlay, config, services, sidebar, corner);
    }

    window.present();
    window
}

fn connect_reach(
    overlay: &gtk4::Overlay,
    config: &Rc<Config>,
    services: &Rc<Services>,
    sidebar: &Rc<Sidebar>,
    corner: Corner,
) {
    let is_at_end = Rc::new(Cell::new(false));
    let hover_controller = gtk4::EventControllerMotion::new();
    hover_controller.connect_enter({
        let sidebar = sidebar.clone();
        let config = config.clone();
        let is_at_end = is_at_end.clone();
        move |controller, x, y| {
            if config.corner_clickless {
                if corner.opens_sidebar() {
                    sidebar.open();
                }
                return;
            }
            update_corner_end(controller, &config, &sidebar, &is_at_end, corner, x, y);
        }
    });
    hover_controller.connect_motion({
        let sidebar = sidebar.clone();
        let config = config.clone();
        let is_at_end = is_at_end.clone();
        move |controller, x, y| {
            update_corner_end(controller, &config, &sidebar, &is_at_end, corner, x, y);
        }
    });
    hover_controller.connect_leave({
        let is_at_end = is_at_end.clone();
        move |_| is_at_end.set(false)
    });
    overlay.add_controller(hover_controller);

    let press_gesture = gtk4::GestureClick::new();
    press_gesture.set_button(gdk::BUTTON_PRIMARY);
    press_gesture.connect_pressed({
        let sidebar = sidebar.clone();
        move |_, _, _, _| {
            if corner.opens_sidebar() {
                sidebar.toggle();
            }
        }
    });
    overlay.add_controller(press_gesture);

    if !config.corner_value_scroll {
        return;
    }
    let scroll_controller =
        gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    scroll_controller.connect_scroll({
        let services = services.clone();
        move |_, _, delta| {
            if corner.is_left() {
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
    overlay.add_controller(scroll_controller);
    let kind = if corner.is_left() {
        Osd::Brightness
    } else {
        Osd::Volume
    };
    crate::panels::bar::watch_moved_away(overlay, &services.states, kind);
}

fn update_corner_end(
    controller: &gtk4::EventControllerMotion,
    config: &Config,
    sidebar: &Rc<Sidebar>,
    is_at_end: &Rc<Cell<bool>>,
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
    let is_at_side_edge = if corner.is_left() {
        x <= EDGE_REACH
    } else {
        x >= widget.width() as f64 - EDGE_REACH
    };
    let is_past_offset = if corner.is_top() {
        y > offset
    } else {
        y < widget.height() as f64 - offset
    };
    let is_reached = is_at_side_edge && is_past_offset;
    if is_reached && !is_at_end.get() && corner.opens_sidebar() {
        sidebar.open();
    }
    is_at_end.set(is_reached);
}

fn paint(corner: Corner, size: i32) -> gtk4::DrawingArea {
    let area = gtk4::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_halign(horizontal_align(corner.is_left()));
    area.set_valign(vertical_align(corner.is_top()));
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

fn horizontal_align(is_left: bool) -> gtk4::Align {
    if is_left {
        gtk4::Align::Start
    } else {
        gtk4::Align::End
    }
}

fn vertical_align(is_top: bool) -> gtk4::Align {
    if is_top {
        gtk4::Align::Start
    } else {
        gtk4::Align::End
    }
}
