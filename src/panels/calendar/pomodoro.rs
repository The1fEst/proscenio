use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::scope::Scope;
use crate::panels::calendar::laps::Laps;
use crate::services::session::timer::Timer;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Ease, Motion, Tween};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::flickable::Flickable;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::input::swipe::Swipe;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::secondarytabs::SecondaryTabs;

const TABS: [(&str, &str); 2] = [("search_activity", "Pomodoro"), ("timer", "Stopwatch")];
const PAGE_GAP: i32 = 10;
const CIRCLE_SIZE: i32 = 200;
const LINE_WIDTH: f64 = 8.0;
const GAP_ANGLE: f64 = 360.0 / 18.0;
const CIRCLE_MILLIS: f64 = 800.0;
const TIME_SIZE: i32 = 40;
const BADGE_SIZE: i32 = 36;
const BUTTON_WIDTH: i32 = 90;
const BUTTON_HEIGHT: i32 = 35;
const POMODORO_SPACING: i32 = 10;
const STOPWATCH_SPACING: i32 = 4;
const FRAME_TOP: i32 = 8;
const FRAME_SIDE: i32 = 16;
const ELAPSED_INDENT: i32 = 6;
const BUTTONS_BOTTOM: i32 = 6;
const LIST_MARGIN: i32 = 16;
const LAP_SPACING: i32 = 4;
const LAP_PADDING_X: i32 = 10;
const LAP_PADDING_Y: i32 = 6;
const LAP_ROW_SPACING: i32 = 5;
const ANCHOR_MILLIS: f64 = 200.0;

pub struct TimerPage {
    pub widget: gtk4::Widget,
    tabs: Rc<SecondaryTabs>,
    swipe: Swipe,
    timer: Timer,
    pomodoro: Rc<Pomodoro>,
    stopwatch: Rc<Stopwatch>,
}

impl TimerPage {
    pub fn reset(&self) {
        self.tabs.show(0, false);
        self.swipe.show(0, false);
        self.pomodoro.update(&self.timer, false);
        self.stopwatch.rebuild(&self.timer);
    }

    pub fn step(&self, delta: i32) {
        self.tabs.step(delta);
    }

    pub fn toggle(&self) {
        if self.tabs.current() == 0 {
            self.timer.toggle_pomodoro();
        } else {
            self.timer.toggle_stopwatch();
        }
    }

    pub fn restart(&self) {
        if self.tabs.current() == 0 {
            self.timer.reset_pomodoro();
        } else {
            self.timer.reset_stopwatch();
        }
    }

    pub fn lap(&self) {
        if self.timer.is_stopwatch_running() {
            self.timer.record_lap();
        }
    }
}

pub fn build(theme: &SharedTheme, timer: &Timer, scope: &Scope) -> Rc<TimerPage> {
    let tabs = SecondaryTabs::new(theme, &TABS);
    let swipe = Swipe::new(PAGE_GAP);
    swipe.set_vexpand(true);
    swipe.set_margin_top(PAGE_GAP);
    let pomodoro = Pomodoro::new(theme, timer);
    let stopwatch = Stopwatch::new(theme, timer);
    swipe.append(&pomodoro.widget);
    swipe.append(&stopwatch.widget);
    tabs.connect_changed({
        let swipe = swipe.clone();
        move |index| swipe.show(index, true)
    });
    swipe.connect_changed({
        let tabs = Rc::downgrade(&tabs);
        move |index| {
            if let Some(tabs) = tabs.upgrade() {
                tabs.show(index, true);
            }
        }
    });

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&tabs.widget);
    column.append(&swipe);

    scope.keep(timer.subscribe({
        let pomodoro = Rc::downgrade(&pomodoro);
        let stopwatch = Rc::downgrade(&stopwatch);
        let timer = timer.clone();
        move || {
            if let Some(pomodoro) = pomodoro.upgrade() {
                pomodoro.update(&timer, true);
            }
            if let Some(stopwatch) = stopwatch.upgrade() {
                stopwatch.update(&timer, true);
            }
        }
    }));

    Rc::new(TimerPage {
        widget: column.upcast(),
        tabs,
        swipe,
        timer: timer.clone(),
        pomodoro,
        stopwatch,
    })
}

struct Pomodoro {
    widget: gtk4::Overlay,
    canvas: gtk4::DrawingArea,
    progress: Rc<Motion>,
    time_label: gtk4::Label,
    phase_label: gtk4::Label,
    cycle_label: gtk4::Label,
    toggle_button: RippleButton,
    toggle_label: gtk4::Label,
    reset_button: RippleButton,
    last_is_running: Cell<Option<bool>>,
}

impl Pomodoro {
    fn new(theme: &SharedTheme, timer: &Timer) -> Rc<Self> {
        let canvas = gtk4::DrawingArea::new();
        canvas.set_content_width(CIRCLE_SIZE);
        canvas.set_content_height(CIRCLE_SIZE);
        let progress = Motion::new(&canvas, 360.0, CIRCLE_MILLIS, Ease::OutCubic);
        canvas.set_draw_func({
            let theme = theme.clone();
            let progress = progress.clone();
            move |_, cr, width, height| {
                let theme = theme.borrow();
                draw_circle(
                    cr,
                    width as f64 / 2.0,
                    height as f64 / 2.0,
                    progress.get(),
                    theme.colors.col_on_secondary_container,
                    theme.colors.col_secondary_container,
                );
            }
        });

        let time_label = text::styled_sized("", TIME_SIZE);
        text::set_color(&time_label, "m3onSurface");
        let phase_label = text::styled_sized("", pixel_size::NORMAL);
        text::set_color(&phase_label, "colSubtext");
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        content.append(&Centered::new(&time_label));
        content.append(&Centered::new(&phase_label));

        let cycle_label = text::styled("");
        text::set_color(&cycle_label, "colOnLayer2");
        let badge = Centered::integral(&cycle_label);
        badge.add_css_class("timer-cycle");
        badge.set_size_request(BADGE_SIZE, BADGE_SIZE);
        badge.set_halign(gtk4::Align::End);
        badge.set_valign(gtk4::Align::End);

        let circle = gtk4::Overlay::new();
        circle.set_child(Some(&canvas));
        circle.add_overlay(&Centered::integral(&content));
        circle.add_overlay(&badge);

        let (toggle_button, toggle_label) = button(theme);
        let (reset_button, reset_label) = button(theme);
        reset_label.set_text(&tr("Reset"));
        text::set_color(&reset_label, "colOnErrorContainer");
        reset_button.set_look(Look {
            background: |theme| theme.colors.col_error_container,
            hover: |theme| theme.colors.col_error_container_hover,
            ripple: |theme| theme.colors.col_error_container_active,
            ..Look::default()
        });
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, POMODORO_SPACING);
        buttons.append(&toggle_button);
        buttons.append(&reset_button);

        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&gtk4::Box::new(gtk4::Orientation::Vertical, 0)));
        widget.add_overlay(&circle);
        widget.add_overlay(&buttons);
        widget.connect_get_child_position({
            let circle = circle.clone().upcast::<gtk4::Widget>();
            move |widget, child| {
                let (width, height) = (widget.width(), widget.height());
                let total = CIRCLE_SIZE + BUTTON_HEIGHT;
                let extra = (height - total).max(0) as f64;
                let circle_area_height =
                    (CIRCLE_SIZE as f64 + extra * CIRCLE_SIZE as f64 / total as f64).round();
                let center = |outer: f64, inner: i32| ((outer - inner as f64) / 2.0).round() as i32;
                if *child == circle {
                    return Some(gtk4::gdk::Rectangle::new(
                        center(width as f64, CIRCLE_SIZE),
                        center(circle_area_height, CIRCLE_SIZE),
                        CIRCLE_SIZE,
                        CIRCLE_SIZE,
                    ));
                }
                let buttons_width = BUTTON_WIDTH * 2 + POMODORO_SPACING;
                Some(gtk4::gdk::Rectangle::new(
                    center(width as f64, buttons_width),
                    circle_area_height as i32
                        + center(height as f64 - circle_area_height, BUTTON_HEIGHT),
                    buttons_width,
                    BUTTON_HEIGHT,
                ))
            }
        });

        let pomodoro = Rc::new(Pomodoro {
            widget,
            canvas,
            progress,
            time_label,
            phase_label,
            cycle_label,
            toggle_button,
            toggle_label,
            reset_button,
            last_is_running: Cell::new(None),
        });
        pomodoro.toggle_button.connect_clicked({
            let timer = timer.clone();
            move |_| timer.toggle_pomodoro()
        });
        pomodoro.reset_button.connect_clicked({
            let timer = timer.clone();
            move |_| timer.reset_pomodoro()
        });
        pomodoro.update(timer, false);
        pomodoro
    }

    fn update(&self, timer: &Timer, should_animate: bool) {
        let seconds_left = timer.seconds_left();
        let duration_seconds = timer.lap_duration();
        let degree = seconds_left as f64 / duration_seconds as f64 * 360.0;
        if should_animate && self.canvas.is_mapped() {
            self.progress.to(degree);
        } else {
            self.progress.jump(degree);
        }
        self.canvas.queue_draw();

        self.time_label.set_text(&format!(
            "{:02}:{:02}",
            seconds_left / 60,
            seconds_left % 60
        ));
        self.phase_label
            .set_text(&tr(if timer.is_pomodoro_long_break() {
                "Long break"
            } else if timer.is_pomodoro_break() {
                "Break"
            } else {
                "Focus"
            }));
        self.cycle_label
            .set_text(&(timer.pomodoro_cycle() + 1).to_string());

        let is_running = timer.is_pomodoro_running();
        self.toggle_label.set_text(&tr(if is_running {
            "Pause"
        } else if seconds_left == timer.focus_time() {
            "Start"
        } else {
            "Resume"
        }));
        if self.last_is_running.replace(Some(is_running)) != Some(is_running) {
            text::set_color(
                &self.toggle_label,
                if is_running {
                    "colOnSecondaryContainer"
                } else {
                    "colOnPrimary"
                },
            );
            self.toggle_button.set_look(if is_running {
                Look {
                    background: |theme| theme.colors.col_secondary_container,
                    hover: |theme| theme.colors.col_secondary_container,
                    ..Look::default()
                }
            } else {
                Look {
                    background: |theme| theme.colors.col_primary,
                    hover: |theme| theme.colors.col_primary,
                    ..Look::default()
                }
            });
        }
        self.reset_button.set_sensitive(
            seconds_left < duration_seconds
                || timer.pomodoro_cycle() > 0
                || timer.is_pomodoro_break(),
        );
    }
}

struct Stopwatch {
    widget: gtk4::Overlay,
    time_label: gtk4::Label,
    centiseconds_label: gtk4::Label,
    laps: Laps,
    added_lap_count: Cell<usize>,
    toggle_button: RippleButton,
    toggle_label: gtk4::Label,
    second_button: RippleButton,
    second_label: gtk4::Label,
    last_is_running: Cell<Option<bool>>,
    top: Cell<Tween>,
    is_ticking: Cell<bool>,
}

impl Stopwatch {
    fn new(theme: &SharedTheme, timer: &Timer) -> Rc<Self> {
        let time_label = text::styled_sized("", TIME_SIZE);
        text::set_color(&time_label, "m3onSurface");
        let centiseconds_label = text::styled_sized("", TIME_SIZE);
        text::set_color(&centiseconds_label, "colSubtext");
        let elapsed_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        elapsed_box.append(&time_label);
        elapsed_box.append(&centiseconds_label);

        let laps = Laps::new(LAP_SPACING);
        let scroller = Flickable::new(theme);
        scroller.set_child(&laps);

        let (toggle_button, toggle_label) = button(theme);
        let (second_button, second_label) = button(theme);
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, STOPWATCH_SPACING);
        buttons.append(&toggle_button);
        buttons.append(&second_button);

        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&gtk4::Box::new(gtk4::Orientation::Vertical, 0)));
        widget.add_overlay(&elapsed_box);
        widget.add_overlay(&scroller);
        widget.add_overlay(&buttons);

        let stopwatch = Rc::new(Stopwatch {
            widget: widget.clone(),
            time_label,
            centiseconds_label,
            laps,
            added_lap_count: Cell::new(0),
            toggle_button,
            toggle_label,
            second_button,
            second_label,
            last_is_running: Cell::new(None),
            top: Cell::new(Tween::new(0.0, ANCHOR_MILLIS, EXPRESSIVE_EFFECTS)),
            is_ticking: Cell::new(false),
        });

        widget.connect_get_child_position({
            let stopwatch = Rc::downgrade(&stopwatch);
            let elapsed_box = elapsed_box.clone().upcast::<gtk4::Widget>();
            let scroller = scroller.clone().upcast::<gtk4::Widget>();
            move |widget, child| {
                let stopwatch = stopwatch.upgrade()?;
                let (width, height) = (widget.width(), widget.height());
                let frame_width = width - FRAME_SIDE * 2;
                let frame_height = height - FRAME_TOP;
                let buttons_width = BUTTON_WIDTH * 2 + STOPWATCH_SPACING;
                let buttons_x = FRAME_SIDE + half(frame_width) - half(buttons_width);
                let buttons_y = height - BUTTONS_BOTTOM - BUTTON_HEIGHT;
                let (elapsed_width, elapsed_height) = (
                    elapsed_box.measure(gtk4::Orientation::Horizontal, -1).1,
                    elapsed_box.measure(gtk4::Orientation::Vertical, -1).1,
                );
                let center_y = (FRAME_TOP + half(frame_height) - half(elapsed_height)) as f64;
                let part = stopwatch.top.get().value(stopwatch.now());
                let elapsed_y = (center_y + (FRAME_TOP as f64 - center_y) * part).round() as i32;
                if *child == elapsed_box {
                    return Some(gtk4::gdk::Rectangle::new(
                        buttons_x + ELAPSED_INDENT,
                        elapsed_y,
                        elapsed_width,
                        elapsed_height,
                    ));
                }
                if *child == scroller {
                    let top = elapsed_y + elapsed_height + LIST_MARGIN;
                    return Some(gtk4::gdk::Rectangle::new(
                        FRAME_SIDE,
                        top,
                        frame_width,
                        (buttons_y - LIST_MARGIN - top).max(0),
                    ));
                }
                Some(gtk4::gdk::Rectangle::new(
                    buttons_x,
                    buttons_y,
                    buttons_width,
                    BUTTON_HEIGHT,
                ))
            }
        });

        stopwatch.toggle_button.connect_clicked({
            let timer = timer.clone();
            move |_| timer.toggle_stopwatch()
        });
        stopwatch.second_button.connect_clicked({
            let timer = timer.clone();
            move |_| {
                if timer.is_stopwatch_running() {
                    timer.record_lap();
                } else {
                    timer.reset_stopwatch();
                }
            }
        });
        stopwatch.rebuild(timer);
        stopwatch
    }

    fn now(&self) -> i64 {
        self.widget
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn rebuild(self: &Rc<Self>, timer: &Timer) {
        self.laps.clear();
        self.added_lap_count.set(0);
        self.update(timer, false);
    }

    fn update(self: &Rc<Self>, timer: &Timer, should_animate: bool) {
        let elapsed_centiseconds = timer.stopwatch_time();
        self.time_label.set_text(&format!(
            "{:02}:{:02}",
            elapsed_centiseconds / 6000,
            elapsed_centiseconds / 100 % 60
        ));
        self.centiseconds_label
            .set_text(&format!(":{:02}", elapsed_centiseconds % 100));

        let is_running = timer.is_stopwatch_running();
        self.toggle_label.set_text(&tr(if is_running {
            "Pause"
        } else if elapsed_centiseconds == 0 {
            "Start"
        } else {
            "Resume"
        }));
        self.second_label
            .set_text(&tr(if is_running { "Lap" } else { "Reset" }));
        if self.last_is_running.replace(Some(is_running)) != Some(is_running) {
            self.restyle(is_running);
        }

        let laps = timer.laps();
        self.second_button
            .set_sensitive(elapsed_centiseconds > 0 || !laps.is_empty());
        if laps.len() < self.added_lap_count.get() {
            self.laps.clear();
            self.added_lap_count.set(0);
        }
        for index in self.added_lap_count.get()..laps.len() {
            let previous = if index > 0 { laps[index - 1] } else { 0 };
            self.laps
                .prepend(&lap(index + 1, laps[index], laps[index] - previous));
        }
        self.added_lap_count.set(laps.len());

        let target = if laps.is_empty() { 0.0 } else { 1.0 };
        let mut top = self.top.get();
        if (top.target() - target).abs() < f64::EPSILON {
            return;
        }
        if should_animate && self.widget.is_mapped() {
            top.retarget(target, self.now());
        } else {
            top.jump(target);
        }
        self.top.set(top);
        self.widget.queue_allocate();
        if !should_animate || self.is_ticking.replace(true) {
            return;
        }
        let stopwatch = Rc::downgrade(self);
        self.widget.add_tick_callback(move |widget, clock| {
            let Some(stopwatch) = stopwatch.upgrade() else {
                return glib::ControlFlow::Break;
            };
            widget.queue_allocate();
            if stopwatch.top.get().is_running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            stopwatch.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn restyle(&self, is_running: bool) {
        text::set_color(
            &self.toggle_label,
            if is_running {
                "colOnSecondaryContainer"
            } else {
                "colOnPrimary"
            },
        );
        text::set_color(
            &self.second_label,
            if is_running {
                "colOnLayer2"
            } else {
                "colOnErrorContainer"
            },
        );
        self.toggle_button.set_look(if is_running {
            Look {
                background: |theme| theme.colors.col_secondary_container,
                hover: |theme| theme.colors.col_secondary_container_hover,
                ripple: |theme| theme.colors.col_secondary_container_active,
                ..Look::default()
            }
        } else {
            Look {
                background: |theme| theme.colors.col_primary,
                hover: |theme| theme.colors.col_primary_hover,
                ripple: |theme| theme.colors.col_primary_active,
                ..Look::default()
            }
        });
        self.second_button.set_look(if is_running {
            Look {
                background: |theme| theme.colors.col_layer2,
                hover: |theme| theme.colors.col_layer2_hover,
                ripple: |theme| theme.colors.col_layer2_active,
                ..Look::default()
            }
        } else {
            Look {
                background: |theme| theme.colors.col_error_container,
                hover: |theme| theme.colors.col_error_container_hover,
                ripple: |theme| theme.colors.col_error_container_active,
                ..Look::default()
            }
        });
    }
}

fn lap(number: usize, total_centiseconds: i64, split_centiseconds: i64) -> gtk4::Widget {
    let number_label = text::styled(&format!("{number}."));
    text::set_color(&number_label, "colSubtext");
    let time_label = text::styled(&format!(
        "{:02}:{:02}.{:02}",
        total_centiseconds / 6000,
        total_centiseconds / 100 % 60,
        total_centiseconds % 100
    ));
    let minutes = split_centiseconds / 6000;
    let difference_label = text::styled_sized(
        &format!(
            "+{}{:02}.{:02}",
            if minutes == 0 {
                String::new()
            } else {
                format!("{minutes:02}:")
            },
            split_centiseconds / 100 % 60,
            split_centiseconds % 100
        ),
        pixel_size::SMALLER,
    );
    text::set_color(&difference_label, "colPrimary");
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, LAP_ROW_SPACING);
    row.set_margin_start(LAP_PADDING_X);
    row.set_margin_end(LAP_PADDING_X);
    row.set_margin_top(LAP_PADDING_Y);
    row.set_margin_bottom(LAP_PADDING_Y);
    row.append(&Centered::new(&number_label));
    row.append(&Centered::new(&time_label));
    row.append(&spacer);
    row.append(&Centered::new(&difference_label));

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("timer-lap");
    card.append(&row);
    card.upcast()
}

fn button(theme: &SharedTheme) -> (RippleButton, gtk4::Label) {
    let label = text::styled("");
    let button = RippleButton::new(theme);
    button.set_size_request(BUTTON_WIDTH, BUTTON_HEIGHT);
    button.set_content(&Centered::integral(&label), 0, 0);
    (button, label)
}

fn draw_circle(
    cr: &gtk4::cairo::Context,
    center_x: f64,
    center_y: f64,
    degree: f64,
    primary: RGBA,
    secondary: RGBA,
) {
    let radius = CIRCLE_SIZE as f64 / 2.0 - LINE_WIDTH;
    let start = -90.0;
    cr.set_line_width(LINE_WIDTH);
    cr.set_line_cap(gtk4::cairo::LineCap::Round);
    let track = 360.0 - degree - 2.0 * GAP_ANGLE;
    if track > 0.0 {
        let from = start - GAP_ANGLE;
        cr.new_sub_path();
        cr.arc_negative(
            center_x,
            center_y,
            radius,
            radians(from),
            radians(from - track),
        );
        set_source(cr, secondary);
        let _ = cr.stroke();
    }
    if degree > 0.0 {
        cr.new_sub_path();
        cr.arc(
            center_x,
            center_y,
            radius,
            radians(start),
            radians(start + degree),
        );
        set_source(cr, primary);
        let _ = cr.stroke();
    }
}

fn radians(degrees: f64) -> f64 {
    degrees * PI / 180.0
}

fn set_source(cr: &gtk4::cairo::Context, color: RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

fn half(size: i32) -> i32 {
    (size + 1) / 2
}
