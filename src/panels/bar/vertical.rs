use gtk4::prelude::*;
use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::panels::mediacontrols::{MediaControls, clean_title};
use crate::panels::notifications::indicator;
use crate::services::Services;
use crate::services::states::Osd;
use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::popup;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::text;

use super::{
    Group, Screen, VERTICAL_INDICATOR_SPACING, battery, bluetooth, clock, horizontal_separator,
    indicator_look, keyboard, media, mute, network, over_button, resources, scroll_volume, tray,
    watch_moved_away, workspaces,
};

const MIDDLE_SPACING: i32 = 4;
const BOTTOM_SPACING: i32 = 4;
const SIDE_GROUP_PADDING: i32 = 8;
const WORKSPACES_GROUP_PADDING: i32 = 6;
const INDICATORS_HORIZONTAL_PADDING: i32 = 6;
const INDICATORS_VERTICAL_PADDING: i32 = 4;

#[allow(clippy::too_many_arguments)]
pub fn build(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Services,
    sidebar: &Rc<crate::panels::sidebar::Sidebar>,
    calendar: &Rc<crate::panels::calendar::Calendar>,
    media: &Rc<MediaControls>,
    overview: &Rc<crate::panels::overview::Overview>,
    screen: Screen<'_>,
    scope: &Scope,
) -> gtk4::Widget {
    let background = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    if config.show_background {
        background.add_css_class("bar");
    }
    if config.floating() {
        background.add_css_class("floating");
        if config.float_shadow && config.show_background {
            background.add_css_class("shadowed");
        }
    }

    let sections = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    sections.append(&top_section(services));
    sections.append(&middle_section(
        config, theme, services, calendar, media, overview, &screen, scope,
    ));
    let rest = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    rest.set_vexpand(true);
    sections.append(&rest);

    let content = gtk4::Overlay::new();
    content.set_child(Some(&background));
    content.add_overlay(&sections);
    content.add_overlay(&bottom_section(config, theme, services, sidebar, scope));
    content.set_size_request(config.vertical_bar_width(), -1);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    let corners = hug_corners(config, theme);
    if config.bottom {
        row.append(&corners);
        row.append(&content);
    } else {
        row.append(&content);
        row.append(&corners);
    }
    row.upcast()
}

fn top_section(services: &Services) -> gtk4::Widget {
    let area = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    area.set_vexpand(true);
    watch_moved_away(&area, &services.states, Osd::Brightness);

    let light = services.light.clone();
    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll(move |_, _, delta| {
        if delta < 0.0 {
            light.raise();
        } else {
            light.lower();
        }
        gtk4::glib::Propagation::Stop
    });
    area.add_controller(wheel);

    area.upcast()
}

#[allow(clippy::too_many_arguments)]
fn middle_section(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Services,
    calendar: &Rc<crate::panels::calendar::Calendar>,
    media: &Rc<MediaControls>,
    overview: &Rc<crate::panels::overview::Overview>,
    screen: &Screen<'_>,
    scope: &Scope,
) -> gtk4::Widget {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, MIDDLE_SPACING);
    column.set_halign(gtk4::Align::Center);

    let top = Group::vertical(config, SIDE_GROUP_PADDING);
    top.append(&resources::build(theme, config, &services.resources, scope));
    top.append(&horizontal_separator());
    top.append(&media_button(config, theme, services, media, scope));

    let center = Group::vertical(config, WORKSPACES_GROUP_PADDING);
    center.append(&workspaces::build(
        config,
        theme,
        services,
        screen.connector,
        {
            let overview = Rc::downgrade(overview);
            move || {
                if let Some(overview) = overview.upgrade() {
                    overview.toggle();
                }
            }
        },
        scope,
    ));

    let bottom = Group::vertical(config, SIDE_GROUP_PADDING);
    let time = clock::build(config, false, &services.background, scope);
    let open_calendar = gtk4::GestureClick::new();
    open_calendar.set_button(gtk4::gdk::BUTTON_PRIMARY);
    open_calendar.connect_pressed({
        let calendar = Rc::downgrade(calendar);
        move |gesture, _, _, _| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            if let (Some(calendar), Some(time)) = (calendar.upgrade(), gesture.widget()) {
                calendar.toggle(&time);
            }
        }
    });
    time.add_controller(open_calendar);
    bottom.append(&time);
    let battery_separator = horizontal_separator();
    let battery = battery::build(&services.battery, theme, config, scope);
    battery_separator.set_visible(battery.is_visible());
    battery.connect_visible_notify({
        let battery_separator = battery_separator.clone();
        move |battery| battery_separator.set_visible(battery.is_visible())
    });
    bottom.append(&battery_separator);
    bottom.append(&battery);

    column.append(&top.holder);
    if config.borderless {
        column.append(&horizontal_separator());
    }
    column.append(&center.holder);
    if config.borderless {
        column.append(&horizontal_separator());
    }
    column.append(&bottom.holder);
    column.upcast()
}

fn media_button(
    config: &Config,
    theme: &SharedTheme,
    services: &Services,
    controls: &Rc<MediaControls>,
    scope: &Scope,
) -> gtk4::Widget {
    let button = media::build(
        &services.mpris,
        theme,
        {
            let controls = controls.clone();
            move || controls.toggle()
        },
        true,
        scope,
    );

    let details = popup::column();
    details.append(&popup::header("music_note", "Media"));
    let track = text::styled("");
    text::set_color(&track, "colOnSurfaceVariant");
    track.set_xalign(0.0);
    details.append(&track);
    let show_track = {
        let mpris = services.mpris.clone();
        move || {
            let active = mpris.active();
            let title = active
                .as_ref()
                .map(|track| clean_title(&track.title))
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| "No media".to_owned());
            let artist = active.map(|track| track.artist).unwrap_or_default();
            track.set_text(&if artist.is_empty() {
                title
            } else {
                format!("{title}\n{artist}")
            });
        }
    };
    show_track();
    scope.keep(services.mpris.subscribe(show_track));

    let details = popup::Popup::new(&button, popup::Bar::of(config), &details);
    let hovered = Rc::new(Cell::new(false));
    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let details = details.clone();
        let hovered = hovered.clone();
        let controls = Rc::downgrade(controls);
        move |_, _, _| {
            hovered.set(true);
            details.show(
                controls
                    .upgrade()
                    .is_some_and(|controls| !controls.is_open()),
            );
        }
    });
    hover.connect_leave({
        let details = details.clone();
        let hovered = hovered.clone();
        move |_| {
            hovered.set(false);
            details.show(false);
        }
    });
    button.add_controller(hover);
    let follow = controls.window.connect_visible_notify(move |window| {
        details.show(hovered.get() && !window.is_visible());
    });
    scope.defer({
        let window = controls.window.downgrade();
        move || {
            if let Some(window) = window.upgrade() {
                window.disconnect(follow);
            }
        }
    });

    button
}

fn bottom_section(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Services,
    sidebar: &Rc<crate::panels::sidebar::Sidebar>,
    scope: &Scope,
) -> gtk4::Widget {
    let area = gtk4::Box::new(gtk4::Orientation::Vertical, BOTTOM_SPACING);
    area.set_valign(gtk4::Align::End);

    if let Some(bus) = &services.session_bus {
        area.append(&tray::build(bus, config, theme, scope, None, true, false));
    }

    let indicators = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    indicators.set_halign(gtk4::Align::Center);
    indicators.set_valign(gtk4::Align::Center);
    if let Some(audio) = &services.audio {
        let (sink, source) = mute::build(audio, true, scope);
        indicators.append(&sink);
        indicators.append(&source);
    }
    let layout = keyboard::build(&services.xkb, scope);
    layout.set_halign(gtk4::Align::Center);
    layout.set_margin_bottom(VERTICAL_INDICATOR_SPACING);
    indicators.append(&layout);
    indicators.append(&indicator::build(
        &services.notifications,
        theme,
        config,
        true,
        scope,
    ));
    indicators.append(&network::build(&services.net, scope));
    let bluetooth = bluetooth::build(&services.bluez, scope);
    bluetooth.set_margin_top(VERTICAL_INDICATOR_SPACING);
    indicators.append(&bluetooth);

    let button = RippleButton::new(theme);
    button.add_css_class("bar-indicators");
    button.set_size_request(-1, -1);
    button.set_radius(rounding::FULL as f64);
    button.set_look(indicator_look(false));
    button.set_content(
        &indicators,
        INDICATORS_HORIZONTAL_PADDING,
        INDICATORS_VERTICAL_PADDING,
    );
    button.set_halign(gtk4::Align::Center);
    button.set_margin_bottom(rounding::SCREEN_ROUNDING);
    button.connect_clicked({
        let sidebar = Rc::downgrade(sidebar);
        move |_| {
            if let Some(sidebar) = sidebar.upgrade() {
                sidebar.toggle();
            }
        }
    });
    sidebar.watch({
        let button = button.clone();
        move |open| {
            button.set_toggled(open);
            if open {
                button.add_css_class("toggled");
            } else {
                button.remove_css_class("toggled");
            }
        }
    });
    area.append(&button);

    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let button = button.clone();
        move |_, _, _| button.set_look(indicator_look(true))
    });
    hover.connect_leave({
        let button = button.clone();
        move |_| button.set_look(indicator_look(false))
    });
    area.add_controller(hover);
    watch_moved_away(&area, &services.states, Osd::Volume);

    let open = gtk4::GestureClick::new();
    open.set_button(gtk4::gdk::BUTTON_PRIMARY);
    open.connect_pressed({
        let sidebar = Rc::downgrade(sidebar);
        move |gesture, _, x, y| {
            if let (Some(sidebar), Some(area)) = (sidebar.upgrade(), gesture.widget())
                && !over_button(&area, x, y)
            {
                sidebar.toggle();
            }
        }
    });
    area.add_controller(open);
    scroll_volume(&area, services);

    area.upcast()
}

fn hug_corners(config: &Rc<Config>, theme: &SharedTheme) -> gtk4::Widget {
    let area = gtk4::DrawingArea::new();
    let radius = config.hug_rounding();
    area.set_content_width(radius);
    if radius == 0 || !config.hug_corners() {
        return area.upcast();
    }

    let theme = theme.clone();
    let right = config.bottom;
    area.set_draw_func(move |_, cr, width, height| {
        let radius = width as f64;
        let height = height as f64;
        let color = theme.borrow().colors.col_layer0;
        cr.set_source_rgba(
            color.red() as f64,
            color.green() as f64,
            color.blue() as f64,
            color.alpha() as f64,
        );
        if right {
            cr.translate(width as f64, 0.0);
            cr.scale(-1.0, 1.0);
        }
        cr.move_to(0.0, 0.0);
        cr.arc(radius, radius, radius, PI, 1.5 * PI);
        cr.close_path();
        cr.move_to(0.0, height);
        cr.arc(radius, height - radius, radius, 0.5 * PI, PI);
        cr.close_path();
        let _ = cr.fill();
    });
    area.upcast()
}
