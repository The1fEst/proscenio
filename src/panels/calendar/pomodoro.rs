use gtk4::gdk::RGBA;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::Cell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::scope::Scope;
use crate::panels::calendar::laps::Laps;
use crate::services::timer::Timer;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Ease, Motion, Tween};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::flickable::Flickable;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::secondarytabs::SecondaryTabs;
use crate::ui::widgets::swipe::Swipe;
use crate::ui::widgets::text;

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
        if self.timer.stopwatch_running() {
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
    time: gtk4::Label,
    phase: gtk4::Label,
    cycle: gtk4::Label,
    toggle: RippleButton,
    toggle_label: gtk4::Label,
    reset: RippleButton,
    running: Cell<Option<bool>>,
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

        let time = text::styled_sized("", TIME_SIZE);
        text::set_color(&time, "m3onSurface");
        let phase = text::styled_sized("", pixel_size::NORMAL);
        text::set_color(&phase, "colSubtext");
        let inside = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        inside.append(&Centred::new(&time));
        inside.append(&Centred::new(&phase));

        let cycle = text::styled("");
        text::set_color(&cycle, "colOnLayer2");
        let badge = Centred::integral(&cycle);
        badge.add_css_class("timer-cycle");
        badge.set_size_request(BADGE_SIZE, BADGE_SIZE);
        badge.set_halign(gtk4::Align::End);
        badge.set_valign(gtk4::Align::End);

        let circle = gtk4::Overlay::new();
        circle.set_child(Some(&canvas));
        circle.add_overlay(&Centred::integral(&inside));
        circle.add_overlay(&badge);

        let (toggle, toggle_label) = button(theme);
        let (reset, reset_label) = button(theme);
        reset_label.set_text(&tr("Reset"));
        text::set_color(&reset_label, "colOnErrorContainer");
        reset.set_look(Look {
            background: |theme| theme.colors.col_error_container,
            hover: |theme| theme.colors.col_error_container_hover,
            ripple: |theme| theme.colors.col_error_container_active,
            ..Look::default()
        });
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, POMODORO_SPACING);
        buttons.append(&toggle);
        buttons.append(&reset);

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
                let first =
                    (CIRCLE_SIZE as f64 + extra * CIRCLE_SIZE as f64 / total as f64).round();
                let centre = |outer: f64, inner: i32| ((outer - inner as f64) / 2.0).round() as i32;
                if *child == circle {
                    return Some(gtk4::gdk::Rectangle::new(
                        centre(width as f64, CIRCLE_SIZE),
                        centre(first, CIRCLE_SIZE),
                        CIRCLE_SIZE,
                        CIRCLE_SIZE,
                    ));
                }
                let row = BUTTON_WIDTH * 2 + POMODORO_SPACING;
                Some(gtk4::gdk::Rectangle::new(
                    centre(width as f64, row),
                    first as i32 + centre(height as f64 - first, BUTTON_HEIGHT),
                    row,
                    BUTTON_HEIGHT,
                ))
            }
        });

        let pomodoro = Rc::new(Pomodoro {
            widget,
            canvas,
            progress,
            time,
            phase,
            cycle,
            toggle,
            toggle_label,
            reset,
            running: Cell::new(None),
        });
        pomodoro.toggle.connect_clicked({
            let timer = timer.clone();
            move |_| timer.toggle_pomodoro()
        });
        pomodoro.reset.connect_clicked({
            let timer = timer.clone();
            move |_| timer.reset_pomodoro()
        });
        pomodoro.update(timer, false);
        pomodoro
    }

    fn update(&self, timer: &Timer, animate: bool) {
        let left = timer.seconds_left();
        let duration = timer.lap_duration();
        let degree = left as f64 / duration as f64 * 360.0;
        if animate && self.canvas.is_mapped() {
            self.progress.to(degree);
        } else {
            self.progress.jump(degree);
        }
        self.canvas.queue_draw();

        self.time
            .set_text(&format!("{:02}:{:02}", left / 60, left % 60));
        self.phase.set_text(&tr(if timer.pomodoro_long_break() {
            "Long break"
        } else if timer.pomodoro_break() {
            "Break"
        } else {
            "Focus"
        }));
        self.cycle
            .set_text(&(timer.pomodoro_cycle() + 1).to_string());

        let running = timer.pomodoro_running();
        self.toggle_label.set_text(&tr(if running {
            "Pause"
        } else if left == timer.focus_time() {
            "Start"
        } else {
            "Resume"
        }));
        if self.running.replace(Some(running)) != Some(running) {
            text::set_color(
                &self.toggle_label,
                if running {
                    "colOnSecondaryContainer"
                } else {
                    "colOnPrimary"
                },
            );
            self.toggle.set_look(if running {
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
        self.reset
            .set_sensitive(left < duration || timer.pomodoro_cycle() > 0 || timer.pomodoro_break());
    }
}

struct Stopwatch {
    widget: gtk4::Overlay,
    time: gtk4::Label,
    centis: gtk4::Label,
    laps: Laps,
    shown: Cell<usize>,
    toggle: RippleButton,
    toggle_label: gtk4::Label,
    second: RippleButton,
    second_label: gtk4::Label,
    running: Cell<Option<bool>>,
    top: Cell<Tween>,
    ticking: Cell<bool>,
}

impl Stopwatch {
    fn new(theme: &SharedTheme, timer: &Timer) -> Rc<Self> {
        let time = text::styled_sized("", TIME_SIZE);
        text::set_color(&time, "m3onSurface");
        let centis = text::styled_sized("", TIME_SIZE);
        text::set_color(&centis, "colSubtext");
        let elapsed = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        elapsed.append(&time);
        elapsed.append(&centis);

        let laps = Laps::new(LAP_SPACING);
        let scroller = Flickable::new(theme);
        scroller.set_child(&laps);

        let (toggle, toggle_label) = button(theme);
        let (second, second_label) = button(theme);
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, STOPWATCH_SPACING);
        buttons.append(&toggle);
        buttons.append(&second);

        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&gtk4::Box::new(gtk4::Orientation::Vertical, 0)));
        widget.add_overlay(&elapsed);
        widget.add_overlay(&scroller);
        widget.add_overlay(&buttons);

        let stopwatch = Rc::new(Stopwatch {
            widget: widget.clone(),
            time,
            centis,
            laps,
            shown: Cell::new(0),
            toggle,
            toggle_label,
            second,
            second_label,
            running: Cell::new(None),
            top: Cell::new(Tween::new(0.0, ANCHOR_MILLIS, EXPRESSIVE_EFFECTS)),
            ticking: Cell::new(false),
        });

        widget.connect_get_child_position({
            let stopwatch = Rc::downgrade(&stopwatch);
            let elapsed = elapsed.clone().upcast::<gtk4::Widget>();
            let scroller = scroller.clone().upcast::<gtk4::Widget>();
            move |widget, child| {
                let stopwatch = stopwatch.upgrade()?;
                let (width, height) = (widget.width(), widget.height());
                let frame_width = width - FRAME_SIDE * 2;
                let frame_height = height - FRAME_TOP;
                let row = BUTTON_WIDTH * 2 + STOPWATCH_SPACING;
                let buttons_x = FRAME_SIDE + half(frame_width) - half(row);
                let buttons_y = height - BUTTONS_BOTTOM - BUTTON_HEIGHT;
                let (elapsed_width, elapsed_height) = (
                    elapsed.measure(gtk4::Orientation::Horizontal, -1).1,
                    elapsed.measure(gtk4::Orientation::Vertical, -1).1,
                );
                let centred = (FRAME_TOP + half(frame_height) - half(elapsed_height)) as f64;
                let part = stopwatch.top.get().value(stopwatch.now());
                let elapsed_y = (centred + (FRAME_TOP as f64 - centred) * part).round() as i32;
                if *child == elapsed {
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
                    row,
                    BUTTON_HEIGHT,
                ))
            }
        });

        stopwatch.toggle.connect_clicked({
            let timer = timer.clone();
            move |_| timer.toggle_stopwatch()
        });
        stopwatch.second.connect_clicked({
            let timer = timer.clone();
            move |_| {
                if timer.stopwatch_running() {
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
        self.shown.set(0);
        self.update(timer, false);
    }

    fn update(self: &Rc<Self>, timer: &Timer, animate: bool) {
        let elapsed = timer.stopwatch_time();
        self.time
            .set_text(&format!("{:02}:{:02}", elapsed / 6000, elapsed / 100 % 60));
        self.centis.set_text(&format!(":{:02}", elapsed % 100));

        let running = timer.stopwatch_running();
        self.toggle_label.set_text(&tr(if running {
            "Pause"
        } else if elapsed == 0 {
            "Start"
        } else {
            "Resume"
        }));
        self.second_label
            .set_text(&tr(if running { "Lap" } else { "Reset" }));
        if self.running.replace(Some(running)) != Some(running) {
            self.restyle(running);
        }

        let laps = timer.laps();
        self.second.set_sensitive(elapsed > 0 || !laps.is_empty());
        if laps.len() < self.shown.get() {
            self.laps.clear();
            self.shown.set(0);
        }
        for index in self.shown.get()..laps.len() {
            let previous = if index > 0 { laps[index - 1] } else { 0 };
            self.laps
                .prepend(&lap(index + 1, laps[index], laps[index] - previous));
        }
        self.shown.set(laps.len());

        let target = if laps.is_empty() { 0.0 } else { 1.0 };
        let mut top = self.top.get();
        if (top.target() - target).abs() < f64::EPSILON {
            return;
        }
        if animate && self.widget.is_mapped() {
            top.retarget(target, self.now());
        } else {
            top.jump(target);
        }
        self.top.set(top);
        self.widget.queue_allocate();
        if !animate || self.ticking.replace(true) {
            return;
        }
        let stopwatch = Rc::downgrade(self);
        self.widget.add_tick_callback(move |widget, clock| {
            let Some(stopwatch) = stopwatch.upgrade() else {
                return glib::ControlFlow::Break;
            };
            widget.queue_allocate();
            if stopwatch.top.get().running(clock.frame_time()) {
                return glib::ControlFlow::Continue;
            }
            stopwatch.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn restyle(&self, running: bool) {
        text::set_color(
            &self.toggle_label,
            if running {
                "colOnSecondaryContainer"
            } else {
                "colOnPrimary"
            },
        );
        text::set_color(
            &self.second_label,
            if running {
                "colOnLayer2"
            } else {
                "colOnErrorContainer"
            },
        );
        self.toggle.set_look(if running {
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
        self.second.set_look(if running {
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

fn lap(number: usize, total: i64, split: i64) -> gtk4::Widget {
    let index = text::styled(&format!("{number}."));
    text::set_color(&index, "colSubtext");
    let time = text::styled(&format!(
        "{:02}:{:02}.{:02}",
        total / 6000,
        total / 100 % 60,
        total % 100
    ));
    let minutes = split / 6000;
    let difference = text::styled_sized(
        &format!(
            "+{}{:02}.{:02}",
            if minutes == 0 {
                String::new()
            } else {
                format!("{minutes:02}:")
            },
            split / 100 % 60,
            split % 100
        ),
        pixel_size::SMALLER,
    );
    text::set_color(&difference, "colPrimary");
    let spacer = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, LAP_ROW_SPACING);
    row.set_margin_start(LAP_PADDING_X);
    row.set_margin_end(LAP_PADDING_X);
    row.set_margin_top(LAP_PADDING_Y);
    row.set_margin_bottom(LAP_PADDING_Y);
    row.append(&Centred::new(&index));
    row.append(&Centred::new(&time));
    row.append(&spacer);
    row.append(&Centred::new(&difference));

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("timer-lap");
    card.append(&row);
    card.upcast()
}

fn button(theme: &SharedTheme) -> (RippleButton, gtk4::Label) {
    let label = text::styled("");
    let button = RippleButton::new(theme);
    button.set_size_request(BUTTON_WIDTH, BUTTON_HEIGHT);
    button.set_content(&Centred::integral(&label), 0, 0);
    (button, label)
}

fn draw_circle(
    cr: &gtk4::cairo::Context,
    centre_x: f64,
    centre_y: f64,
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
            centre_x,
            centre_y,
            radius,
            radians(from),
            radians(from - track),
        );
        source(cr, secondary);
        let _ = cr.stroke();
    }
    if degree > 0.0 {
        cr.new_sub_path();
        cr.arc(
            centre_x,
            centre_y,
            radius,
            radians(start),
            radians(start + degree),
        );
        source(cr, primary);
        let _ = cr.stroke();
    }
}

fn radians(degrees: f64) -> f64 {
    degrees * PI / 180.0
}

fn source(cr: &gtk4::cairo::Context, colour: RGBA) {
    cr.set_source_rgba(
        colour.red() as f64,
        colour.green() as f64,
        colour.blue() as f64,
        colour.alpha() as f64,
    );
}

fn half(size: i32) -> i32 {
    (size + 1) / 2
}
