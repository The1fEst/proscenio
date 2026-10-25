use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use super::{Lock, Target};
use crate::core::config;
use crate::core::i18n::tr;
use crate::core::listeners::Subscription;
use crate::core::scope::Scope;
use crate::panels::bar::tray;
use crate::platform::security::pam;
use crate::ui::anim::{EMPHASIZED_DECEL, EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Tween};
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::{SharedTheme, Theme, pixel_size};
use crate::ui::widgets::display::text::{self, Shift};
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::toolbar;

const ISLAND_GAP: i32 = 10;
const BOTTOM: i32 = 20;
const BUTTON: i32 = 40;
const CONFIRM_ICON: f64 = 24.0;
const BUTTON_ICON: f64 = 22.0;
const FIELD_WIDTH: i32 = 200;
const FIELD_PADDING: i32 = 10;
const PLACEHOLDER_START: i32 = FIELD_PADDING + 4;
const CHAR_SIZE: f64 = 20.0;
const CHAR_SHAPE: f64 = 18.0;
const CHAR_START: f64 = -1.0;
const CURSOR_WIDTH: f64 = 2.0;
const PAIR_SPACING: i32 = 4;
const PAIR_SIDE: i32 = 10;
const NAME_START: i32 = 8;
const LAYOUT_END: i32 = 8;
const LAYOUT_SPACING: i32 = 8;
const FINGERPRINT_START: i32 = 10;
const FINGERPRINT_END: i32 = 6;
const FCITX: &str = "Fcitx";
const FCITX_END: i32 = 10;
const APPEAR_SCALE: f64 = 0.9;
const APPEAR_SCALE_MILLIS: f64 = 500.0;
const APPEAR_FADE_MILLIS: f64 = 200.0;
const SHAPE_FADE_MILLIS: f64 = 50.0;
const SHAPE_SCALE_MILLIS: f64 = 200.0;
const SHAPE_GROW_MILLIS: f64 = 250.0;
const SHAPE_COLOR_MILLIS: f64 = 1000.0;
const SCROLL_MILLIS: f64 = 400.0;
const SHAKE: [(f64, f64); 5] = [
    (-30.0, 50.0),
    (30.0, 50.0),
    (-15.0, 40.0),
    (15.0, 40.0),
    (0.0, 30.0),
];
const CHAR_SHAPES: [Shape; 7] = [
    Shape::Clover4Leaf,
    Shape::Arrow,
    Shape::Pill,
    Shape::SoftBurst,
    Shape::Diamond,
    Shape::ClamShell,
    Shape::Pentagon,
];

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Islands {
        pub scale: Cell<f64>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Islands {
        const NAME: &'static str = "ProscenioLockIslands";
        type Type = super::Islands;
        type ParentType = gtk4::Widget;
    }

    impl ObjectImpl for Islands {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Islands {
        fn measure(&self, _orientation: gtk4::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (0, 0, -1, -1)
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            let Some(left) = obj.first_child() else {
                return;
            };
            let Some(main) = left.next_sibling() else {
                return;
            };
            let Some(right) = main.next_sibling() else {
                return;
            };
            let natural = |widget: &gtk4::Widget| {
                (
                    widget.measure(gtk4::Orientation::Horizontal, -1).1,
                    widget.measure(gtk4::Orientation::Vertical, -1).1,
                )
            };
            let (main_width, main_height) = natural(&main);
            let x = (width - main_width) / 2;
            let y = height - BOTTOM - main_height;
            let (left_width, _) = natural(&left);
            let (right_width, _) = natural(&right);
            let scale = self.scale.get() as f32;
            let place = |widget: &gtk4::Widget, x: i32, width: i32| {
                let half = gtk4::graphene::Point::new(width as f32 / 2.0, main_height as f32 / 2.0);
                let center = gtk4::graphene::Point::new(x as f32 + half.x(), y as f32 + half.y());
                let transform = gtk4::gsk::Transform::new()
                    .translate(&center)
                    .scale(scale, scale)
                    .translate(&gtk4::graphene::Point::new(-half.x(), -half.y()));
                widget.allocate(width, main_height, -1, Some(transform));
            };
            place(&left, x - ISLAND_GAP - left_width, left_width);
            place(&main, x, main_width);
            place(&right, x + main_width + ISLAND_GAP, right_width);
        }
    }
}

glib::wrapper! {
    pub struct Islands(ObjectSubclass<imp::Islands>)
        @extends gtk4::Widget,
        @implements gtk4::Accessible, gtk4::Buildable, gtk4::ConstraintTarget;
}

impl Islands {
    fn new(left: &gtk4::Widget, main: &gtk4::Widget, right: &gtk4::Widget) -> Self {
        let islands: Islands = glib::Object::new();
        islands.imp().scale.set(APPEAR_SCALE);
        for child in [left, main, right] {
            child.set_parent(&islands);
        }
        islands
    }

    fn set_scale(&self, scale: f64) {
        self.imp().scale.set(scale);
        self.queue_allocate();
    }
}

fn is_control(key: gdk::Key) -> bool {
    matches!(key, gdk::Key::Control_L | gdk::Key::Control_R)
}

fn frame_time(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}

fn display_name() -> String {
    let user = glib::user_name().to_string_lossy().into_owned();
    std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|passwd| {
            passwd.lines().find_map(|line| {
                let mut fields = line.split(':');
                (fields.next()? == user).then_some(())?;
                let gecos = fields.nth(3)?;
                let name = gecos.split(',').next()?.trim().to_owned();
                (!name.is_empty()).then_some(name)
            })
        })
        .unwrap_or(user)
}

fn look(
    toggled: fn(&Theme) -> gdk::RGBA,
    hover: fn(&Theme) -> gdk::RGBA,
    ripple: fn(&Theme) -> gdk::RGBA,
) -> Look {
    Look {
        toggled,
        toggled_hover: hover,
        ripple_toggled: ripple,
        ..Look::default()
    }
}

fn round_button(theme: &SharedTheme, icon: &str, size: f64) -> (RippleButton, gtk4::Label) {
    let button = RippleButton::new(theme);
    button.set_radius(BUTTON as f64 / 2.0);
    button.set_size_request(BUTTON, BUTTON);
    button.set_valign(gtk4::Align::Center);
    let symbol = text::symbol(icon, size);
    button.set_content(&Centered::integral(&symbol), 0, 0);
    (button, symbol)
}

fn pair(icon: &str, start: i32) -> (gtk4::Box, gtk4::Label, gtk4::Label) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, PAIR_SPACING);
    row.set_margin_start(start);
    row.set_margin_end(PAIR_SIDE);
    let symbol = text::symbol_filled(icon, pixel_size::HUGE as f64, 1.0);
    text::set_color(&symbol, "colOnSurfaceVariant");
    row.append(&Centered::integral(&symbol));
    let label = text::styled("");
    text::set_color(&label, "colOnSurfaceVariant");
    row.append(&Centered::new(&label));
    (row, symbol, label)
}

pub(super) struct Surface {
    pub(super) window: gtk4::Window,
    pub(super) monitor: gdk::Monitor,
    pub(super) is_settled: Cell<bool>,
    pub(super) field: gtk4::Text,
    dots: gtk4::DrawingArea,
    shift: Shift,
    placeholder_label: gtk4::Label,
    confirm_button: RippleButton,
    confirm_icon: gtk4::Label,
    power_button: RippleButton,
    reboot_button: RippleButton,
    pub(super) islands: Islands,
    appear_tweens: Cell<(Tween, Tween)>,
    birth_times: RefCell<Vec<i64>>,
    scroll: Cell<Tween>,
    caret: Cell<Tween>,
    shake_start: Cell<i64>,
    theme: SharedTheme,
    subscriptions: RefCell<Vec<Subscription>>,
    pub(super) scope: Scope,
    is_syncing: Cell<bool>,
}

impl Surface {
    fn draw_dots(&self, cr: &gtk4::cairo::Context, height: f64) {
        let now = frame_time(&self.dots);
        let theme = self.theme.borrow();
        let pill = FIELD_WIDTH as f64;
        let radius = height / 2.0;
        let quarter = std::f64::consts::FRAC_PI_2;
        cr.new_path();
        cr.arc(radius, radius, radius, quarter, 3.0 * quarter);
        cr.arc(pill - radius, radius, radius, -quarter, quarter);
        cr.close_path();
        cr.clip_preserve();
        cr.set_source_color(&theme.colors.col_layer1);
        let _ = cr.fill();
        if !config::value_bool("/lock/materialShapeChars", true) {
            return;
        }
        let scroll = self.scroll.get().value(now);
        let (start, end) = self
            .field
            .selection_bounds()
            .unwrap_or((self.field.position(), self.field.position()));
        let (start, end) = (start.min(end), start.max(end));
        let center = height / 2.0;
        let origin = FIELD_PADDING as f64 - scroll;
        let selection = theme.colors.col_secondary_container;
        let primary = theme.colors.col_primary;
        let idle_color = theme.colors.col_on_layer1;
        let selected_color = theme.colors.col_on_secondary_container;
        for (index, birth_time) in self.birth_times.borrow().iter().enumerate() {
            let left = origin + CHAR_START + index as f64 * CHAR_SIZE;
            let is_selected = (index as i32) >= start && (index as i32) < end;
            if is_selected {
                cr.rectangle(left, center - CHAR_SIZE / 2.0, CHAR_SIZE, CHAR_SIZE);
                cr.set_source_color(&selection);
                let _ = cr.fill();
            }
            let age_millis = (now - birth_time) as f64 / 1000.0;
            let part = |duration_millis: f64| (age_millis / duration_millis).clamp(0.0, 1.0);
            let opacity = EXPRESSIVE_EFFECTS.at(part(SHAPE_FADE_MILLIS));
            let scale = 0.5 + 0.5 * EXPRESSIVE_FAST.at(part(SHAPE_SCALE_MILLIS));
            let size = CHAR_SHAPE * EXPRESSIVE_FAST.at(part(SHAPE_GROW_MILLIS)) * scale;
            let fade = EXPRESSIVE_EFFECTS.at(part(SHAPE_COLOR_MILLIS));
            let mut color = if is_selected {
                selected_color
            } else {
                mix(primary, idle_color, fade)
            };
            color.set_alpha(color.alpha() * opacity as f32);
            let corner = left + (CHAR_SIZE - size) / 2.0;
            let shape = shapes::polygon(CHAR_SHAPES[index % CHAR_SHAPES.len()]);
            shape.trace(cr, corner, center - size / 2.0, size);
            cr.set_source_color(&color);
            let _ = cr.fill();
        }
        let caret = origin + self.caret.get().value(now);
        cr.rectangle(caret, center - CHAR_SIZE / 2.0, CURSOR_WIDTH, CHAR_SIZE);
        cr.set_source_color(&primary);
        let _ = cr.fill();
    }

    pub(super) fn is_animating(&self, now: i64) -> bool {
        let (scale, fade) = self.appear_tweens.get();
        let are_chars_animating = self
            .birth_times
            .borrow()
            .iter()
            .any(|birth_time| (now - birth_time) as f64 / 1000.0 < SHAPE_COLOR_MILLIS);
        let shake_millis: f64 = SHAKE.iter().map(|(_, millis)| millis).sum();
        let is_shaking = self.shake_start.get() >= 0
            && ((now - self.shake_start.get()) as f64 / 1000.0) < shake_millis;
        let is_moving = [scale, fade, self.scroll.get(), self.caret.get()]
            .iter()
            .any(|tween| tween.is_running(now));
        is_moving || are_chars_animating || is_shaking
    }

    fn step(&self, now: i64) {
        let (scale, fade) = self.appear_tweens.get();
        self.islands.set_scale(scale.value(now));
        self.islands.set_opacity(fade.value(now));
        let shake_start = self.shake_start.get();
        if shake_start >= 0 {
            let mut elapsed_millis = (now - shake_start) as f64 / 1000.0;
            let mut from = 0.0;
            let mut offset = 0.0;
            for (to, millis) in SHAKE {
                if elapsed_millis <= millis {
                    offset = from + (to - from) * (elapsed_millis / millis);
                    break;
                }
                elapsed_millis -= millis;
                from = to;
                offset = to;
            }
            self.shift.set_offset(offset as f32);
        }
        self.dots.queue_draw();
    }
}

fn mix(from: gdk::RGBA, to: gdk::RGBA, part: f64) -> gdk::RGBA {
    let blend = |a: f32, b: f32| a + (b - a) * part as f32;
    gdk::RGBA::new(
        blend(from.red(), to.red()),
        blend(from.green(), to.green()),
        blend(from.blue(), to.blue()),
        blend(from.alpha(), to.alpha()),
    )
}

impl Lock {
    pub(super) fn surface(self: &Rc<Self>, monitor: &gdk::Monitor) -> Rc<Surface> {
        let theme = &self.theme;
        let context = &self.context;

        let field = gtk4::Text::with_buffer(&gtk4::PasswordEntryBuffer::new());
        field.add_css_class("lock-field");
        field.set_visibility(false);
        field.set_margin_start(FIELD_PADDING);
        field.set_margin_end(FIELD_PADDING);
        field.set_hexpand(true);
        let dots = gtk4::DrawingArea::new();
        dots.set_size_request(FIELD_WIDTH, BUTTON);
        let placeholder_label = text::styled(&tr("Enter password"));
        text::set_color(&placeholder_label, "colSubtext");
        placeholder_label.set_xalign(0.0);
        let placeholder_box = Centered::filling_width(&placeholder_label);
        placeholder_box.set_margin_start(PLACEHOLDER_START);
        placeholder_box.set_can_target(false);
        let field_overlay = gtk4::Overlay::new();
        field_overlay.set_child(Some(&dots));
        field_overlay.add_overlay(&field);
        field_overlay.add_overlay(&placeholder_box);
        let shift = Shift::new(&field_overlay);
        shift.set_sideways(true);

        let (confirm_button, confirm_icon) = round_button(theme, "arrow_right_alt", CONFIRM_ICON);
        confirm_button.set_look(look(
            |theme| theme.colors.col_primary,
            |theme| theme.colors.col_primary_hover,
            |theme| theme.colors.col_primary_active,
        ));
        confirm_button.set_toggled(true);
        let fingerprint_icon = text::symbol_filled("fingerprint", pixel_size::HUGEASS as f64, 1.0);
        text::set_color(&fingerprint_icon, "colOnSurfaceVariant");
        let fingerprint_box = Centered::integral(&fingerprint_icon);
        fingerprint_box.set_margin_start(FINGERPRINT_START);
        fingerprint_box.set_margin_end(FINGERPRINT_END);
        let main_frame = toolbar::frame();
        main_frame.append(&fingerprint_box);
        main_frame.append(&shift);
        main_frame.append(&confirm_button);

        let (name_pair, _, name_label) = pair("account_circle", NAME_START);
        name_label.set_text(&display_name());
        let layout_row = gtk4::Box::new(gtk4::Orientation::Horizontal, LAYOUT_SPACING);
        layout_row.set_margin_end(LAYOUT_END);
        let keyboard_icon = text::symbol_filled("keyboard_alt", pixel_size::HUGE as f64, 1.0);
        text::set_color(&keyboard_icon, "colOnSurfaceVariant");
        layout_row.append(&Centered::integral(&keyboard_icon));
        let layout_code_label = text::styled("");
        text::set_color(&layout_code_label, "colOnSurfaceVariant");
        layout_row.append(&Centered::new(&layout_code_label));
        let left_frame = toolbar::frame();
        left_frame.append(&name_pair);
        left_frame.append(&layout_row);
        let scope = Scope::default();
        if let Some(bus) = &self.services.session_bus {
            let fcitx = tray::build(
                bus,
                &config::current(),
                theme,
                &scope,
                Some(FCITX),
                false,
                false,
            );
            fcitx.set_margin_end(FCITX_END);
            left_frame.append(&fcitx);
        }

        let (battery_pair, battery_icon, battery_level_label) =
            pair("battery_android_full", PAIR_SIDE);
        let (sleep_button, sleep_icon) = round_button(theme, "dark_mode", BUTTON_ICON);
        let secondary_look = || {
            look(
                |theme| theme.colors.col_secondary_container,
                |theme| theme.colors.col_secondary_container_hover,
                |theme| theme.colors.col_secondary_container_active,
            )
        };
        sleep_button.set_look(secondary_look());
        text::set_color(&sleep_icon, "colOnSurfaceVariant");
        let (power_button, power_icon) = round_button(theme, "power_settings_new", BUTTON_ICON);
        power_button.set_look(secondary_look());
        let (reboot_button, reboot_icon) = round_button(theme, "restart_alt", BUTTON_ICON);
        reboot_button.set_look(secondary_look());
        let right_frame = toolbar::frame();
        right_frame.append(&battery_pair);
        right_frame.append(&sleep_button);
        right_frame.append(&power_button);
        right_frame.append(&reboot_button);

        let islands = Islands::new(
            left_frame.upcast_ref(),
            main_frame.upcast_ref(),
            right_frame.upcast_ref(),
        );
        islands.set_opacity(0.0);
        let window = gtk4::Window::new();
        window.add_css_class("lock-surface");
        window.set_child(Some(&islands));

        let now = glib::monotonic_time();
        let mut scale = Tween::new(APPEAR_SCALE, APPEAR_SCALE_MILLIS, EXPRESSIVE_FAST);
        scale.retarget(1.0, now);
        let mut fade = Tween::new(0.0, APPEAR_FADE_MILLIS, EXPRESSIVE_EFFECTS);
        fade.retarget(1.0, now);
        let surface = Rc::new(Surface {
            window: window.clone(),
            monitor: monitor.clone(),
            is_settled: Cell::new(false),
            field: field.clone(),
            dots: dots.clone(),
            shift,
            placeholder_label,
            confirm_button: confirm_button.clone(),
            confirm_icon,
            power_button: power_button.clone(),
            reboot_button: reboot_button.clone(),
            islands: islands.clone(),
            appear_tweens: Cell::new((scale, fade)),
            birth_times: RefCell::new(Vec::new()),
            scroll: Cell::new(Tween::new(0.0, SCROLL_MILLIS, EMPHASIZED_DECEL)),
            caret: Cell::new(Tween::new(0.0, SCROLL_MILLIS, EMPHASIZED_DECEL)),
            shake_start: Cell::new(-1),
            theme: theme.clone(),
            subscriptions: RefCell::new(Vec::new()),
            scope,
            is_syncing: Cell::new(false),
        });

        dots.set_draw_func({
            let surface = Rc::downgrade(&surface);
            move |_, cr, _, height| {
                if let Some(surface) = surface.upgrade() {
                    surface.draw_dots(cr, height as f64);
                }
            }
        });
        islands.add_tick_callback({
            let surface = Rc::downgrade(&surface);
            move |_, clock| {
                let Some(surface) = surface.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let now = clock.frame_time();
                surface.step(now);
                if surface.is_animating(now) {
                    glib::ControlFlow::Continue
                } else {
                    glib::ControlFlow::Break
                }
            }
        });

        let refresh = {
            let surface = Rc::downgrade(&surface);
            let context = Rc::downgrade(context);
            let services = self.services.clone();
            move || {
                let (Some(surface), Some(context)) = (surface.upgrade(), context.upgrade()) else {
                    return;
                };
                let password_text = context.text.borrow().clone();
                let field_text = pam::copy_text(&surface.field.buffer());
                if field_text != password_text {
                    surface.is_syncing.set(true);
                    surface.field.set_text(&password_text);
                    surface.field.set_position(-1);
                    surface.is_syncing.set(false);
                }
                pam::wipe(&mut field_text.into_bytes());
                let length = password_text.chars().count();
                pam::wipe(&mut password_text.into_bytes());
                let now = frame_time(&surface.dots);
                {
                    let mut birth_times = surface.birth_times.borrow_mut();
                    birth_times.truncate(length);
                    while birth_times.len() < length {
                        birth_times.push(now);
                    }
                }
                let width = (FIELD_WIDTH - 2 * FIELD_PADDING) as f64;
                let mut scroll = surface.scroll.get();
                scroll.retarget((length as f64 * CHAR_SIZE - width).max(0.0), now);
                surface.scroll.set(scroll);
                let mut caret = surface.caret.get();
                caret.retarget(CHAR_SIZE * surface.field.position() as f64, now);
                surface.caret.set(caret);
                fingerprint_box.set_visible(context.is_fingerprint_active.get());
                if config::value_bool("/lock/materialShapeChars", true) {
                    surface.field.remove_css_class("plain");
                } else {
                    surface.field.add_css_class("plain");
                }
                surface.placeholder_label.set_visible(length == 0);
                surface
                    .placeholder_label
                    .set_text(&tr(if context.has_failed.get() {
                        "Incorrect password"
                    } else {
                        "Enter password"
                    }));
                let is_busy = context.is_in_progress.get();
                surface.field.set_sensitive(!is_busy);
                surface.confirm_button.set_sensitive(!is_busy);
                text::set_color(
                    &surface.confirm_icon,
                    if is_busy {
                        "colSubtext"
                    } else {
                        "colOnPrimary"
                    },
                );
                surface.confirm_icon.set_text(match context.target() {
                    Target::Unlock if context.is_ctrl_held.get() => "coffee",
                    Target::Unlock => "arrow_right_alt",
                    Target::Poweroff => "power_settings_new",
                    Target::Reboot => "restart_alt",
                });
                let target = context.target.get();
                surface
                    .power_button
                    .set_toggled(target == Some(Target::Poweroff));
                surface
                    .reboot_button
                    .set_toggled(target == Some(Target::Reboot));
                let charge = services.battery.charge.get();
                battery_pair.set_visible(charge.is_available);
                let is_charging = charge.is_charging();
                battery_icon.set_text(if is_charging {
                    "bolt"
                } else {
                    "battery_android_full"
                });
                battery_level_label.set_text(&format!("{}", (charge.percentage * 100.0).round()));
                let is_low =
                    charge.percentage * 100.0 <= config::value_i64("/battery/low", 20) as f64;
                let tone = if is_low && !is_charging {
                    "colError"
                } else {
                    "colOnSurfaceVariant"
                };
                text::set_color(&battery_icon, tone);
                text::set_color(&battery_level_label, tone);
                layout_code_label.set_text(&services.xkb.current_code.borrow().to_uppercase());
                let power_buttons = [
                    (&power_icon, &surface.power_button),
                    (&reboot_icon, &surface.reboot_button),
                ];
                for (icon, button) in power_buttons {
                    text::set_color(
                        icon,
                        if button.is_toggled() {
                            "colOnSecondaryContainer"
                        } else {
                            "colOnSurfaceVariant"
                        },
                    );
                }
                surface.islands.queue_allocate();
                surface.dots.queue_draw();
                start_ticking(&surface);
            }
        };
        let refresh = Rc::new(refresh);
        refresh();
        surface
            .subscriptions
            .borrow_mut()
            .push(context.changed.add({
                let refresh = refresh.clone();
                move || refresh()
            }));
        surface
            .subscriptions
            .borrow_mut()
            .push(context.shake_requested.add({
                let surface = Rc::downgrade(&surface);
                move || {
                    if let Some(surface) = surface.upgrade() {
                        surface.shake_start.set(frame_time(&surface.dots));
                        start_ticking(&surface);
                    }
                }
            }));
        surface
            .subscriptions
            .borrow_mut()
            .push(context.focus_requested.add({
                let field = field.clone();
                move || {
                    field.grab_focus();
                }
            }));
        surface
            .subscriptions
            .borrow_mut()
            .push(self.services.battery.subscribe({
                let refresh = Rc::downgrade(&refresh);
                move || {
                    if let Some(refresh) = refresh.upgrade() {
                        refresh();
                    }
                }
            }));
        surface
            .subscriptions
            .borrow_mut()
            .push(self.services.xkb.subscribe({
                let refresh = Rc::downgrade(&refresh);
                move || {
                    if let Some(refresh) = refresh.upgrade() {
                        refresh();
                    }
                }
            }));

        field.connect_changed({
            let context = Rc::downgrade(context);
            let surface = Rc::downgrade(&surface);
            move |field| {
                if surface
                    .upgrade()
                    .is_some_and(|surface| surface.is_syncing.get())
                {
                    return;
                }
                if let Some(context) = context.upgrade() {
                    let password_text = pam::copy_text(&field.buffer());
                    context.set_text(&password_text);
                    pam::wipe(&mut password_text.into_bytes());
                }
            }
        });
        for property in ["cursor-position", "selection-bound"] {
            field.connect_notify_local(Some(property), {
                let surface = Rc::downgrade(&surface);
                move |field, _| {
                    let Some(surface) = surface.upgrade() else {
                        return;
                    };
                    let now = frame_time(&surface.dots);
                    let mut caret = surface.caret.get();
                    caret.retarget(CHAR_SIZE * field.position() as f64, now);
                    surface.caret.set(caret);
                    surface.dots.queue_draw();
                    start_ticking(&surface);
                }
            });
        }
        field.connect_activate({
            let lock = Rc::downgrade(self);
            move |_| {
                if let Some(lock) = lock.upgrade() {
                    let should_also_inhibit = lock.context.is_ctrl_held.get();
                    lock.try_unlock(should_also_inhibit);
                }
            }
        });
        confirm_button.connect_clicked({
            let lock = Rc::downgrade(self);
            move |_| {
                if let Some(lock) = lock.upgrade() {
                    lock.try_unlock(false);
                }
            }
        });
        sleep_button.connect_clicked({
            let session = self.services.session.clone();
            move |_| session.suspend()
        });
        for (button, target) in [
            (&power_button, Target::Poweroff),
            (&reboot_button, Target::Reboot),
        ] {
            button.connect_clicked({
                let lock = Rc::downgrade(self);
                move |_| {
                    let Some(lock) = lock.upgrade() else {
                        return;
                    };
                    if !config::value_bool("/lock/security/requirePasswordToPower", false) {
                        lock.run_target(target, None);
                        return;
                    }
                    let context = &lock.context;
                    if context.target.get() == Some(target) {
                        context.target.set(None);
                    } else {
                        context.target.set(Some(target));
                        context.focus_requested.notify();
                    }
                    context.changed.notify();
                }
            });
        }

        let key_controller = gtk4::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk4::PropagationPhase::Capture);
        key_controller.connect_key_pressed({
            let context = Rc::downgrade(context);
            let field = field.clone();
            move |_, key, _, _| {
                let Some(context) = context.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                context.restart_clear();
                if is_control(key) && !context.is_ctrl_held.replace(true) {
                    context.changed.notify();
                }
                if key == gdk::Key::Escape {
                    context.set_text("");
                }
                if !field.has_focus() {
                    field.grab_focus();
                }
                glib::Propagation::Proceed
            }
        });
        key_controller.connect_key_released({
            let context = Rc::downgrade(context);
            let field = field.clone();
            move |_, key, _, _| {
                let Some(context) = context.upgrade() else {
                    return;
                };
                if is_control(key) && context.is_ctrl_held.replace(false) {
                    context.changed.notify();
                }
                if !field.has_focus() {
                    field.grab_focus();
                }
            }
        });
        window.add_controller(key_controller);
        let motion_controller = gtk4::EventControllerMotion::new();
        motion_controller.connect_motion({
            let field = field.clone();
            move |_, _, _| {
                if !field.has_focus() {
                    field.grab_focus();
                }
            }
        });
        window.add_controller(motion_controller);
        let press_gesture = gtk4::GestureClick::new();
        press_gesture.connect_pressed({
            let field = field.clone();
            move |_, _, _, _| {
                if !field.has_focus() {
                    field.grab_focus();
                }
            }
        });
        window.add_controller(press_gesture);
        surface
    }
}

fn start_ticking(surface: &Rc<Surface>) {
    let now = frame_time(&surface.dots);
    if !surface.is_animating(now) {
        return;
    }
    let weak: Weak<Surface> = Rc::downgrade(surface);
    surface.islands.add_tick_callback(move |_, clock| {
        let Some(surface) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let now = clock.frame_time();
        surface.step(now);
        if surface.is_animating(now) {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}
