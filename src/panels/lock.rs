use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::scope::Scope;
use crate::core::{config, persistent};
use crate::panels::bar::tray;
use crate::platform::fprint::Reader;
use crate::platform::locknotify::LockNotifier;
use crate::platform::sessionlock::SessionLock;
use crate::platform::{hypr, pam};
use crate::services::Services;
use crate::ui::anim::{EMPHASIZED_DECEL, EXPRESSIVE_EFFECTS, EXPRESSIVE_FAST, Tween};
use crate::ui::shapes::{self, Shape};
use crate::ui::theme::{SharedTheme, Theme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text::{self, Shift};
use crate::ui::widgets::toolbar;

const PAM_SERVICE: &str = "login";
const SECRETS: &str = "org.freedesktop.secrets";
const SECRETS_PATH: &str = "/org/freedesktop/secrets";
const SECRET_SERVICE: &str = "org.freedesktop.Secret.Service";
const KEYRING_INTERNAL: &str = "org.gnome.keyring.InternalUnsupportedGuiltRiddenInterface";
const LOGIN_KEYRING: &str = "/org/freedesktop/secrets/collection/login";
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
const SHAPE_COLOUR_MILLIS: f64 = 1000.0;
const SCROLL_MILLIS: f64 = 400.0;
const SHAKE: [(f64, f64); 5] = [
    (-30.0, 50.0),
    (30.0, 50.0),
    (-15.0, 40.0),
    (15.0, 40.0),
    (0.0, 30.0),
];
const CLEAR_AFTER: Duration = Duration::from_secs(10);
const RESTORE_DELAY: Duration = Duration::from_millis(150);
const GRANT_TIMEOUT: Duration = Duration::from_secs(6);
const HIDDEN_WORKSPACE: i64 = 2_147_483_647;
const CHAR_SHAPES: [Shape; 7] = [
    Shape::Clover4Leaf,
    Shape::Arrow,
    Shape::Pill,
    Shape::SoftBurst,
    Shape::Diamond,
    Shape::ClamShell,
    Shape::Pentagon,
];

#[derive(Clone, Copy, PartialEq)]
enum Target {
    Unlock,
    Poweroff,
    Reboot,
}

#[derive(Default)]
struct Context {
    text: RefCell<String>,
    in_progress: Cell<bool>,
    failed: Cell<bool>,
    fingerprint: Cell<bool>,
    target: Cell<Option<Target>>,
    also_inhibit: Cell<bool>,
    ctrl: Cell<bool>,
    clear: Cell<Option<glib::SourceId>>,
    changed: Listeners,
    shake: Listeners,
    focus: Listeners,
}

impl Context {
    fn target(&self) -> Target {
        self.target.get().unwrap_or(Target::Unlock)
    }

    fn set_text(self: &Rc<Self>, text: &str) {
        if *self.text.borrow() == text {
            return;
        }
        self.text.replace(text.to_owned());
        if !text.is_empty() {
            self.failed.set(false);
        }
        self.restart_clear();
        self.changed.notify();
    }

    fn restart_clear(self: &Rc<Self>) {
        if let Some(source) = self.clear.take() {
            source.remove();
        }
        let context = Rc::downgrade(self);
        self.clear
            .set(Some(glib::timeout_add_local_once(CLEAR_AFTER, move || {
                if let Some(context) = context.upgrade() {
                    context.clear.set(None);
                    context.reset();
                }
            })));
    }

    fn reset(self: &Rc<Self>) {
        self.target.set(None);
        self.in_progress.set(false);
        self.text.replace(String::new());
        self.changed.notify();
    }
}

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
                let centre = gtk4::graphene::Point::new(x as f32 + half.x(), y as f32 + half.y());
                let transform = gtk4::gsk::Transform::new()
                    .translate(&centre)
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
    button.set_content(&Centred::integral(&symbol), 0, 0);
    (button, symbol)
}

fn pair(icon: &str, start: i32) -> (gtk4::Box, gtk4::Label, gtk4::Label) {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, PAIR_SPACING);
    row.set_margin_start(start);
    row.set_margin_end(PAIR_SIDE);
    let symbol = text::symbol_filled(icon, pixel_size::HUGE as f64, 1.0);
    text::set_color(&symbol, "colOnSurfaceVariant");
    row.append(&Centred::integral(&symbol));
    let label = text::styled("");
    text::set_color(&label, "colOnSurfaceVariant");
    row.append(&Centred::new(&label));
    (row, symbol, label)
}

struct Surface {
    window: gtk4::Window,
    field: gtk4::Text,
    dots: gtk4::DrawingArea,
    shift: Shift,
    placeholder: gtk4::Label,
    confirm: RippleButton,
    confirm_icon: gtk4::Label,
    power: RippleButton,
    reboot: RippleButton,
    islands: Islands,
    appear: Cell<(Tween, Tween)>,
    born: RefCell<Vec<i64>>,
    scroll: Cell<Tween>,
    caret: Cell<Tween>,
    shaking: Cell<i64>,
    theme: SharedTheme,
    kept: RefCell<Vec<Subscription>>,
    _scope: Scope,
    syncing: Cell<bool>,
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
        let centre = height / 2.0;
        let origin = FIELD_PADDING as f64 - scroll;
        let selection = theme.colors.col_secondary_container;
        let primary = theme.colors.col_primary;
        let resting = theme.colors.col_on_layer1;
        let selected_colour = theme.colors.col_on_secondary_container;
        for (index, born) in self.born.borrow().iter().enumerate() {
            let left = origin + CHAR_START + index as f64 * CHAR_SIZE;
            let selected = (index as i32) >= start && (index as i32) < end;
            if selected {
                cr.rectangle(left, centre - CHAR_SIZE / 2.0, CHAR_SIZE, CHAR_SIZE);
                cr.set_source_color(&selection);
                let _ = cr.fill();
            }
            let age = (now - born) as f64 / 1000.0;
            let part = |millis: f64| (age / millis).clamp(0.0, 1.0);
            let opacity = EXPRESSIVE_EFFECTS.at(part(SHAPE_FADE_MILLIS));
            let scale = 0.5 + 0.5 * EXPRESSIVE_FAST.at(part(SHAPE_SCALE_MILLIS));
            let size = CHAR_SHAPE * EXPRESSIVE_FAST.at(part(SHAPE_GROW_MILLIS)) * scale;
            let fade = EXPRESSIVE_EFFECTS.at(part(SHAPE_COLOUR_MILLIS));
            let mut colour = if selected {
                selected_colour
            } else {
                mix(primary, resting, fade)
            };
            colour.set_alpha(colour.alpha() * opacity as f32);
            let corner = left + (CHAR_SIZE - size) / 2.0;
            let shape = shapes::polygon(CHAR_SHAPES[index % CHAR_SHAPES.len()]);
            shape.trace(cr, corner, centre - size / 2.0, size);
            cr.set_source_color(&colour);
            let _ = cr.fill();
        }
        let caret = origin + self.caret.get().value(now);
        cr.rectangle(caret, centre - CHAR_SIZE / 2.0, CURSOR_WIDTH, CHAR_SIZE);
        cr.set_source_color(&primary);
        let _ = cr.fill();
    }

    fn animating(&self, now: i64) -> bool {
        let (scale, fade) = self.appear.get();
        let chars = self
            .born
            .borrow()
            .iter()
            .any(|born| (now - born) as f64 / 1000.0 < SHAPE_COLOUR_MILLIS);
        let shake: f64 = SHAKE.iter().map(|(_, millis)| millis).sum();
        let shaking =
            self.shaking.get() >= 0 && ((now - self.shaking.get()) as f64 / 1000.0) < shake;
        let moving = [scale, fade, self.scroll.get(), self.caret.get()]
            .iter()
            .any(|tween| tween.running(now));
        moving || chars || shaking
    }

    fn step(&self, now: i64) {
        let (scale, fade) = self.appear.get();
        self.islands.set_scale(scale.value(now));
        self.islands.set_opacity(fade.value(now));
        let shaking = self.shaking.get();
        if shaking >= 0 {
            let mut elapsed = (now - shaking) as f64 / 1000.0;
            let mut from = 0.0;
            let mut offset = 0.0;
            for (to, millis) in SHAKE {
                if elapsed <= millis {
                    offset = from + (to - from) * (elapsed / millis);
                    break;
                }
                elapsed -= millis;
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

pub struct Lock {
    theme: SharedTheme,
    services: Rc<Services>,
    instance: RefCell<Option<Rc<SessionLock>>>,
    context: Rc<Context>,
    surfaces: RefCell<Vec<Rc<Surface>>>,
    saved: RefCell<Vec<(String, i64)>>,
    notifier: RefCell<Option<Rc<LockNotifier>>>,
    reader: RefCell<Option<Rc<Reader>>>,
    session_locked: Cell<bool>,
    granted: Cell<bool>,
    seen: Cell<bool>,
    waiting: RefCell<Option<glib::SourceId>>,
}

impl Lock {
    pub fn new(theme: &SharedTheme, services: &Rc<Services>) -> Rc<Self> {
        let lock = Rc::new(Lock {
            theme: theme.clone(),
            services: services.clone(),
            instance: RefCell::new(None),
            context: Rc::new(Context::default()),
            surfaces: RefCell::new(Vec::new()),
            saved: RefCell::new(Vec::new()),
            notifier: RefCell::new(None),
            reader: RefCell::new(None),
            session_locked: Cell::new(false),
            granted: Cell::new(false),
            seen: Cell::new(false),
            waiting: RefCell::new(None),
        });
        let notifier = LockNotifier::watch({
            let lock = Rc::downgrade(&lock);
            move |locked| {
                if let Some(lock) = lock.upgrade() {
                    lock.session_changed(locked);
                }
            }
        });
        lock.notifier.replace(notifier);
        lock
    }

    pub fn lock(self: &Rc<Self>) {
        if config::value_bool("/lock/useHyprlock", false)
            && crate::core::process::exists("hyprlock")
        {
            crate::platform::desktop::shell("pidof hyprlock || hyprlock");
            return;
        }
        if self.instance.borrow().is_some() {
            return;
        }
        self.context.reset();
        let instance = Rc::new(SessionLock::new());
        instance.connect_monitor({
            let lock = Rc::downgrade(self);
            move |monitor| {
                if let Some(lock) = lock.upgrade() {
                    lock.cover(monitor);
                }
            }
        });
        instance.connect_locked({
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.granted();
                }
            }
        });
        instance.connect_unlocked({
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.released();
                }
            }
        });
        instance.connect_failed({
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.released();
                }
            }
        });
        self.instance.replace(Some(instance.clone()));
        self.granted.set(false);
        self.seen.set(false);
        self.services.states.set_screen_locked(true);
        instance.lock();
        if self.instance.borrow().is_none() {
            return;
        }
        self.hide_windows();
        self.watch_finger();
        let waiting = glib::timeout_add_local_once(GRANT_TIMEOUT, {
            let lock = Rc::downgrade(self);
            move || {
                if let Some(lock) = lock.upgrade() {
                    lock.waiting.take();
                    if !lock.granted.get() {
                        lock.released();
                    }
                }
            }
        });
        self.waiting.replace(Some(waiting));
    }

    fn watch_finger(self: &Rc<Self>) {
        let lock = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(system) = gio::bus_get_future(gio::BusType::System).await else {
                return;
            };
            let user = glib::user_name().to_string_lossy().into_owned();
            let Some(reader) = Reader::open(&system, &user).await else {
                return;
            };
            let Some(lock) = lock
                .upgrade()
                .filter(|lock| lock.instance.borrow().is_some())
            else {
                return;
            };
            if let Some(previous) = lock.reader.replace(Some(reader.clone())) {
                previous.stop();
            }
            lock.context.fingerprint.set(true);
            lock.context.changed.notify();
            let lock = Rc::downgrade(&lock);
            reader
                .verify(move || {
                    if let Some(lock) = lock.upgrade() {
                        lock.unlocked(lock.context.target());
                    }
                })
                .await;
        });
    }

    fn granted(&self) {
        if let Some(waiting) = self.waiting.take() {
            waiting.remove();
        }
        if let Some(notifier) = self.notifier.borrow().clone() {
            notifier.pump();
        }
        self.granted.set(true);
        self.seen.set(self.session_locked.get());
    }

    fn session_changed(self: &Rc<Self>, locked: bool) {
        self.session_locked.set(locked);
        if !self.granted.get() {
            return;
        }
        if locked {
            self.seen.set(true);
            return;
        }
        if !self.seen.get() {
            return;
        }
        let instance = self.instance.borrow().clone();
        match instance {
            Some(instance) => instance.unlock(),
            None => self.released(),
        }
    }

    pub fn focus(&self) {
        self.context.focus.notify();
    }

    pub fn start(self: &Rc<Self>) {
        let new_instance = persistent::is_new_hyprland_instance();
        if new_instance && config::value_bool("/lock/launchOnStartup", false) {
            self.lock();
        }
    }

    fn hide_windows(&self) {
        let monitors = hypr::json("monitors")
            .and_then(|monitors| monitors.as_array().cloned())
            .unwrap_or_default();
        let mut saved = Vec::new();
        for monitor in &monitors {
            let Some(name) = monitor.get("name").and_then(|name| name.as_str()) else {
                return;
            };
            let Some(workspace) = monitor
                .get("activeWorkspace")
                .and_then(|workspace| workspace.get("id"))
                .and_then(|id| id.as_i64())
            else {
                return;
            };
            saved.push((name.to_owned(), workspace));
        }
        for (name, workspace) in &saved {
            hypr::request(&format!("dispatch hl.dsp.focus({{monitor=\"{name}\"}})"));
            hypr::request(&format!(
                "dispatch hl.dsp.focus({{workspace={}}})",
                HIDDEN_WORKSPACE - workspace
            ));
        }
        self.saved.replace(saved);
    }

    fn restore_windows(&self) {
        let saved = self.saved.take();
        glib::timeout_add_local_once(RESTORE_DELAY, move || {
            for (name, workspace) in saved {
                hypr::request(&format!("dispatch hl.dsp.focus({{monitor=\"{name}\"}})"));
                hypr::request(&format!("dispatch hl.dsp.focus({{workspace={workspace}}})"));
            }
        });
    }

    fn released(self: &Rc<Self>) {
        if let Some(waiting) = self.waiting.take() {
            waiting.remove();
        }
        self.granted.set(false);
        self.seen.set(false);
        self.instance.replace(None);
        if let Some(reader) = self.reader.take() {
            reader.stop();
        }
        self.context.fingerprint.set(false);
        self.surfaces.borrow_mut().clear();
        if self.services.states.screen_locked.get() {
            self.services.states.set_screen_locked(false);
            self.restore_windows();
        }
    }

    fn try_unlock(self: &Rc<Self>, also_inhibit: bool) {
        if self.context.in_progress.replace(true) {
            return;
        }
        self.context.also_inhibit.set(also_inhibit);
        self.context.changed.notify();
        let password = self.context.text.borrow().clone();
        let user = glib::user_name().to_string_lossy().into_owned();
        let lock = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let checked =
                gio::spawn_blocking(move || pam::authenticate(PAM_SERVICE, &user, password)).await;
            let Some(lock) = lock.upgrade() else {
                return;
            };
            if checked.unwrap_or(false) {
                lock.unlocked(lock.context.target());
                return;
            }
            lock.context.text.replace(String::new());
            lock.context.in_progress.set(false);
            lock.context.failed.set(true);
            lock.context.changed.notify();
            lock.context.shake.notify();
        });
    }

    fn unlocked(self: &Rc<Self>, target: Target) {
        match target {
            Target::Poweroff => {
                self.services.session.poweroff();
                return;
            }
            Target::Reboot => {
                self.services.session.reboot();
                return;
            }
            Target::Unlock => {}
        }
        if config::value_bool("/lock/security/unlockKeyring", true) {
            let password = self.context.text.borrow().clone();
            self.services.prompter.unlocked_with(&password);
            gio::spawn_blocking(move || unlock_keyring(password));
        }
        let instance = self.instance.borrow().clone();
        if let Some(instance) = instance {
            instance.unlock();
        }
        self.context.reset();
        if self.context.also_inhibit.replace(false) && !self.services.session.awake.get() {
            self.services.session.toggle_awake();
        }
    }

    fn cover(self: &Rc<Self>, monitor: &gdk::Monitor) {
        let Some(instance) = self.instance.borrow().clone() else {
            return;
        };
        let surface = self.surface();
        instance.assign(&surface.window, monitor);
        surface.window.present();
        surface.field.grab_focus();
        self.surfaces.borrow_mut().push(surface);
    }

    fn surface(self: &Rc<Self>) -> Rc<Surface> {
        let theme = &self.theme;
        let context = &self.context;

        let field = gtk4::Text::new();
        field.add_css_class("lock-field");
        field.set_visibility(false);
        field.set_margin_start(FIELD_PADDING);
        field.set_margin_end(FIELD_PADDING);
        field.set_hexpand(true);
        let dots = gtk4::DrawingArea::new();
        dots.set_size_request(FIELD_WIDTH, BUTTON);
        let placeholder = text::styled(&tr("Enter password"));
        text::set_color(&placeholder, "colSubtext");
        placeholder.set_xalign(0.0);
        let placeholder_box = Centred::filling_width(&placeholder);
        placeholder_box.set_margin_start(PLACEHOLDER_START);
        placeholder_box.set_can_target(false);
        let field_stack = gtk4::Overlay::new();
        field_stack.set_child(Some(&dots));
        field_stack.add_overlay(&field);
        field_stack.add_overlay(&placeholder_box);
        let shift = Shift::new(&field_stack);
        shift.set_sideways(true);

        let (confirm, confirm_icon) = round_button(theme, "arrow_right_alt", CONFIRM_ICON);
        confirm.set_look(look(
            |theme| theme.colors.col_primary,
            |theme| theme.colors.col_primary_hover,
            |theme| theme.colors.col_primary_active,
        ));
        confirm.set_toggled(true);
        let fingerprint_icon = text::symbol_filled("fingerprint", pixel_size::HUGEASS as f64, 1.0);
        text::set_color(&fingerprint_icon, "colOnSurfaceVariant");
        let fingerprint = Centred::integral(&fingerprint_icon);
        fingerprint.set_margin_start(FINGERPRINT_START);
        fingerprint.set_margin_end(FINGERPRINT_END);
        let main = toolbar::frame();
        main.append(&fingerprint);
        main.append(&shift);
        main.append(&confirm);

        let (name_pair, _, name) = pair("account_circle", NAME_START);
        name.set_text(&display_name());
        let layout_row = gtk4::Box::new(gtk4::Orientation::Horizontal, LAYOUT_SPACING);
        layout_row.set_margin_end(LAYOUT_END);
        let keyboard = text::symbol_filled("keyboard_alt", pixel_size::HUGE as f64, 1.0);
        text::set_color(&keyboard, "colOnSurfaceVariant");
        layout_row.append(&Centred::integral(&keyboard));
        let layout_code = text::styled("");
        text::set_color(&layout_code, "colOnSurfaceVariant");
        layout_row.append(&Centred::new(&layout_code));
        let left = toolbar::frame();
        left.append(&name_pair);
        left.append(&layout_row);
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
            left.append(&fcitx);
        }

        let (battery, battery_icon, battery_level) = pair("battery_android_full", PAIR_SIDE);
        let (sleep, sleep_icon) = round_button(theme, "dark_mode", BUTTON_ICON);
        let secondary = || {
            look(
                |theme| theme.colors.col_secondary_container,
                |theme| theme.colors.col_secondary_container_hover,
                |theme| theme.colors.col_secondary_container_active,
            )
        };
        sleep.set_look(secondary());
        text::set_color(&sleep_icon, "colOnSurfaceVariant");
        let (power, power_icon) = round_button(theme, "power_settings_new", BUTTON_ICON);
        power.set_look(secondary());
        let (reboot, reboot_icon) = round_button(theme, "restart_alt", BUTTON_ICON);
        reboot.set_look(secondary());
        let right = toolbar::frame();
        right.append(&battery);
        right.append(&sleep);
        right.append(&power);
        right.append(&reboot);

        let islands = Islands::new(left.upcast_ref(), main.upcast_ref(), right.upcast_ref());
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
            field: field.clone(),
            dots: dots.clone(),
            shift,
            placeholder,
            confirm: confirm.clone(),
            confirm_icon,
            power: power.clone(),
            reboot: reboot.clone(),
            islands: islands.clone(),
            appear: Cell::new((scale, fade)),
            born: RefCell::new(Vec::new()),
            scroll: Cell::new(Tween::new(0.0, SCROLL_MILLIS, EMPHASIZED_DECEL)),
            caret: Cell::new(Tween::new(0.0, SCROLL_MILLIS, EMPHASIZED_DECEL)),
            shaking: Cell::new(-1),
            theme: theme.clone(),
            kept: RefCell::new(Vec::new()),
            _scope: scope,
            syncing: Cell::new(false),
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
                if surface.animating(now) {
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
                let text = context.text.borrow().clone();
                if surface.field.text() != text {
                    surface.syncing.set(true);
                    surface.field.set_text(&text);
                    surface.field.set_position(-1);
                    surface.syncing.set(false);
                }
                let length = text.chars().count();
                let now = frame_time(&surface.dots);
                {
                    let mut born = surface.born.borrow_mut();
                    born.truncate(length);
                    while born.len() < length {
                        born.push(now);
                    }
                }
                let width = (FIELD_WIDTH - 2 * FIELD_PADDING) as f64;
                let mut scroll = surface.scroll.get();
                scroll.retarget((length as f64 * CHAR_SIZE - width).max(0.0), now);
                surface.scroll.set(scroll);
                let mut caret = surface.caret.get();
                caret.retarget(CHAR_SIZE * surface.field.position() as f64, now);
                surface.caret.set(caret);
                fingerprint.set_visible(context.fingerprint.get());
                if config::value_bool("/lock/materialShapeChars", true) {
                    surface.field.remove_css_class("plain");
                } else {
                    surface.field.add_css_class("plain");
                }
                surface.placeholder.set_visible(text.is_empty());
                surface.placeholder.set_text(&tr(if context.failed.get() {
                    "Incorrect password"
                } else {
                    "Enter password"
                }));
                let busy = context.in_progress.get();
                surface.field.set_sensitive(!busy);
                surface.confirm.set_sensitive(!busy);
                text::set_color(
                    &surface.confirm_icon,
                    if busy { "colSubtext" } else { "colOnPrimary" },
                );
                surface.confirm_icon.set_text(match context.target() {
                    Target::Unlock if context.ctrl.get() => "coffee",
                    Target::Unlock => "arrow_right_alt",
                    Target::Poweroff => "power_settings_new",
                    Target::Reboot => "restart_alt",
                });
                let target = context.target.get();
                surface.power.set_toggled(target == Some(Target::Poweroff));
                surface.reboot.set_toggled(target == Some(Target::Reboot));
                let charge = services.battery.charge.get();
                battery.set_visible(charge.available);
                let charging = charge.charging();
                let (icon, full) = ("bolt", "battery_android_full");
                battery_icon.set_text(if charging { icon } else { full });
                battery_level.set_text(&format!("{}", (charge.percentage * 100.0).round()));
                let low = charge.percentage * 100.0 <= config::value_i64("/battery/low", 20) as f64;
                let (error, calm) = ("colError", "colOnSurfaceVariant");
                let tone = if low && !charging { error } else { calm };
                text::set_color(&battery_icon, tone);
                text::set_color(&battery_level, tone);
                layout_code.set_text(&services.xkb.current_code.borrow().to_uppercase());
                let guarded = [
                    (&power_icon, &surface.power),
                    (&reboot_icon, &surface.reboot),
                ];
                for (icon, button) in guarded {
                    text::set_color(
                        icon,
                        if button.toggled() {
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
        surface.kept.borrow_mut().push(context.changed.add({
            let refresh = refresh.clone();
            move || refresh()
        }));
        surface.kept.borrow_mut().push(context.shake.add({
            let surface = Rc::downgrade(&surface);
            move || {
                if let Some(surface) = surface.upgrade() {
                    surface.shaking.set(frame_time(&surface.dots));
                    start_ticking(&surface);
                }
            }
        }));
        surface.kept.borrow_mut().push(context.focus.add({
            let field = field.clone();
            move || {
                field.grab_focus();
            }
        }));
        surface
            .kept
            .borrow_mut()
            .push(self.services.battery.subscribe({
                let refresh = Rc::downgrade(&refresh);
                move || {
                    if let Some(refresh) = refresh.upgrade() {
                        refresh();
                    }
                }
            }));
        surface.kept.borrow_mut().push(self.services.xkb.subscribe({
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
                    .is_some_and(|surface| surface.syncing.get())
                {
                    return;
                }
                if let Some(context) = context.upgrade() {
                    context.set_text(&field.text());
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
                    let also_inhibit = lock.context.ctrl.get();
                    lock.try_unlock(also_inhibit);
                }
            }
        });
        confirm.connect_clicked({
            let lock = Rc::downgrade(self);
            move |_| {
                if let Some(lock) = lock.upgrade() {
                    lock.try_unlock(false);
                }
            }
        });
        sleep.connect_clicked({
            let session = self.services.session.clone();
            move |_| session.suspend()
        });
        for (button, target) in [(&power, Target::Poweroff), (&reboot, Target::Reboot)] {
            button.connect_clicked({
                let lock = Rc::downgrade(self);
                move |_| {
                    let Some(lock) = lock.upgrade() else {
                        return;
                    };
                    if !config::value_bool("/lock/security/requirePasswordToPower", false) {
                        lock.unlocked(target);
                        return;
                    }
                    let context = &lock.context;
                    if context.target.get() == Some(target) {
                        context.target.set(None);
                    } else {
                        context.target.set(Some(target));
                        context.focus.notify();
                    }
                    context.changed.notify();
                }
            });
        }

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let context = Rc::downgrade(context);
            let field = field.clone();
            move |_, key, _, _| {
                let Some(context) = context.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                context.restart_clear();
                if is_control(key) && !context.ctrl.replace(true) {
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
        keys.connect_key_released({
            let context = Rc::downgrade(context);
            let field = field.clone();
            move |_, key, _, _| {
                let Some(context) = context.upgrade() else {
                    return;
                };
                if is_control(key) && context.ctrl.replace(false) {
                    context.changed.notify();
                }
                if !field.has_focus() {
                    field.grab_focus();
                }
            }
        });
        window.add_controller(keys);
        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion({
            let field = field.clone();
            move |_, _, _| {
                if !field.has_focus() {
                    field.grab_focus();
                }
            }
        });
        window.add_controller(motion);
        let press = gtk4::GestureClick::new();
        press.connect_pressed({
            let field = field.clone();
            move |_, _, _, _| {
                if !field.has_focus() {
                    field.grab_focus();
                }
            }
        });
        window.add_controller(press);
        surface
    }
}

fn start_ticking(surface: &Rc<Surface>) {
    let now = frame_time(&surface.dots);
    if !surface.animating(now) {
        return;
    }
    let weak: Weak<Surface> = Rc::downgrade(surface);
    surface.islands.add_tick_callback(move |_, clock| {
        let Some(surface) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        let now = clock.frame_time();
        surface.step(now);
        if surface.animating(now) {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    });
}

fn unlock_keyring(password: String) {
    let mut secret = password.into_bytes();
    if let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) {
        unlock_login_keyring(&bus, &secret);
    }
    for byte in secret.iter_mut() {
        unsafe { std::ptr::write_volatile(byte, 0) };
    }
}

fn unlock_login_keyring(bus: &gio::DBusConnection, secret: &[u8]) -> Option<()> {
    let call = |path: &str, interface: &str, method: &str, arguments: glib::Variant| {
        bus.call_sync(
            Some(SECRETS),
            path,
            interface,
            method,
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            -1,
            gio::Cancellable::NONE,
        )
        .ok()
    };
    let locked = call(
        LOGIN_KEYRING,
        "org.freedesktop.DBus.Properties",
        "Get",
        ("org.freedesktop.Secret.Collection", "Locked").to_variant(),
    )
    .and_then(|reply| reply.child_value(0).as_variant())
    .and_then(|value| value.get::<bool>());
    if locked == Some(false) {
        return Some(());
    }
    let session = call(
        SECRETS_PATH,
        SECRET_SERVICE,
        "OpenSession",
        ("plain", "".to_variant()).to_variant(),
    )?
    .child_value(1);
    let master = glib::Variant::tuple_from_iter([
        session.clone(),
        glib::Variant::array_from_fixed_array::<u8>(&[]),
        glib::Variant::array_from_fixed_array(secret),
        "text/plain".to_variant(),
    ]);
    if locked.is_some() {
        let login = glib::variant::ObjectPath::try_from(LOGIN_KEYRING.to_owned()).ok()?;
        call(
            SECRETS_PATH,
            KEYRING_INTERNAL,
            "UnlockWithMasterPassword",
            glib::Variant::tuple_from_iter([login.to_variant(), master]),
        );
    } else {
        let properties: std::collections::HashMap<&str, glib::Variant> = [(
            "org.freedesktop.Secret.Collection.Label",
            "login".to_variant(),
        )]
        .into();
        let created = call(
            SECRETS_PATH,
            KEYRING_INTERNAL,
            "CreateWithMasterPassword",
            glib::Variant::tuple_from_iter([properties.to_variant(), master]),
        )
        .map(|reply| reply.child_value(0));
        call(
            LOGIN_KEYRING,
            "org.freedesktop.DBus.Properties",
            "Set",
            (
                "org.freedesktop.Secret.Collection",
                "Label",
                "Login".to_variant(),
            )
                .to_variant(),
        );
        let default = call(
            SECRETS_PATH,
            SECRET_SERVICE,
            "ReadAlias",
            ("default",).to_variant(),
        )
        .and_then(|reply| reply.child_value(0).str().map(str::to_owned));
        if let Some(created) = created
            && default.as_deref() == Some("/")
        {
            call(
                SECRETS_PATH,
                SECRET_SERVICE,
                "SetAlias",
                glib::Variant::tuple_from_iter(["default".to_variant(), created]),
            );
        }
    }
    call(
        session.str()?,
        "org.freedesktop.Secret.Session",
        "Close",
        ().to_variant(),
    );
    Some(())
}
