use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use crate::core::i18n::{tr, trn};
use crate::core::scope::Scope;
use crate::panels::notifications::card;
use crate::panels::notifications::card::Placement;
use crate::services::overlay::notifications::{Group, Notification, Notifications};
use crate::ui::anim::{EMPHASIZED_DECEL, Tween};
use crate::ui::shapes::Shape;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::materialshape::MaterialShape;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::column::Column;
use crate::ui::widgets::layout::group::{ButtonGroup, GroupButton, Look as ButtonLook};
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::slide::{DragList, Slide};
use crate::ui::widgets::{coalesce, display::text};

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
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);
    scroll.add_css_class("notif-scroll");
    scroll.set_overflow(gtk4::Overflow::Hidden);

    let empty_placeholder = Placeholder::new(theme, "notifications_active", &tr("Nothing"));
    let stack = gtk4::Overlay::new();
    stack.set_vexpand(true);
    stack.set_child(Some(&scroll));
    stack.add_overlay(&empty_placeholder.widget);

    let (silent_button, refresh_silent_button) =
        status_button(theme, Some("notifications_paused"), false);
    silent_button.set_fill(false);
    let (count_button, set_count_text) = status_button(theme, None, true);
    count_button.set_fill(true);
    count_button.set_enabled(false);
    let (sweep_button, _) = status_button(theme, Some("delete_sweep"), false);
    sweep_button.set_fill(false);

    let status_group = ButtonGroup::new(theme);
    status_group.set_hexpand(true);
    status_group.append(&silent_button);
    status_group.append(&count_button);
    status_group.append(&sweep_button);

    silent_button.connect_clicked({
        let notifications = notifications.clone();
        move || notifications.set_silent(!notifications.is_silent.get())
    });
    sweep_button.connect_clicked({
        let notifications = notifications.clone();
        move || notifications.discard_all()
    });

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, GAP);
    column.add_css_class("notif-list");
    column.set_vexpand(true);
    column.set_size_request(-1, MINIMUM);
    column.append(&stack);
    column.append(&status_group);

    let cards = CardList::new(&list, notifications, theme, false);
    let rebuild: Rc<dyn Fn()> = {
        let notifications = notifications.clone();
        Rc::new(move || {
            cards.update(notifications.groups(false));

            let notification_count = notifications.list.borrow().len();
            empty_placeholder.show(notification_count == 0);
            set_count_text(Some(&trn(
                "%1 notification",
                "%1 notifications",
                notification_count as u64,
            )));
            silent_button.set_toggled(notifications.is_silent.get());
            refresh_silent_button(None);
        })
    };
    rebuild();
    scope.keep(notifications.subscribe({
        let schedule_rebuild = coalesce(rebuild);
        move || schedule_rebuild()
    }));

    column.upcast()
}

struct Card {
    key: String,
    notifications: Vec<Notification>,
    slide: Slide,
    is_alive: Rc<Cell<bool>>,
    is_leaving: Cell<bool>,
    is_hovered: Rc<Cell<bool>>,
}

impl Card {
    fn release_hold(&self, notifications: &Notifications) {
        if self.is_hovered.replace(false) {
            let ids: Vec<u32> = self.notifications.iter().map(|entry| entry.id).collect();
            notifications.release(&ids);
        }
    }
}

pub struct CardList {
    list: gtk4::Box,
    notifications: Notifications,
    theme: SharedTheme,
    is_popup: bool,
    cards: RefCell<Vec<Rc<Card>>>,
    expanded_states: RefCell<HashMap<String, Rc<Cell<bool>>>>,
    group_drags: Rc<DragList>,
}

impl CardList {
    pub fn new(
        list: &gtk4::Box,
        notifications: &Notifications,
        theme: &SharedTheme,
        is_popup: bool,
    ) -> Rc<Self> {
        Rc::new(CardList {
            list: list.clone(),
            notifications: notifications.clone(),
            theme: theme.clone(),
            is_popup,
            cards: RefCell::new(Vec::new()),
            expanded_states: RefCell::new(HashMap::new()),
            group_drags: DragList::new(),
        })
    }

    fn make(&self, group: &Group) -> Rc<Card> {
        let is_expanded = self
            .expanded_states
            .borrow_mut()
            .entry(group.app_name.clone())
            .or_insert_with(|| Rc::new(Cell::new(false)))
            .clone();
        let is_alive = Rc::new(Cell::new(true));
        let is_hovered = Rc::new(Cell::new(false));
        let slide = card::build(
            group,
            &self.notifications,
            &self.theme,
            &is_alive,
            Placement {
                is_popup: self.is_popup,
                is_expanded,
                group_drags: self.group_drags.clone(),
                is_hovered: is_hovered.clone(),
            },
        );
        Rc::new(Card {
            key: group.app_name.clone(),
            notifications: group.notifications.clone(),
            slide,
            is_alive,
            is_leaving: Cell::new(false),
            is_hovered,
        })
    }

    fn retire(&self, card: &Card) {
        card.is_alive.set(false);
        card.release_hold(&self.notifications);
    }

    pub fn release_holds(&self) {
        for card in self.cards.borrow().iter() {
            card.release_hold(&self.notifications);
        }
    }

    pub fn update(self: &Rc<Self>, groups: Vec<Group>) {
        let mut old: Vec<Rc<Card>> = self.cards.take();
        let should_animate = self.list.is_mapped();
        let mut order: Vec<Rc<Card>> = Vec::new();
        for group in &groups {
            let matching_index = old
                .iter()
                .position(|card| card.key == group.app_name && !card.is_leaving.get());
            let card = match matching_index.map(|index| old.remove(index)) {
                Some(card) if group.is_shown_like(&card.notifications) => card,
                Some(card) => {
                    self.retire(&card);
                    self.list.remove(&card.slide);
                    self.make(group)
                }
                None => {
                    let card = self.make(group);
                    if should_animate {
                        card.slide.pop_in();
                    }
                    card
                }
            };
            order.push(card);
        }

        for card in old {
            if card.is_leaving.replace(true) {
                continue;
            }
            self.retire(&card);
            if !should_animate {
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
            let card_list = Rc::downgrade(self);
            let leaving_card = card.clone();
            card.slide.collapse(move || {
                list.remove(&slide);
                if let Some(card_list) = card_list.upgrade() {
                    card_list
                        .cards
                        .borrow_mut()
                        .retain(|entry| !Rc::ptr_eq(entry, &leaving_card));
                }
            });
            order.insert(position.min(order.len()), card);
        }

        let mut previous: Option<gtk4::Widget> = None;
        self.group_drags.clear();
        for card in &order {
            let widget: gtk4::Widget = card.slide.clone().upcast();
            if widget.parent().is_none() {
                self.list.insert_child_after(&widget, previous.as_ref());
            } else {
                self.list.reorder_child_after(&widget, previous.as_ref());
            }
            if !card.is_leaving.get() {
                self.group_drags.push(&card.slide);
            }
            previous = Some(widget);
        }
        self.cards.replace(order);
    }
}

impl Drop for CardList {
    fn drop(&mut self) {
        self.release_holds();
    }
}

fn status_button(
    theme: &SharedTheme,
    icon: Option<&str>,
    has_text: bool,
) -> (GroupButton, Rc<dyn Fn(Option<&str>)>) {
    let button = GroupButton::new(theme, 0.0, STATUS_HEIGHT);
    button.set_look(ButtonLook {
        background: |theme| theme.colors.col_layer2,
        hover: |theme| theme.colors.col_layer2_hover,
        active: |theme| theme.colors.col_layer2_active,
        ..ButtonLook::default()
    });
    button.set_radii(STATUS_HEIGHT / 2.0, rounding::SMALL as f64);
    button.jump_radius();

    let content = Row::new(GAP);
    let symbol = icon.map(|icon| {
        let symbol = text::symbol(icon, pixel_size::HUGE as f64);
        text::set_color(&symbol, "colOnLayer1");
        content.append(&symbol);
        symbol
    });
    let label = has_text.then(|| {
        let label = text::styled_sized("", pixel_size::SMALL);
        text::set_color(&label, "colOnLayer1");
        content.append(&label);
        label
    });
    let content_box = Centered::integral(&content);
    button.set_content(&content_box);

    let update: Rc<dyn Fn(Option<&str>)> = {
        let button = button.clone();
        Rc::new(move |value: Option<&str>| {
            let token = if button.is_toggled() {
                "m3onPrimary"
            } else {
                "colOnLayer1"
            };
            if let Some(symbol) = &symbol {
                text::set_color(symbol, token);
            }
            if let Some(label) = &label {
                if let Some(value) = value {
                    label.set_text(value);
                }
                text::set_color(label, token);
            }
            let width = content.measure(gtk4::Orientation::Horizontal, -1).1 as f64 + 46.0;
            button.set_base_size(width, STATUS_HEIGHT);
            button.set_clicked_width(width + 6.0);
        })
    };
    update(None);
    (button, update)
}

pub struct Placeholder {
    pub widget: gtk4::Widget,
    visibility: Rc<Cell<Tween>>,
    is_ticking: Rc<Cell<bool>>,
    badge: Centered,
    shift: text::Shift,
}

impl Placeholder {
    pub fn new(theme: &SharedTheme, icon: &str, description: &str) -> Rc<Self> {
        Self::build(theme, icon, None, Some(description), Shape::Ghostish)
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
        let badge_overlay = gtk4::Overlay::new();
        badge_overlay.set_child(Some(&shape.area));
        badge_overlay.add_overlay(&Centered::new(&symbol));
        let badge = Centered::new(&badge_overlay);

        let column = Column::new(GAP, true);
        column.append(&badge);
        if let Some(title) = title {
            let heading_label = text::styled(title);
            text::set_font(
                &heading_label,
                text::Family::Title,
                pixel_size::LARGER as f64,
                "wght=550",
            );
            text::set_color(&heading_label, "m3outline");
            heading_label.set_justify(gtk4::Justification::Center);
            column.append(&heading_label);
        }
        if let Some(description) = description {
            let body_label = text::styled_sized(description, pixel_size::SMALL);
            text::set_color(&body_label, "m3outline");
            body_label.set_justify(gtk4::Justification::Center);
            body_label.set_wrap(true);
            column.append(&body_label);
        }
        let shift = text::Shift::new(&Centered::integral(&column));

        Rc::new(Placeholder {
            widget: shift.clone().upcast(),
            visibility: Rc::new(Cell::new(Tween::new(1.0, ENTER_MILLIS, EMPHASIZED_DECEL))),
            is_ticking: Rc::new(Cell::new(false)),
            badge,
            shift,
        })
    }

    pub fn show(&self, is_visible: bool) {
        let widget = &self.widget;
        let now = widget
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time);
        let mut tween = self.visibility.get();
        let target = if is_visible { 1.0 } else { 0.0 };
        if widget.is_mapped() {
            tween.retarget(target, now);
        } else {
            tween.jump(target);
        }
        self.visibility.set(tween);
        self.apply(tween.value(now));
        if self.is_ticking.replace(true) {
            return;
        }
        let (visibility, is_ticking, badge) = (
            self.visibility.clone(),
            self.is_ticking.clone(),
            self.badge.downgrade(),
        );
        widget.add_tick_callback(move |widget, clock| {
            let (Some(badge), Some(shift)) =
                (badge.upgrade(), widget.downcast_ref::<text::Shift>())
            else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            let tween = visibility.get();
            let opacity = tween.value(now);
            apply(widget, &badge, shift, opacity);
            if tween.is_running(now) {
                return glib::ControlFlow::Continue;
            }
            is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn apply(&self, opacity: f64) {
        apply(&self.widget, &self.badge, &self.shift, opacity);
    }
}

fn apply(widget: &gtk4::Widget, badge: &Centered, shift: &text::Shift, opacity: f64) {
    widget.set_opacity(opacity.clamp(0.0, 1.0));
    widget.set_visible(opacity > 0.0);
    shift.set_offset((-30.0 * (1.0 - opacity)) as f32);
    badge.set_rotation((-30.0 * (1.0 - opacity)) as f32);
}
