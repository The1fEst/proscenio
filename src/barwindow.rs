use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};

use crate::connector;
use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::panels::overview;
use crate::panels::{bar, calendar, leftsidebar, mediacontrols, mixer, settings, sidebar};
use crate::services::Services;
use crate::ui::theme;
use crate::ui::{anim, reserve, widgets};

const NAMESPACE: &str = "proscenio:bar";
const VERTICAL_NAMESPACE: &str = "proscenio:verticalBar";
const RESERVE_NAMESPACE: &str = "proscenio:barReserve";

pub fn open_bar(
    app: &gtk4::Application,
    config: &Rc<Config>,
    theme: &theme::SharedTheme,
    services: &Services,
    sidebar: &Rc<sidebar::Sidebar>,
    left_sidebar: &Rc<leftsidebar::LeftSidebar>,
    calendar: &Rc<calendar::Calendar>,
    media: &Rc<mediacontrols::MediaControls>,
    volume_mixer: &Option<Rc<mixer::VolumeMixer>>,
    overview: &Rc<overview::Overview>,
    settings: &Rc<settings::Settings>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> gtk4::ApplicationWindow {
    let screen = bar::Screen {
        connector: &connector(monitor),
        width: monitor.geometry().width(),
    };
    let body = if config.vertical {
        bar::vertical::build(
            config,
            theme,
            services,
            sidebar,
            left_sidebar,
            calendar,
            media,
            volume_mixer,
            overview,
            settings,
            screen,
            scope,
        )
    } else {
        bar::build(
            config,
            theme,
            services,
            sidebar,
            left_sidebar,
            calendar,
            media,
            volume_mixer,
            overview,
            settings,
            screen,
            scope,
        )
    };
    let content = widgets::display::text::Shift::new(&body);
    let edge_alignment = if config.bottom {
        gtk4::Align::End
    } else {
        gtk4::Align::Start
    };
    let builder = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&content);
    let window = if config.vertical {
        content.set_sideways(true);
        content.set_halign(edge_alignment);
        builder.default_width(config.surface_thickness()).build()
    } else {
        content.set_valign(edge_alignment);
        builder.default_height(config.surface_thickness()).build()
    };

    let edge = match (config.vertical, config.bottom) {
        (false, false) => Edge::Top,
        (false, true) => Edge::Bottom,
        (true, false) => Edge::Left,
        (true, true) => Edge::Right,
    };
    window.init_layer_shell();
    window.set_namespace(Some(if config.vertical {
        VERTICAL_NAMESPACE
    } else {
        NAMESPACE
    }));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    if config.vertical {
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
    } else {
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
    }
    window.set_anchor(edge, true);
    if config.dead_pixel && !config.vertical {
        window.set_margin(Edge::Right, -1);
        if config.bottom {
            window.set_margin(Edge::Bottom, -1);
        }
    }
    window.set_exclusive_zone(config.exclusive_zone());

    let offset = anim::Motion::new(&content, 0.0, 200.0, anim::EXPRESSIVE_EFFECTS);
    let is_hovered = Rc::new(Cell::new(false));
    let is_shown_by_super = Rc::new(Cell::new(false));
    let place = {
        let config = config.clone();
        let window = window.downgrade();
        let content = content.downgrade();
        let offset = offset.clone();
        move || {
            let (Some(content), Some(window)) = (content.upgrade(), window.upgrade()) else {
                return;
            };
            let shift = offset.get() as i32;
            content.set_offset(shift as f32);
            let hover_width = config.hover_region_width;
            let strip_start = if config.bottom {
                config.hug_rounding() + shift - hover_width
            } else {
                shift - hover_width
            };
            let thickness = config.bar_thickness() + hover_width * 2;
            let strip = if config.vertical {
                gtk4::cairo::RectangleInt::new(strip_start, 0, thickness, i32::MAX / 2)
            } else {
                gtk4::cairo::RectangleInt::new(0, strip_start, i32::MAX / 2, thickness)
            };
            if let Some(surface) = window.surface() {
                surface.set_input_region(Some(&gtk4::cairo::Region::create_rectangle(&strip)));
            }
        }
    };
    let place = Rc::new(place);
    window.connect_realize({
        let place = place.clone();
        move |_| place()
    });

    let update = {
        let config = config.clone();
        let window = window.downgrade();
        let content = content.downgrade();
        let offset = offset.clone();
        let is_hovered = is_hovered.clone();
        let is_shown_by_super = is_shown_by_super.clone();
        let place = place.clone();
        move || {
            if !config.auto_hide {
                return;
            }
            let (Some(content), Some(window)) = (content.upgrade(), window.upgrade()) else {
                return;
            };
            let must_show = is_hovered.get() || is_shown_by_super.get();
            window.set_exclusive_zone(if !must_show || !config.auto_hide_push_windows {
                0
            } else {
                config.exclusive_zone()
            });
            let hidden_offset = if config.bottom {
                config.bar_thickness()
            } else {
                -config.bar_thickness()
            };
            offset.to(if must_show { 0.0 } else { hidden_offset as f64 });
            let place = place.clone();
            let offset = offset.clone();
            content.add_tick_callback(move |_, _| {
                place();
                if offset.is_running() {
                    gtk4::glib::ControlFlow::Continue
                } else {
                    gtk4::glib::ControlFlow::Break
                }
            });
        }
    };
    let update = Rc::new(update);
    if config.auto_hide {
        let hidden_offset = if config.bottom {
            config.bar_thickness()
        } else {
            -config.bar_thickness()
        };
        offset.jump(hidden_offset as f64);
        window.set_exclusive_zone(0);
    }

    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let is_hovered = is_hovered.clone();
        let update = update.clone();
        move |_, _, _| {
            is_hovered.set(true);
            update();
        }
    });
    hover.connect_leave({
        let is_hovered = is_hovered.clone();
        let update = update.clone();
        move |_| {
            is_hovered.set(false);
            update();
        }
    });
    window.add_controller(hover);

    let reserve = reserve::Reserve::new(app, monitor, edge, RESERVE_NAMESPACE);
    let privacy_dots = config
        .should_show_privacy_dots_in_fullscreen
        .then(|| bar::privacydots::PrivacyDots::new(app, monitor, config, &services.device_usage));
    let show: Rc<dyn Fn()> = Rc::new({
        let states = services.states.clone();
        let fullscreen = services.fullscreen.clone();
        let connector = connector(monitor);
        let window = window.clone();
        move || {
            let is_bar_requested = states.is_bar_open.get() && !states.is_screen_locked.get();
            if is_bar_requested && fullscreen.covers(&connector) {
                reserve.hold(window.exclusive_zone());
                window.set_visible(false);
                if let Some(privacy_dots) = &privacy_dots {
                    privacy_dots.set_covered(true);
                }
                return;
            }
            window.set_visible(is_bar_requested);
            reserve.release();
            if let Some(privacy_dots) = &privacy_dots {
                privacy_dots.set_covered(false);
            }
        }
    });
    scope.keep(services.fullscreen.subscribe({
        let show = show.clone();
        move || show()
    }));

    let super_show_timer: Rc<RefCell<Option<gtk4::glib::SourceId>>> = Rc::new(RefCell::new(None));
    scope.keep(services.states.subscribe({
        let states = services.states.clone();
        let config = config.clone();
        let show = show.clone();
        move || {
            show();
            if !config.super_show {
                return;
            }
            if let Some(timer) = super_show_timer.borrow_mut().take() {
                timer.remove();
            }
            if states.is_super_down.get() {
                let is_shown_by_super = is_shown_by_super.clone();
                let update = update.clone();
                let timer_slot = super_show_timer.clone();
                let delay = std::time::Duration::from_millis(config.super_show_delay as u64);
                super_show_timer.replace(Some(gtk4::glib::timeout_add_local_once(
                    delay,
                    move || {
                        timer_slot.replace(None);
                        is_shown_by_super.set(true);
                        update();
                    },
                )));
            } else if is_shown_by_super.replace(false) {
                update();
            }
        }
    }));

    show();
    window
}
