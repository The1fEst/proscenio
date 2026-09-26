use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::platform::capture::Capture;
use crate::platform::hypr;
use crate::ui::anim::{EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::group::{GroupButton, Look};
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::text;

const MAX_WIDTH: f64 = 300.0;
const MAX_HEIGHT: f64 = 200.0;
const CONTROLS_HEIGHT: f64 = 30.0;
const ELEVATION: i32 = 10;
const PADDING: i32 = 5;
const CARD_SPACING: i32 = 5;
const TITLE_MARGIN: i32 = 5;
const CONTROLS_SPACING: i32 = 5;
const BUTTON_SIDE_PADDING: i32 = 2;
const PREVIEW_RADIUS: f32 = rounding::SMALL as f32;
const SETTLE: Duration = Duration::from_millis(100);
const FADE_MILLIS: f64 = 200.0;
const GIVE_UP: u32 = 3;

#[derive(Clone, PartialEq)]
pub struct Target {
    pub address: String,
    pub title: String,
}

struct Card {
    address: String,
    widget: RippleButton,
    title: gtk4::Label,
    picture: Paint,
    texture: Rc<RefCell<Option<gdk::Texture>>>,
    alive: Cell<bool>,
    failures: Cell<u32>,
}

impl Card {
    fn ready(&self) -> bool {
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
    popup_hovered: Cell<bool>,
    button_hovered: Cell<bool>,
    centre: Cell<i32>,
    show: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    opacity: Cell<Tween>,
    ticking: Cell<bool>,
    changed: RefCell<Option<Box<dyn Fn()>>>,
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
        crate::ui::unload::when_hidden(&popover);

        let preview = Rc::new(Preview {
            theme: theme.clone(),
            capture: Capture::new(&popover),
            popover: popover.clone(),
            background,
            row,
            app: RefCell::new(None),
            targets: RefCell::new(Vec::new()),
            cards: RefCell::new(Vec::new()),
            popup_hovered: Cell::new(false),
            button_hovered: Cell::new(false),
            centre: Cell::new(0),
            show: Cell::new(false),
            timer: RefCell::new(None),
            opacity: Cell::new(Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS)),
            ticking: Cell::new(false),
            changed: RefCell::new(None),
        });

        let hover = gtk4::EventControllerMotion::new();
        hover.connect_enter({
            let preview = Rc::downgrade(&preview);
            move |_, _, _| {
                if let Some(preview) = preview.upgrade() {
                    preview.popup_hovered.set(true);
                    preview.settle();
                }
            }
        });
        hover.connect_leave({
            let preview = Rc::downgrade(&preview);
            move |_| {
                if let Some(preview) = preview.upgrade() {
                    preview.popup_hovered.set(false);
                    preview.settle();
                }
            }
        });
        popover.add_controller(hover);
        preview
    }

    pub fn connect_changed(&self, action: impl Fn() + 'static) {
        self.changed.replace(Some(Box::new(action)));
    }

    pub fn shown(&self) -> bool {
        self.show.get()
    }

    pub fn app(&self) -> Option<String> {
        self.app.borrow().clone()
    }

    pub fn enter(self: &Rc<Self>, app: &str, centre: i32, targets: Vec<Target>) {
        self.app.replace(Some(app.to_owned()));
        self.centre.set(centre);
        self.button_hovered.set(true);
        self.set_targets(targets);
        self.settle();
    }

    pub fn leave(self: &Rc<Self>, app: &str) {
        if self.app.borrow().as_deref() == Some(app) {
            self.button_hovered.set(false);
            self.settle();
        }
    }

    pub fn set_targets(self: &Rc<Self>, targets: Vec<Target>) {
        if *self.targets.borrow() == targets {
            return;
        }
        self.targets.replace(targets);
        if self.show.get() {
            self.sync_cards();
            if !self.ready() {
                self.hold();
            }
        }
        self.settle();
    }

    fn ready(&self) -> bool {
        self.cards.borrow().iter().all(|card| card.ready())
    }

    fn hold(&self) {
        self.popover.popdown();
        let mut opacity = self.opacity.get();
        opacity.jump(0.0);
        self.opacity.set(opacity);
        self.background.set_opacity(0.0);
    }

    fn should_show(&self) -> bool {
        (self.popup_hovered.get() || self.button_hovered.get()) && !self.targets.borrow().is_empty()
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
        let show = self.should_show();
        let was = self.show.replace(show);
        if show {
            self.popover
                .set_pointing_to(Some(&gdk::Rectangle::new(self.centre.get(), 0, 1, 1)));
            self.sync_cards();
            if !self.ready() {
                self.hold();
                if !was && let Some(changed) = self.changed.borrow().as_ref() {
                    changed();
                }
                return;
            }
            if !self.popover.is_visible() {
                self.popover.popup();
            }
        }
        let mut opacity = self.opacity.get();
        opacity.retarget(if show { 1.0 } else { 0.0 }, now(&self.popover));
        self.opacity.set(opacity);
        self.run();
        if was != show
            && let Some(changed) = self.changed.borrow().as_ref()
        {
            changed();
        }
    }

    fn run(self: &Rc<Self>) {
        if self.ticking.replace(true) {
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
            if opacity.running(now) {
                return glib::ControlFlow::Continue;
            }
            preview.ticking.set(false);
            if !preview.show.get() {
                preview.popover.popdown();
                preview.clear_cards();
            }
            glib::ControlFlow::Break
        });
    }

    fn clear_cards(&self) {
        for card in self.cards.take() {
            card.alive.set(false);
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
            if !cards.iter().any(|kept| Rc::ptr_eq(kept, card)) {
                card.alive.set(false);
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
        let title_holder = Centred::filling_width(&title);
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
        close.set_content(&Centred::integral(&cross));
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
        button.connect_clicked({
            let address = target.address.clone();
            move |_| hypr::focus_window(&address)
        });
        button.connect_middle({
            let address = target.address.clone();
            move |_| hypr::close_window(&address)
        });

        Rc::new(Card {
            address: target.address.clone(),
            widget: button,
            title,
            picture,
            texture,
            alive: Cell::new(true),
            failures: Cell::new(if self.capture.is_none() { GIVE_UP } else { 0 }),
        })
    }

    fn watch(self: &Rc<Self>, card: &Rc<Card>) {
        if let Some(capture) = self.capture.as_ref() {
            follow(capture, card.clone(), Rc::downgrade(self));
        }
    }
}

fn follow(capture: &Rc<Capture>, card: Rc<Card>, preview: std::rc::Weak<Preview>) {
    let next = capture.clone();
    let scale = card.picture.scale_factor().max(1) as f64;
    let limit = ((MAX_WIDTH * scale) as u32, (MAX_HEIGHT * scale) as u32);
    let address = card.address.clone();
    let shown = card.texture.borrow().is_some();
    let done = move |texture: Option<gdk::Texture>| {
        if !card.alive.get() {
            return;
        }
        let waiting = !card.ready();
        match texture {
            Some(texture) => {
                let (width, height) = fit(texture.width() as f64, texture.height() as f64);
                card.picture.set_size_request(width, height);
                card.texture.replace(Some(texture));
                card.picture.queue_draw();
            }
            None => card.failures.set(card.failures.get() + 1),
        }
        if waiting
            && card.ready()
            && let Some(preview) = preview.upgrade()
        {
            preview.apply();
        }
        glib::idle_add_local_once(move || {
            if card.alive.get() {
                follow(&next, card, preview);
            }
        });
    };
    if shown {
        capture.next_frame(&address, limit, done);
    } else {
        capture.grab(&address, limit, done);
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
