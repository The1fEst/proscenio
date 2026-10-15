mod body;
mod controls;

use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use self::body::{body_image, body_images, body_label, body_text};
use self::controls::{actions, reply_field};
use crate::core::i18n::tr;
use crate::panels::notifications::icon;
use crate::services::overlay::notifications::{Group, Notification, Notifications};
use crate::ui::anim;
use crate::ui::theme::{SharedTheme, mix, pixel_size, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::fixedheight::FixedHeight;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::slide::{DragList, Slide};

const GROUP_OVERSHOOT: f64 = 20.0;
const ITEM_OVERSHOOT: f64 = 38.0 + 20.0;
const ACTION_HEIGHT: i32 = 34;
const COLLAPSED: i32 = 80;
const ENTRY_SPACING: i32 = 3;
const REPLY_ROOM: i32 = ACTION_HEIGHT + ENTRY_SPACING;
const PADDING: i32 = 10;

pub struct Placement {
    pub is_popup: bool,
    pub is_expanded: Rc<Cell<bool>>,
    pub group_drags: Rc<DragList>,
    pub is_hovered: Rc<Cell<bool>>,
}

pub fn build(
    group: &Group,
    notifications: &Notifications,
    theme: &SharedTheme,
    is_alive: &Rc<Cell<bool>>,
    placement: Placement,
) -> Slide {
    let Placement {
        is_popup,
        is_expanded,
        group_drags,
        is_hovered,
    } = placement;
    let has_many = group.notifications.len() > 1;
    let drags = DragList::new();

    let items = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    let head = top_row(group, has_many, theme);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    content.set_hexpand(true);
    content.append(&head.row);
    content.append(&items);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 10);
    row.set_margin_start(PADDING);
    row.set_margin_end(PADDING);
    row.set_margin_top(PADDING);
    row.set_margin_bottom(PADDING);
    row.set_valign(gtk4::Align::Start);
    row.append(&icon::build(group, theme));
    row.append(&content);

    let clip = FixedHeight::new(&row);

    let card = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    card.add_css_class("notif-card");
    if is_popup {
        card.add_css_class("popup");
    }
    card.set_overflow(gtk4::Overflow::Hidden);
    card.append(&clip);

    let height_motion = anim::Motion::new(&clip, 0.0, 200.0, anim::EXPRESSIVE_EFFECTS);
    let apply: Rc<dyn Fn(bool, bool)> = {
        let items = items.downgrade();
        let clip = clip.downgrade();
        let row = row.downgrade();
        let chevron = head.chevron.clone();
        let content = content.downgrade();
        let group = clone_group(group);
        let notifications = notifications.clone();
        let theme = theme.clone();
        let is_alive = is_alive.clone();
        let drags = drags.clone();
        Rc::new(move |is_open, should_animate| {
            let (Some(items), Some(clip), Some(row), Some(content)) = (
                items.upgrade(),
                clip.upgrade(),
                row.upgrade(),
                content.upgrade(),
            ) else {
                return;
            };
            chevron.rotate_to(if is_open { 180.0 } else { 0.0 });
            let height_before = clip.height() as f64;
            items.set_spacing(if is_open { 5 } else { 3 });
            let has_picture = group
                .notifications
                .last()
                .is_some_and(|entry| !entry.image.is_empty());
            content.set_spacing(
                match (is_open, group.notifications.len() > 1, has_picture) {
                    (true, true, true) => 35,
                    (true, true, false) => 5,
                    _ => 0,
                },
            );
            fill(
                &items,
                &group,
                &notifications,
                &theme,
                &is_alive,
                is_open,
                &drags,
            );
            let cap = (!is_open).then(|| collapsed_height(&group));
            if !should_animate {
                clip.follow(cap);
                return;
            }
            let height_after = measure(&clip, &row, cap);
            height_motion.jump(height_before);
            height_motion.to(height_after as f64);
            let height_motion = height_motion.clone();
            clip.add_tick_callback(move |widget, _| {
                let Some(clip) = widget.downcast_ref::<FixedHeight>() else {
                    return glib::ControlFlow::Break;
                };
                if height_motion.is_running() {
                    clip.set_height(height_motion.get().round() as i32);
                    return glib::ControlFlow::Continue;
                }
                clip.follow(cap);
                glib::ControlFlow::Break
            });
        })
    };
    apply(is_expanded.get(), false);

    let toggle: Rc<dyn Fn()> = {
        let is_expanded = is_expanded.clone();
        let apply = apply.clone();
        let is_alive = is_alive.clone();
        Rc::new(move || {
            if !is_alive.get() {
                return;
            }
            let is_open = !is_expanded.get();
            is_expanded.set(is_open);
            apply(is_open, !is_open);
        })
    };
    head.expand_button.connect_clicked({
        let toggle = toggle.clone();
        move |_| toggle()
    });
    head.expand_button.connect_alt({
        let toggle = toggle.clone();
        move |_| toggle()
    });

    let slide = Slide::new(&card);
    group_drags.push(&slide);

    let destroy: Rc<dyn Fn(bool)> = {
        let slide = slide.downgrade();
        let notifications = notifications.clone();
        let is_alive = is_alive.clone();
        let ids = group.ids();
        Rc::new(move |is_left| {
            let Some(slide) = slide.upgrade() else {
                return;
            };
            let notifications = notifications.clone();
            let is_alive = is_alive.clone();
            let ids = ids.clone();
            slide.slide_out(is_left, GROUP_OVERSHOOT, move || {
                if !is_alive.get() {
                    return;
                }
                for id in &ids {
                    notifications.dismiss(*id);
                }
            });
        })
    };
    group_drags.attach(
        &slide,
        &slide,
        {
            let is_expanded = is_expanded.clone();
            Rc::new(move || !is_expanded.get())
        },
        destroy.clone(),
    );

    let expand_gesture = gtk4::GestureClick::new();
    expand_gesture.set_button(gdk::BUTTON_SECONDARY);
    expand_gesture.connect_pressed(move |_, _, _, _| toggle());
    card.add_controller(expand_gesture);

    let sweep_gesture = gtk4::GestureClick::new();
    sweep_gesture.set_button(gdk::BUTTON_MIDDLE);
    sweep_gesture.connect_released(move |gesture, _, x, y| {
        let is_inside = gesture.widget().is_some_and(|card| {
            x >= 0.0 && y >= 0.0 && x < card.width() as f64 && y < card.height() as f64
        });
        if is_inside {
            destroy(false);
        }
    });
    card.add_controller(sweep_gesture);

    if is_popup {
        let hover_controller = gtk4::EventControllerMotion::new();
        hover_controller.connect_enter({
            let notifications = notifications.clone();
            let is_alive = is_alive.clone();
            let is_hovered = is_hovered.clone();
            let ids = group.ids();
            move |_, _, _| {
                if is_alive.get() {
                    notifications.hold(&ids);
                    is_hovered.set(true);
                }
            }
        });
        hover_controller.connect_leave({
            let notifications = notifications.clone();
            let ids = group.ids();
            move |_| {
                if is_hovered.replace(false) {
                    notifications.release(&ids);
                }
            }
        });
        card.add_controller(hover_controller);
    }

    slide
}

struct Head {
    row: gtk4::Box,
    chevron: Centered,
    expand_button: RippleButton,
}

fn measure(clip: &FixedHeight, row: &gtk4::Box, cap: Option<i32>) -> i32 {
    let width = clip.width();
    let natural = row
        .measure(
            gtk4::Orientation::Vertical,
            if width > 0 { width } else { -1 },
        )
        .1;
    cap.map_or(natural, |cap| natural.min(cap))
}

fn collapsed_height(group: &Group) -> i32 {
    let has_reply = group
        .notifications
        .last()
        .is_some_and(|newest| newest.reply.is_some());
    if has_reply {
        COLLAPSED + REPLY_ROOM
    } else {
        COLLAPSED
    }
}

fn clone_group(group: &Group) -> Group {
    Group {
        app_name: group.app_name.clone(),
        app_icon: group.app_icon.clone(),
        time_millis: group.time_millis,
        notifications: group.notifications.clone(),
    }
}

fn top_row(group: &Group, has_many: bool, theme: &SharedTheme) -> Head {
    let title = gtk4::Label::new(Some(&if has_many {
        group.app_name.clone()
    } else {
        group
            .notifications
            .first()
            .map(|entry| entry.summary.clone())
            .unwrap_or_default()
    }));
    title.add_css_class(if has_many {
        "notif-app"
    } else {
        "notif-summary"
    });
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(pango::EllipsizeMode::End);

    let time_label = gtk4::Label::new(Some(&format_time(group.time_millis)));
    time_label.add_css_class("notif-time");
    time_label.set_margin_end(10);

    let arrow = text::symbol("keyboard_arrow_down", pixel_size::NORMAL as f64);
    text::set_color(&arrow, "colOnLayer2");
    let chevron = Centered::new(&arrow);

    let content = Row::new(3);
    if has_many {
        let count_label =
            text::styled_sized(&group.notifications.len().to_string(), pixel_size::SMALLER);
        count_label.set_margin_start(4);
        content.append(&count_label);
    }
    content.append(&chevron);

    let expand_button = RippleButton::new(theme);
    expand_button.set_radius(rounding::FULL as f64);
    expand_button.set_look(Look {
        background: |theme| mix(theme.colors.col_layer2, theme.colors.col_layer2_hover, 0.5),
        hover: |theme| theme.colors.col_layer2_hover,
        ripple: |theme| theme.colors.col_layer2_active,
        ..Look::default()
    });
    let content_box = Centered::new(&content);
    expand_button.set_content(&content_box, 0, 0);
    let width = (content.measure(gtk4::Orientation::Horizontal, -1).1 + 10).max(30);
    expand_button.set_size_request(width, pixel_size::SMALLER + 8);
    expand_button.set_valign(gtk4::Align::Center);

    let texts = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    texts.set_hexpand(true);
    texts.append(&title);
    texts.append(&time_label);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.append(&texts);
    row.append(&expand_button);

    Head {
        row,
        chevron,
        expand_button,
    }
}

fn fill(
    items: &gtk4::Box,
    group: &Group,
    notifications: &Notifications,
    theme: &SharedTheme,
    is_alive: &Rc<Cell<bool>>,
    is_expanded: bool,
    drags: &Rc<DragList>,
) {
    while let Some(child) = items.first_child() {
        items.remove(&child);
    }

    let is_only = group.notifications.len() == 1;
    let newest_first: Vec<&Notification> = group.notifications.iter().rev().collect();
    let visible_count = if is_expanded {
        newest_first.len()
    } else {
        2.min(newest_first.len())
    };

    drags.clear();
    for (index, notification) in newest_first.iter().take(visible_count).enumerate() {
        let slide_slot: Rc<glib::WeakRef<Slide>> = Rc::new(glib::WeakRef::new());
        let dismiss: Rc<dyn Fn(bool)> = {
            let slide_slot = slide_slot.clone();
            let notifications = notifications.clone();
            let is_alive = is_alive.clone();
            let id = notification.id;
            Rc::new(move |is_left| {
                let Some(slide) = slide_slot.upgrade() else {
                    return;
                };
                let notifications = notifications.clone();
                let is_alive = is_alive.clone();
                slide.slide_out(is_left, ITEM_OVERSHOOT, move || {
                    if is_alive.get() {
                        notifications.dismiss(id);
                    }
                });
            })
        };
        let item = entry(
            notification,
            notifications,
            theme,
            Shape {
                is_expanded,
                is_only,
                is_newest: index == 0,
            },
            dismiss.clone(),
        );
        if !is_expanded && index == 1 && group.notifications.len() > 2 {
            item.set_opacity(0.5);
        }
        let slide = Slide::new(&item);
        drags.push(&slide);
        drags.attach(&slide, &slide, Rc::new(move || is_expanded), dismiss);
        slide_slot.set(Some(&slide));
        items.append(&slide);
    }
}

struct Shape {
    is_expanded: bool,
    is_only: bool,
    is_newest: bool,
}

fn entry(
    notification: &Notification,
    notifications: &Notifications,
    theme: &SharedTheme,
    shape: Shape,
    dismiss: Rc<dyn Fn(bool)>,
) -> gtk4::Widget {
    let Shape {
        is_expanded,
        is_only,
        is_newest,
    } = shape;
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, ENTRY_SPACING);
    column.add_css_class("notif-item");
    if is_expanded && !is_only {
        column.add_css_class("open");
        if notification.urgency == 2 {
            column.add_css_class("urgent");
        }
    }

    if !is_only {
        let summary_label = gtk4::Label::new(Some(&notification.summary));
        summary_label.add_css_class("notif-item-summary");
        summary_label.set_xalign(0.0);
        summary_label.set_ellipsize(pango::EllipsizeMode::End);
        if is_expanded {
            column.append(&summary_label);
        } else {
            let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
            summary_label.set_hexpand(false);
            line.append(&summary_label);
            line.append(&body_label(notification, false));
            column.append(&line);
        }
    } else if !is_expanded {
        column.append(&body_label(notification, false));
    }

    let field = notification
        .reply
        .as_ref()
        .filter(|_| is_expanded || is_newest)
        .map(|reply| reply_field(notification, reply, notifications, theme));
    if is_expanded {
        let details_box = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        details_box.append(&body_label(notification, true));
        for path in body_images(&body_text(&notification.body, &notification.app_name)) {
            details_box.append(&body_image(path));
        }
        details_box.append(&actions(notification, notifications, theme, dismiss));
        if let Some(field) = &field {
            details_box.append(field);
        }
        column.append(&details_box);
    } else if let Some(field) = &field {
        column.append(field);
    }

    column.upcast()
}

fn format_time(time_millis: i64) -> String {
    let now_millis = glib::real_time() / 1000;
    let difference_millis = now_millis - time_millis;
    if difference_millis < 60_000 {
        return tr("Now");
    }
    let Ok(then) = glib::DateTime::from_unix_local(time_millis / 1000) else {
        return String::new();
    };
    let Ok(today) = glib::DateTime::now_local() else {
        return String::new();
    };
    if then.day_of_year() == today.day_of_year() && then.year() == today.year() {
        let hours = difference_millis / 3_600_000;
        if hours > 0 {
            return format!("{hours}h");
        }
        return format!("{}m", difference_millis / 60_000);
    }
    if let Ok(yesterday) = today.add_days(-1)
        && then.day_of_year() == yesterday.day_of_year()
        && then.year() == yesterday.year()
    {
        return tr("Yesterday");
    }
    then.format("%B %d").map(Into::into).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{COLLAPSED, REPLY_ROOM, collapsed_height};
    use crate::services::overlay::notifications::sample_group;

    #[test]
    fn a_collapsed_card_makes_room_for_the_newest_notification_s_reply_field() {
        assert_eq!(collapsed_height(&sample_group(&[false])), COLLAPSED);
        assert_eq!(collapsed_height(&sample_group(&[true, false])), COLLAPSED);
        assert_eq!(
            collapsed_height(&sample_group(&[false, true])),
            COLLAPSED + REPLY_ROOM
        );
    }
}
