use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::panels::notifications::card;
use crate::panels::notifications::card::Placement;
use crate::services::notifications::{Group, Notifications};
use crate::ui::anim::{EMPHASIZED_DECEL, Tween};
use crate::ui::shapes::Shape;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::column::Column;
use crate::ui::widgets::group::{ButtonGroup, GroupButton, Look as ButtonLook};
use crate::ui::widgets::materialshape::MaterialShape;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::slide::{DragList, Slide};
use crate::ui::widgets::{coalesce, text};

const MINIMUM: i32 = 170;
const GAP: i32 = 5;
const STATUS_HEIGHT: f64 = 36.0;
const PLACEHOLDER_ICON: f64 = 56.0;
const PLACEHOLDER_PADDING: i32 = 12;
const ENTER_MILLIS: f64 = 400.0;

pub fn build(notifications: &Notifications, theme: &SharedTheme, scope: &Scope) -> gtk4::Widget {
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    list.set_valign(gtk4::Align::Start);

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(&list));
    crate::ui::widgets::flickable::follow_scroll_settings(&scroll);
    scroll.add_css_class("notif-scroll");
    scroll.set_overflow(gtk4::Overflow::Hidden);

    let empty = Placeholder::new(theme, "notifications_active", "Nothing");
    let stack = gtk4::Overlay::new();
    stack.set_vexpand(true);
    stack.set_child(Some(&scroll));
    stack.add_overlay(&empty.widget);

    let (silent, silent_text) = status_button(theme, Some("notifications_paused"), false);
    silent.set_fill(false);
    let (quiet, count) = status_button(theme, None, true);
    quiet.set_fill(true);
    quiet.set_enabled(false);
    let (sweep, _) = status_button(theme, Some("delete_sweep"), false);
    sweep.set_fill(false);

    let status = ButtonGroup::new(theme);
    status.set_hexpand(true);
    status.append(&silent);
    status.append(&quiet);
    status.append(&sweep);

    silent.connect_clicked({
        let notifications = notifications.clone();
        move || notifications.set_silent(!notifications.silent.get())
    });
    sweep.connect_clicked({
        let notifications = notifications.clone();
        move || notifications.discard_all()
    });

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, GAP);
    column.add_css_class("notif-list");
    column.set_vexpand(true);
    column.set_size_request(-1, MINIMUM);
    column.append(&stack);
    column.append(&status);

    let cards = CardList::new(&list, notifications, theme, false);
    let rebuild: Rc<dyn Fn()> = {
        let notifications = notifications.clone();
        Rc::new(move || {
            cards.update(notifications.groups(false));

            let held = notifications.list.borrow().len();
            empty.show(held == 0);
            count(&format!("{held} notifications"));
            let silenced = notifications.silent.get();
            silent.set_toggled(silenced);
            silent_text(if silenced { "on" } else { "" });
        })
    };
    rebuild();
    scope.keep(notifications.subscribe({
        let queue = coalesce(rebuild);
        move || queue()
    }));

    column.upcast()
}

struct Card {
    key: String,
    ids: Vec<u32>,
    slide: Slide,
    alive: Rc<Cell<bool>>,
    leaving: Cell<bool>,
}

pub struct CardList {
    list: gtk4::Box,
    notifications: Notifications,
    theme: SharedTheme,
    popup: bool,
    cards: RefCell<Vec<Rc<Card>>>,
    expanded: RefCell<HashMap<String, Rc<Cell<bool>>>>,
    groups: Rc<DragList>,
}

impl CardList {
    pub fn new(
        list: &gtk4::Box,
        notifications: &Notifications,
        theme: &SharedTheme,
        popup: bool,
    ) -> Rc<Self> {
        Rc::new(CardList {
            list: list.clone(),
            notifications: notifications.clone(),
            theme: theme.clone(),
            popup,
            cards: RefCell::new(Vec::new()),
            expanded: RefCell::new(HashMap::new()),
            groups: DragList::new(),
        })
    }

    fn make(&self, group: &Group) -> Rc<Card> {
        let expanded = self
            .expanded
            .borrow_mut()
            .entry(group.app_name.clone())
            .or_insert_with(|| Rc::new(Cell::new(false)))
            .clone();
        let alive = Rc::new(Cell::new(true));
        let slide = card::build(
            group,
            &self.notifications,
            &self.theme,
            &alive,
            Placement {
                popup: self.popup,
                expanded,
                groups: self.groups.clone(),
            },
        );
        Rc::new(Card {
            key: group.app_name.clone(),
            ids: group.ids(),
            slide,
            alive,
            leaving: Cell::new(false),
        })
    }

    pub fn update(self: &Rc<Self>, groups: Vec<Group>) {
        let mut old: Vec<Rc<Card>> = self.cards.take();
        let animate = self.list.is_mapped();
        let mut order: Vec<Rc<Card>> = Vec::new();
        for group in &groups {
            let found = old
                .iter()
                .position(|card| card.key == group.app_name && !card.leaving.get());
            let card = match found.map(|index| old.remove(index)) {
                Some(card) if card.ids == group.ids() => card,
                Some(card) => {
                    card.alive.set(false);
                    self.list.remove(&card.slide);
                    self.make(group)
                }
                None => {
                    let card = self.make(group);
                    if animate {
                        card.slide.pop_in();
                    }
                    card
                }
            };
            order.push(card);
        }

        for card in old {
            if card.leaving.replace(true) {
                continue;
            }
            card.alive.set(false);
            if !animate {
                self.list.remove(&card.slide);
                continue;
            }
            let position = self
                .list
                .observe_children()
                .into_iter()
                .flatten()
                .position(|child| child == card.slide.clone().upcast::<glib::Object>())
                .unwrap_or(order.len());
            let list = self.list.clone();
            let slide = card.slide.clone();
            let cards = Rc::downgrade(self);
            let leaving = card.clone();
            card.slide.collapse(move || {
                list.remove(&slide);
                if let Some(cards) = cards.upgrade() {
                    cards
                        .cards
                        .borrow_mut()
                        .retain(|entry| !Rc::ptr_eq(entry, &leaving));
                }
            });
            order.insert(position.min(order.len()), card);
        }

        let mut previous: Option<gtk4::Widget> = None;
        self.groups.clear();
        for card in &order {
            let widget: gtk4::Widget = card.slide.clone().upcast();
            if widget.parent().is_none() {
                self.list.insert_child_after(&widget, previous.as_ref());
            } else {
                self.list.reorder_child_after(&widget, previous.as_ref());
            }
            if !card.leaving.get() {
                self.groups.push(&card.slide);
            }
            previous = Some(widget);
        }
        self.cards.replace(order);
    }
}

fn status_button(
    theme: &SharedTheme,
    icon: Option<&str>,
    with_text: bool,
) -> (GroupButton, Rc<dyn Fn(&str)>) {
    let button = GroupButton::new(theme, 0.0, STATUS_HEIGHT);
    button.set_look(ButtonLook {
        background: |theme| theme.colors.col_layer2,
        hover: |theme| theme.colors.col_layer2_hover,
        active: |theme| theme.colors.col_layer2_active,
        ..ButtonLook::default()
    });
    button.set_radii(STATUS_HEIGHT / 2.0, rounding::SMALL as f64);
    button.jump_radius();

    let inside = Row::new(GAP);
    let symbol = icon.map(|icon| {
        let symbol = text::symbol(icon, pixel_size::HUGE as f64);
        text::set_color(&symbol, "colOnLayer1");
        inside.append(&symbol);
        symbol
    });
    let label = with_text.then(|| {
        let label = text::styled_sized("", pixel_size::SMALL);
        text::set_color(&label, "colOnLayer1");
        inside.append(&label);
        label
    });
    let content = Centred::integral(&inside);
    button.set_content(&content);

    let update: Rc<dyn Fn(&str)> = {
        let button = button.clone();
        Rc::new(move |value: &str| {
            let token = if button.toggled() {
                "m3onPrimary"
            } else {
                "colOnLayer1"
            };
            if let Some(symbol) = &symbol {
                text::set_color(symbol, token);
            }
            if let Some(label) = &label {
                if with_text {
                    label.set_text(value);
                }
                text::set_color(label, token);
            }
            let width = inside.measure(gtk4::Orientation::Horizontal, -1).1 as f64 + 46.0;
            button.set_base_size(width, STATUS_HEIGHT);
            button.set_clicked_width(width + 6.0);
        })
    };
    update("");
    (button, update)
}

pub struct Placeholder {
    pub widget: gtk4::Widget,
    shown: Rc<Cell<Tween>>,
    ticking: Rc<Cell<bool>>,
    badge: Centred,
    shift: text::Shift,
}

impl Placeholder {
    pub fn new(theme: &SharedTheme, icon: &str, description: &str) -> Rc<Self> {
        Self::build(theme, icon, None, Some(description), Shape::Ghostish)
    }

    pub fn titled(theme: &SharedTheme, icon: &str, title: &str, shape: Shape) -> Rc<Self> {
        Self::build(theme, icon, Some(title), None, shape)
    }

    pub fn build(
        theme: &SharedTheme,
        icon: &str,
        title: Option<&str>,
        description: Option<&str>,
        shape: Shape,
    ) -> Rc<Self> {
        let symbol = text::symbol(icon, PLACEHOLDER_ICON);
        text::set_color(&symbol, "colOnSecondaryContainer");
        let (width, height) = (
            symbol.measure(gtk4::Orientation::Horizontal, -1).1,
            symbol.measure(gtk4::Orientation::Vertical, -1).1,
        );
        let size = width.max(height) + PLACEHOLDER_PADDING * 2;
        let shape = MaterialShape::new(theme, shape, size, |theme| {
            theme.colors.col_secondary_container
        });
        let badge = gtk4::Overlay::new();
        badge.set_child(Some(&shape.area));
        badge.add_overlay(&Centred::new(&symbol));
        let badge = Centred::new(&badge);

        let column = Column::new(GAP, true);
        column.append(&badge);
        if let Some(title) = title {
            let heading = text::styled(title);
            text::set_font(
                &heading,
                text::Family::Title,
                pixel_size::LARGER as f64,
                "wght=550",
            );
            text::set_color(&heading, "m3outline");
            heading.set_justify(gtk4::Justification::Center);
            column.append(&heading);
        }
        if let Some(description) = description {
            let body = text::styled_sized(description, pixel_size::SMALL);
            text::set_color(&body, "m3outline");
            body.set_justify(gtk4::Justification::Center);
            body.set_wrap(true);
            column.append(&body);
        }
        let shift = text::Shift::new(&Centred::integral(&column));

        Rc::new(Placeholder {
            widget: shift.clone().upcast(),
            shown: Rc::new(Cell::new(Tween::new(1.0, ENTER_MILLIS, EMPHASIZED_DECEL))),
            ticking: Rc::new(Cell::new(false)),
            badge,
            shift,
        })
    }

    pub fn show(&self, shown: bool) {
        let widget = &self.widget;
        let now = widget
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let mut tween = self.shown.get();
        let target = if shown { 1.0 } else { 0.0 };
        if widget.is_mapped() {
            tween.retarget(target, now);
        } else {
            tween.jump(target);
        }
        self.shown.set(tween);
        self.apply(tween.value(now));
        if self.ticking.replace(true) {
            return;
        }
        let (shown, ticking, badge) = (
            self.shown.clone(),
            self.ticking.clone(),
            self.badge.downgrade(),
        );
        widget.add_tick_callback(move |widget, clock| {
            let (Some(badge), Some(shift)) =
                (badge.upgrade(), widget.downcast_ref::<text::Shift>())
            else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            let tween = shown.get();
            let opacity = tween.value(now);
            apply(widget, &badge, shift, opacity);
            if tween.running(now) {
                return glib::ControlFlow::Continue;
            }
            ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn apply(&self, opacity: f64) {
        apply(&self.widget, &self.badge, &self.shift, opacity);
    }
}

fn apply(widget: &gtk4::Widget, badge: &Centred, shift: &text::Shift, opacity: f64) {
    widget.set_opacity(opacity.clamp(0.0, 1.0));
    widget.set_visible(opacity > 0.0);
    shift.set_offset((-30.0 * (1.0 - opacity)) as f32);
    badge.set_rotation((-30.0 * (1.0 - opacity)) as f32);
}
