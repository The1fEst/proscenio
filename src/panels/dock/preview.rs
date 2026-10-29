use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::platform::hyprland::hypr;
use crate::platform::wayland::capture::Capture;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::group::{GroupButton, Look};

const MAX_WIDTH: f64 = 300.0;
const MAX_HEIGHT: f64 = 200.0;
const CONTROLS_HEIGHT: f64 = 30.0;
const ELEVATION: i32 = 10;
const PADDING: i32 = 5;
const CARD_SPACING: i32 = 5;
const TITLE_MARGIN: i32 = 5;
const CONTROLS_SPACING: i32 = 5;
const BUTTON_SIDE_PADDING: i32 = 2;
const MINIMIZED_OPACITY: f64 = 0.5;
const PREVIEW_RADIUS: f32 = rounding::SMALL as f32;
const SETTLE: Duration = Duration::from_millis(100);
const FADE_MILLIS: f64 = 200.0;
const GIVE_UP: u32 = 3;

#[derive(Clone, PartialEq)]
pub struct Target {
    pub address: String,
    pub title: String,
    pub is_minimized: bool,
}

struct Card {
    address: String,
    widget: RippleButton,
    title: gtk4::Label,
    picture: Paint,
    texture: Rc<RefCell<Option<gdk::Texture>>>,
    is_alive: Cell<bool>,
    failures: Cell<u32>,
}

impl Card {
    fn is_ready(&self) -> bool {
        self.failures.get() >= GIVE_UP || self.texture.borrow().is_some()
    }
}

pub struct Preview {
    theme: SharedTheme,
    capture: Option<Rc<Capture>>,
    popover: gtk4::Popover,
    background: gtk4::Box,
    row: gtk4::Box,
    app: RefCell<Option<String>>,
    targets: RefCell<Vec<Target>>,
    cards: RefCell<Vec<Rc<Card>>>,
    is_popup_hovered: Cell<bool>,
    is_button_hovered: Cell<bool>,
    center: Cell<i32>,
    is_requested: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    opacity: Cell<Tween>,
    is_ticking: Cell<bool>,
    on_changed: RefCell<Option<Box<dyn Fn()>>>,
}

impl Preview {
    pub fn new(theme: &SharedTheme, parent: &impl IsA<gtk4::Widget>) -> Rc<Self> {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, CARD_SPACING);
        row.set_margin_top(PADDING);
        row.set_margin_bottom(PADDING);
        row.set_margin_start(PADDING);
        row.set_margin_end(PADDING);
        let background = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        background.add_css_class("dock-preview");
        background.set_overflow(gtk4::Overflow::Hidden);
        background.append(&row);
        background.set_margin_top(ELEVATION);
        background.set_margin_bottom(ELEVATION);
        background.set_margin_start(ELEVATION);
        background.set_margin_end(ELEVATION);
        background.set_opacity(0.0);

        let popover = gtk4::Popover::new();
        popover.add_css_class("dock-preview-popover");
        popover.set_has_arrow(false);
        popover.set_autohide(false);
        popover.set_can_focus(false);
        popover.set_position(gtk4::PositionType::Top);
        popover.set_child(Some(&background));
        popover.set_parent(parent);
        crate::ui::unload::unrealize_when_hidden(&popover);

        let preview = Rc::new(Preview {
            theme: theme.clone(),
            capture: Capture::new(&popover),
            popover: popover.clone(),
            background,
            row,
            app: RefCell::new(None),
            targets: RefCell::new(Vec::new()),
            cards: RefCell::new(Vec::new()),
            is_popup_hovered: Cell::new(false),
            is_button_hovered: Cell::new(false),
            center: Cell::new(0),
            is_requested: Cell::new(false),
            timer: RefCell::new(None),
            opacity: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            is_ticking: Cell::new(false),
            on_changed: RefCell::new(None),
        });

        let hover = gtk4::EventControllerMotion::new();
        hover.connect_enter({
            let preview = Rc::downgrade(&preview);
            move |_, _, _| {
                if let Some(preview) = preview.upgrade() {
                    preview.is_popup_hovered.set(true);
                    preview.settle();
                }
            }
        });
        hover.connect_leave({
            let preview = Rc::downgrade(&preview);
            move |_| {
                if let Some(preview) = preview.upgrade() {
                    preview.is_popup_hovered.set(false);
                    preview.settle();
                }
            }
        });
        popover.add_controller(hover);
        let drag_over = gtk4::DropControllerMotion::new();
        drag_over.connect_enter({
            let preview = Rc::downgrade(&preview);
            move |_, _, _| {
                if let Some(preview) = preview.upgrade() {
                    preview.is_popup_hovered.set(true);
                    preview.settle();
                }
            }
        });
        drag_over.connect_leave({
            let preview = Rc::downgrade(&preview);
            move |_| {
                if let Some(preview) = preview.upgrade() {
                    preview.is_popup_hovered.set(false);
                    preview.settle();
                }
            }
        });
        popover.add_controller(drag_over);
        preview
    }

    pub fn connect_changed(&self, action: impl Fn() + 'static) {
        self.on_changed.replace(Some(Box::new(action)));
    }

    pub fn is_requested(&self) -> bool {
        self.is_requested.get()
    }

    pub fn app(&self) -> Option<String> {
        self.app.borrow().clone()
    }

    pub fn enter(self: &Rc<Self>, app: &str, center: i32, targets: Vec<Target>) {
        self.app.replace(Some(app.to_owned()));
        self.center.set(center);
        self.is_button_hovered.set(true);
        self.set_targets(targets);
        self.settle();
    }

    pub fn leave(self: &Rc<Self>, app: &str) {
        if self.app.borrow().as_deref() == Some(app) {
            self.is_button_hovered.set(false);
            self.settle();
        }
    }

    pub fn set_targets(self: &Rc<Self>, targets: Vec<Target>) {
        if *self.targets.borrow() == targets {
            return;
        }
        self.targets.replace(targets);
        if self.is_requested.get() {
            self.sync_cards();
            if !self.is_ready() {
                self.hold();
            }
        }
        self.settle();
    }

    fn is_ready(&self) -> bool {
        self.cards.borrow().iter().all(|card| card.is_ready())
    }

    fn hold(&self) {
        self.popover.popdown();
        let mut opacity = self.opacity.get();
        opacity.jump(0.0);
        self.opacity.set(opacity);
        self.background.set_opacity(0.0);
    }

    fn should_show(&self) -> bool {
        (self.is_popup_hovered.get() || self.is_button_hovered.get())
            && !self.targets.borrow().is_empty()
    }

    fn settle(self: &Rc<Self>) {
        if let Some(timer) = self.timer.take() {
            timer.remove();
        }
        let preview = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(SETTLE, move || {
            if let Some(preview) = preview.upgrade() {
                preview.timer.take();
                preview.apply();
            }
        });
        self.timer.replace(Some(source));
    }

    fn apply(self: &Rc<Self>) {
        let should_show = self.should_show();
        let was_requested = self.is_requested.replace(should_show);
        if should_show {
            self.popover
                .set_pointing_to(Some(&gdk::Rectangle::new(self.center.get(), 0, 1, 1)));
            self.sync_cards();
            if !self.is_ready() {
                self.hold();
                if !was_requested && let Some(on_changed) = self.on_changed.borrow().as_ref() {
                    on_changed();
                }
                return;
            }
            if !self.popover.is_visible() {
                self.popover.popup();
            }
        }
        let mut opacity = self.opacity.get();
        opacity.retarget(if should_show { 1.0 } else { 0.0 }, now(&self.popover));
        self.opacity.set(opacity);
        self.run();
        if was_requested != should_show
            && let Some(on_changed) = self.on_changed.borrow().as_ref()
        {
            on_changed();
        }
    }

    fn run(self: &Rc<Self>) {
        if self.is_ticking.replace(true) {
            return;
        }
        let preview = Rc::downgrade(self);
        self.background.add_tick_callback(move |_, clock| {
            let Some(preview) = preview.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            let opacity = preview.opacity.get();
            preview
                .background
                .set_opacity(opacity.value(now).clamp(0.0, 1.0));
            if opacity.is_running(now) {
                return glib::ControlFlow::Continue;
            }
            preview.is_ticking.set(false);
            if !preview.is_requested.get() {
                preview.popover.popdown();
                preview.clear_cards();
            }
            glib::ControlFlow::Break
        });
    }

    fn clear_cards(&self) {
        for card in self.cards.take() {
            card.is_alive.set(false);
            self.row.remove(&card.widget);
        }
    }

    fn sync_cards(self: &Rc<Self>) {
        let targets = self.targets.borrow().clone();
        let old = self.cards.take();
        let mut cards: Vec<Rc<Card>> = Vec::new();
        for target in &targets {
            let card = match old.iter().find(|card| card.address == target.address) {
                Some(card) => {
                    card.title.set_text(&target.title);
                    card.widget.set_opacity(choose_card_opacity(target));
                    card.clone()
                }
                None => {
                    let card = self.card(target);
                    self.row.append(&card.widget);
                    self.watch(&card);
                    card
                }
            };
            cards.push(card);
        }
        for card in &old {
            if !cards.iter().any(|new_card| Rc::ptr_eq(new_card, card)) {
                card.is_alive.set(false);
                self.row.remove(&card.widget);
            }
        }
        let mut previous: Option<gtk4::Widget> = None;
        for card in &cards {
            self.row
                .reorder_child_after(&card.widget, previous.as_ref());
            previous = Some(card.widget.clone().upcast());
        }
        self.cards.replace(cards);
    }

    fn card(&self, target: &Target) -> Rc<Card> {
        let title = text::styled(&target.title);
        text::set_color(&title, "m3onSurface");
        title.set_xalign(0.0);
        title.set_ellipsize(pango::EllipsizeMode::End);
        title.set_width_chars(1);
        title.set_max_width_chars(1);
        let title_holder = Centered::filling_width(&title);
        title_holder.set_hexpand(true);
        title_holder.set_margin_top(TITLE_MARGIN);
        title_holder.set_margin_bottom(TITLE_MARGIN);
        title_holder.set_margin_start(TITLE_MARGIN);
        title_holder.set_margin_end(TITLE_MARGIN);

        let close = GroupButton::new(&self.theme, CONTROLS_HEIGHT, CONTROLS_HEIGHT);
        close.set_look(Look {
            background: |theme| transparentize(theme.colors.col_surface_container, 1.0),
            ..Look::default()
        });
        close.set_radii(CONTROLS_HEIGHT / 2.0, CONTROLS_HEIGHT / 2.0);
        let cross = text::symbol("close", pixel_size::NORMAL as f64);
        text::set_color(&cross, "m3onSurface");
        close.set_content(&Centered::integral(&cross));
        close.connect_clicked({
            let address = target.address.clone();
            move || hypr::close_window(&address)
        });

        let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, CONTROLS_SPACING);
        controls.append(&title_holder);
        controls.append(&close);

        let texture: Rc<RefCell<Option<gdk::Texture>>> = Rc::new(RefCell::new(None));
        let picture = Paint::new({
            let texture = texture.clone();
            move |snapshot, width, height| {
                let Some(texture) = texture.borrow().clone() else {
                    return;
                };
                let bounds = graphene::Rect::new(0.0, 0.0, width, height);
                snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(bounds, PREVIEW_RADIUS));
                snapshot.append_scaled_texture(&texture, gsk::ScalingFilter::Trilinear, &bounds);
                snapshot.pop();
            }
        });
        picture.set_halign(gtk4::Align::Center);
        picture.set_valign(gtk4::Align::Center);
        picture.set_vexpand(true);

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, CARD_SPACING);
        column.append(&controls);
        column.append(&picture);

        let button = RippleButton::new(&self.theme);
        button.set_content(&column, BUTTON_SIDE_PADDING, 0);
        button.set_valign(gtk4::Align::Fill);
        button.set_opacity(choose_card_opacity(target));
        button.connect_clicked({
            let address = target.address.clone();
            move |_| hypr::focus_window(&address)
        });
        button.connect_middle({
            let address = target.address.clone();
            move |_| hypr::close_window(&address)
        });
        super::buttons::spring_load(&button, {
            let address = target.address.clone();
            move || hypr::focus_window(&address)
        });

        Rc::new(Card {
            address: target.address.clone(),
            widget: button,
            title,
            picture,
            texture,
            is_alive: Cell::new(true),
            failures: Cell::new(if self.capture.is_none() { GIVE_UP } else { 0 }),
        })
    }

    fn watch(self: &Rc<Self>, card: &Rc<Card>) {
        if let Some(capture) = self.capture.as_ref() {
            follow(capture, card.clone(), Rc::downgrade(self));
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.clear_cards();
        if let Some(capture) = self.capture.as_ref() {
            capture.stop();
        }
    }
}

fn follow(capture: &Rc<Capture>, card: Rc<Card>, preview: std::rc::Weak<Preview>) {
    let next = capture.clone();
    let scale = card.picture.scale_factor().max(1) as f64;
    let limit = ((MAX_WIDTH * scale) as u32, (MAX_HEIGHT * scale) as u32);
    let address = card.address.clone();
    let has_texture = card.texture.borrow().is_some();
    let on_capture = move |texture: Option<gdk::Texture>| {
        if !card.is_alive.get() {
            return;
        }
        let was_waiting = !card.is_ready();
        match texture {
            Some(texture) => {
                let (width, height) = fit(texture.width() as f64, texture.height() as f64);
                card.picture.set_size_request(width, height);
                card.texture.replace(Some(texture));
                card.picture.queue_draw();
            }
            None => card.failures.set(card.failures.get() + 1),
        }
        if was_waiting
            && card.is_ready()
            && let Some(preview) = preview.upgrade()
        {
            preview.apply();
        }
        glib::idle_add_local_once(move || {
            if card.is_alive.get() {
                follow(&next, card, preview);
            }
        });
    };
    if has_texture {
        capture.next_frame(&address, limit, on_capture);
    } else {
        capture.grab(&address, limit, on_capture);
    }
}

fn choose_card_opacity(target: &Target) -> f64 {
    if target.is_minimized {
        MINIMIZED_OPACITY
    } else {
        1.0
    }
}

fn fit(width: f64, height: f64) -> (i32, i32) {
    if width <= 0.0 || height <= 0.0 {
        return (0, 0);
    }
    let scale = (MAX_WIDTH / width).min(MAX_HEIGHT / height);
    (
        (width * scale).round() as i32,
        (height * scale).round() as i32,
    )
}

fn now(widget: &impl IsA<gtk4::Widget>) -> i64 {
    widget
        .frame_clock()
        .map(|clock| clock.frame_time())
        .unwrap_or_else(glib::monotonic_time)
}
