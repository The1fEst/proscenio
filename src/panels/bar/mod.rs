pub mod battery;
pub mod bluetooth;
pub mod clock;
pub mod keyboard;
pub mod media;
pub mod mute;
pub mod network;
pub mod recording;
pub mod resources;
pub mod strip;
pub mod tray;
pub mod traymenu;
pub mod utilbuttons;
pub mod vertical;
pub mod weather;
pub mod wireguard;
pub mod workspaces;

use gtk4::prelude::*;
use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::panels::notifications::indicator;
use crate::services::Services;
use crate::services::states::{Osd, States};
use crate::ui::theme::{SharedTheme, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::scrollhint;
use strip::Strip;

const GROUP_HEIGHT: i32 = 40;
const GROUP_INSET: i32 = 4;
const GROUP_PADDING: i32 = 5;
const INDICATOR_SPACING: i32 = 15;
pub const VERTICAL_INDICATOR_SPACING: i32 = 6;
const VERTICAL_GROUP_SPACING: i32 = 12;

pub struct Screen<'a> {
    pub connector: &'a str,
    pub width: i32,
}

pub fn build(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Services,
    sidebar: &Rc<crate::panels::sidebar::Sidebar>,
    calendar: &Rc<crate::panels::calendar::Calendar>,
    media: &Rc<crate::panels::mediacontrols::MediaControls>,
    overview: &Rc<crate::panels::overview::Overview>,
    screen: Screen<'_>,
    scope: &Scope,
) -> gtk4::Widget {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);

    let strip = Strip::new(
        &left_section(config, services, theme, scope),
        &middle_section(
            config, theme, services, sidebar, calendar, media, overview, &screen, scope,
        ),
        &right_section(config, theme, services, sidebar, &screen, scope),
    );
    let background = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    if config.show_background {
        background.add_css_class("bar");
    }
    if config.floating() {
        background.add_css_class("floating");
        if config.float_shadow && config.show_background {
            background.add_css_class("shadowed");
        }
    }
    let content = gtk4::Overlay::new();
    content.set_child(Some(&background));
    content.add_overlay(&strip);
    content.set_size_request(-1, config.bar_height());

    let corners = hug_corners(config, theme);
    if config.bottom {
        column.append(&corners);
        column.append(&content);
    } else {
        column.append(&content);
        column.append(&corners);
    }
    column.upcast()
}

fn watch_hint(area: &impl IsA<gtk4::Widget>, hint: &Rc<scrollhint::Hint>) {
    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let hint = hint.clone();
        move |_, _, _| hint.show(true)
    });
    hover.connect_leave({
        let hint = hint.clone();
        move |_| hint.show(false)
    });
    area.add_controller(hover);
}

const MOVE_THRESHOLD: f64 = 20.0;

pub fn watch_moved_away(area: &impl IsA<gtk4::Widget>, states: &States, kind: Osd) {
    let pointer = Rc::new(Cell::new((0.0, 0.0)));
    let anchor: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));

    let motion = gtk4::EventControllerMotion::new();
    motion.connect_motion({
        let pointer = pointer.clone();
        let anchor = anchor.clone();
        let states = states.clone();
        move |_, x, y| {
            pointer.set((x, y));
            if let Some((from_x, from_y)) = anchor.get()
                && (x - from_x).hypot(y - from_y) > MOVE_THRESHOLD
            {
                anchor.set(None);
                states.close_osd(kind);
            }
        }
    });
    motion.connect_leave({
        let anchor = anchor.clone();
        let states = states.clone();
        move |_| {
            if anchor.take().is_some() {
                states.close_osd(kind);
            }
        }
    });
    area.add_controller(motion);

    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk4::PropagationPhase::Capture);
    wheel.connect_scroll(move |_, _, _| {
        anchor.set(Some(pointer.get()));
        gtk4::glib::Propagation::Proceed
    });
    area.add_controller(wheel);
}

fn left_section(
    config: &Rc<Config>,
    services: &Services,
    theme: &SharedTheme,
    scope: &Scope,
) -> gtk4::Widget {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.set_hexpand(true);
    row.set_halign(gtk4::Align::Fill);
    if config.weather_enable {
        let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        row.append(&spacer);
        let pill = Group::new(config);
        pill.holder.set_margin_end(4);
        pill.append(&crate::panels::bar::weather::build(
            &services.weather,
            config,
            scope,
        ));
        row.append(&pill.holder);
    }

    let light = services.light.clone();
    let hint = Rc::new(scrollhint::Hint::new(
        if light.gamma.get() == 100.0 {
            "light_mode"
        } else {
            "wb_twilight"
        },
        "Scroll to change brightness",
        theme,
    ));
    let area = gtk4::Overlay::new();
    area.set_child(Some(&row));
    let slot = Centred::new(&hint.holder);
    slot.set_halign(gtk4::Align::Start);
    area.add_overlay(&slot);
    watch_hint(&area, &hint);
    watch_moved_away(&area, &services.states, Osd::Brightness);

    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll(move |_, _, delta| {
        if delta < 0.0 {
            light.raise();
        } else {
            light.lower();
        }
        hint.set_icon(if light.gamma.get() == 100.0 {
            "light_mode"
        } else {
            "wb_twilight"
        });
        gtk4::glib::Propagation::Stop
    });
    area.add_controller(wheel);

    area.upcast()
}

fn middle_section(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Services,
    sidebar: &Rc<crate::panels::sidebar::Sidebar>,
    calendar: &Rc<crate::panels::calendar::Calendar>,
    media: &Rc<crate::panels::mediacontrols::MediaControls>,
    overview: &Rc<crate::panels::overview::Overview>,
    screen: &Screen<'_>,
    scope: &Scope,
) -> gtk4::Widget {
    let shortened = config.shortened(screen.width);
    let side_width = config.center_side_width(screen.width);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    row.set_halign(gtk4::Align::Center);
    row.set_hexpand(false);

    let left = Group::new(config);
    left.holder.set_size_request(side_width, GROUP_HEIGHT);
    if shortened < 2 {
        let media = media.clone();
        left.append(&media::build(
            &services.mpris,
            theme,
            move || media.toggle(),
            false,
            scope,
        ));
    }
    left.append(&resources::build(theme, config, &services.resources, scope));

    let center = Group::new(config);
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

    let right = Group::new(config);
    right.holder.set_size_request(side_width, GROUP_HEIGHT);

    let time = clock::build(
        config,
        config.verbose && shortened < 2,
        &services.background,
        scope,
    );
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
    right.append(&time);
    right.append(&crate::panels::bar::recording::build(
        &services.recording,
        theme,
        scope,
    ));
    if shortened == 0 && config.verbose {
        right.append(&utilbuttons::build(config, services, theme, scope));
    }
    if shortened < 2 {
        right.append(&battery::build(&services.battery, theme, config, scope));
    }

    let open = gtk4::GestureClick::new();
    open.set_button(gtk4::gdk::BUTTON_PRIMARY);
    open.connect_pressed({
        let sidebar = Rc::downgrade(sidebar);
        move |gesture, _, x, y| {
            if let (Some(sidebar), Some(holder)) = (sidebar.upgrade(), gesture.widget())
                && !over_button(&holder, x, y)
            {
                sidebar.toggle();
            }
        }
    });
    right.holder.add_controller(open);

    let (left, center, right) = (left.holder, center.holder, right.holder);
    row.append(&left);
    if config.borderless {
        row.append(&separator());
    }
    row.append(&center);
    if config.borderless {
        row.append(&separator());
    }
    row.append(&right);
    row.upcast()
}

fn right_section(
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Services,
    sidebar: &Rc<crate::panels::sidebar::Sidebar>,
    screen: &Screen<'_>,
    scope: &Scope,
) -> gtk4::Widget {
    let area = gtk4::Overlay::new();
    area.set_hexpand(true);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    row.set_hexpand(true);
    row.set_halign(gtk4::Align::Fill);
    area.set_child(Some(&row));

    let indicators = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    indicators.set_halign(gtk4::Align::Center);
    indicators.set_valign(gtk4::Align::Center);

    let button = RippleButton::new(theme);
    button.add_css_class("bar-indicators");
    button.set_size_request(-1, -1);
    button.set_radius(rounding::FULL as f64);
    button.set_look(indicator_look(false));
    button.set_content(&indicators, 10, 5);
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

    let hint = Rc::new(scrollhint::Hint::new(
        "volume_up",
        "Scroll to change volume",
        theme,
    ));
    let slot = Centred::new(&hint.holder);
    slot.set_halign(gtk4::Align::End);
    area.add_overlay(&slot);

    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let button = button.clone();
        let hint = hint.clone();
        move |_, _, _| {
            button.set_look(indicator_look(true));
            hint.show(true);
        }
    });
    hover.connect_leave({
        let button = button.clone();
        let hint = hint.clone();
        move |_| {
            button.set_look(indicator_look(false));
            hint.show(false);
        }
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

    if let Some(audio) = &services.audio {
        let (sink, source) = mute::build(audio, false, scope);
        indicators.append(&sink);
        indicators.append(&source);
    }
    let layout = keyboard::build(&services.xkb, scope);
    layout.set_margin_end(INDICATOR_SPACING);
    indicators.append(&layout);
    indicators.append(&indicator::build(
        &services.notifications,
        theme,
        config,
        false,
        scope,
    ));
    indicators.append(&network::build(&services.net, scope));
    indicators.append(&wireguard::build(&services.net, scope));
    let bluetooth = bluetooth::build(&services.bluez, scope);
    bluetooth.set_margin_start(INDICATOR_SPACING);
    indicators.append(&bluetooth);

    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    row.append(&spacer);
    if let Some(bus) = &services.session_bus {
        let collapsed = config.shortened(screen.width) > 0;
        row.append(&tray::build(
            bus, config, theme, scope, None, false, collapsed,
        ));
    }
    let slot = Centred::new(&button);
    slot.set_margin_end(rounding::SCREEN_ROUNDING);
    row.append(&slot);
    area.upcast()
}

fn over_button(area: &impl IsA<gtk4::Widget>, x: f64, y: f64) -> bool {
    let area = area.as_ref();
    let mut picked = area.pick(x, y, gtk4::PickFlags::DEFAULT);
    while let Some(widget) = picked {
        if &widget == area {
            return false;
        }
        if widget.is::<gtk4::Button>() || widget.has_css_class("press-sink") {
            return true;
        }
        picked = widget.parent();
    }
    false
}

fn indicator_look(area_hovered: bool) -> Look {
    Look {
        background: if area_hovered {
            |theme| theme.colors.col_layer1_hover
        } else {
            |theme| transparentize(theme.colors.col_layer1_hover, 1.0)
        },
        hover: |theme| theme.colors.col_layer1_hover,
        ripple: |theme| theme.colors.col_layer1_active,
        toggled: |theme| theme.colors.col_secondary_container,
        toggled_hover: |theme| theme.colors.col_secondary_container_hover,
        ripple_toggled: |theme| theme.colors.col_secondary_container_active,
    }
}

struct Group {
    holder: gtk4::Overlay,
    row: gtk4::Box,
}

impl Group {
    fn new(config: &Rc<Config>) -> Self {
        let background = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        background.set_margin_top(GROUP_INSET);
        background.set_margin_bottom(GROUP_INSET);
        if !config.borderless {
            background.add_css_class("group");
        }

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
        row.set_margin_start(GROUP_PADDING);
        row.set_margin_end(GROUP_PADDING);
        row.set_valign(gtk4::Align::Center);

        let holder = gtk4::Overlay::new();
        holder.set_child(Some(&background));
        holder.add_overlay(&row);
        holder.set_measure_overlay(&row, true);
        holder.set_valign(gtk4::Align::Center);
        holder.set_size_request(-1, GROUP_HEIGHT);
        holder.set_hexpand(false);
        Group { holder, row }
    }

    fn vertical(config: &Rc<Config>, padding: i32) -> Self {
        let background = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        background.set_margin_start(GROUP_INSET);
        background.set_margin_end(GROUP_INSET);
        if !config.borderless {
            background.add_css_class("group");
        }

        let row = gtk4::Box::new(gtk4::Orientation::Vertical, VERTICAL_GROUP_SPACING);
        row.set_margin_top(padding);
        row.set_margin_bottom(padding);

        let holder = gtk4::Overlay::new();
        holder.set_child(Some(&background));
        holder.add_overlay(&row);
        holder.set_measure_overlay(&row, true);
        holder.set_halign(gtk4::Align::Center);
        holder.set_size_request(crate::core::config::BASE_VERTICAL_BAR_WIDTH, -1);
        Group { holder, row }
    }

    fn append(&self, child: &impl IsA<gtk4::Widget>) {
        self.row.append(child);
    }
}

fn separator() -> gtk4::Widget {
    let line = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    line.add_css_class("separator");
    line.set_size_request(1, -1);
    line.set_margin_top(crate::core::config::BASE_BAR_HEIGHT / 3);
    line.set_margin_bottom(crate::core::config::BASE_BAR_HEIGHT / 3);
    line.upcast()
}

fn horizontal_separator() -> gtk4::Widget {
    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    line.add_css_class("separator");
    line.set_size_request(-1, 1);
    line.set_margin_start(crate::core::config::BASE_BAR_HEIGHT / 3);
    line.set_margin_end(crate::core::config::BASE_BAR_HEIGHT / 3);
    line.upcast()
}

fn scroll_volume(area: &impl IsA<gtk4::Widget>, services: &Services) {
    let Some(audio) = services.audio.clone() else {
        return;
    };
    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll(move |_, _, delta| {
        let level = audio.sink_volume.get();
        let step = if level < 0.1 { 0.01 } else { 0.02 };
        audio.set_sink_volume(if delta < 0.0 {
            (level + step).min(1.0)
        } else {
            level - step
        });
        gtk4::glib::Propagation::Stop
    });
    area.as_ref().add_controller(wheel);
}

fn hug_corners(config: &Rc<Config>, theme: &SharedTheme) -> gtk4::Widget {
    let area = gtk4::DrawingArea::new();
    let radius = config.hug_rounding();
    area.set_content_height(radius);
    if radius == 0 || !config.hug_corners() {
        return area.upcast();
    }

    let theme = theme.clone();
    let from_top = !config.bottom;
    area.set_draw_func(move |_, cr, width, height| {
        let radius = height as f64;
        let width = width as f64;
        let color = theme.borrow().colors.col_layer0;
        cr.set_source_rgba(
            color.red() as f64,
            color.green() as f64,
            color.blue() as f64,
            color.alpha() as f64,
        );
        if !from_top {
            cr.translate(0.0, height as f64);
            cr.scale(1.0, -1.0);
        }
        cr.move_to(0.0, 0.0);
        cr.arc(radius, radius, radius, PI, 1.5 * PI);
        cr.close_path();
        cr.move_to(width, 0.0);
        cr.arc(width - radius, radius, radius, 1.5 * PI, 2.0 * PI);
        cr.close_path();
        let _ = cr.fill();
    });
    area.upcast()
}
