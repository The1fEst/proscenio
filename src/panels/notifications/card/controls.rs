use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use super::ACTION_HEIGHT;
use crate::core::i18n::tr;
use crate::services::overlay::notifications::{Notification, Notifications, Reply};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;

const ACTION_PADDING: i32 = 15;
const COPY_RESET: Duration = Duration::from_millis(1500);

pub(super) fn actions(
    notification: &Notification,
    notifications: &Notifications,
    theme: &SharedTheme,
    dismiss: Rc<dyn Fn(bool)>,
) -> gtk4::Widget {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    let has_actions = !notification.actions.is_empty();
    row.set_homogeneous(!has_actions);
    if has_actions {
        row.set_halign(gtk4::Align::Start);
    }
    let is_urgent = notification.urgency == 2;

    let close_button = action_button(theme, None, Some("close"), is_urgent);
    close_button.connect_clicked(move |_| dismiss(false));
    row.append(&close_button);

    for (identifier, label) in &notification.actions {
        let button = action_button(theme, Some(label), None, is_urgent);
        button.connect_clicked({
            let notifications = notifications.clone();
            let id = notification.id;
            let identifier = identifier.clone();
            move |_| notifications.invoke(id, &identifier)
        });
        row.append(&button);
    }

    let copy_button = action_button(theme, None, Some("content_copy"), is_urgent);
    copy_button.connect_clicked({
        let body = notification.body.clone();
        move |button| {
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&body);
            }
            let Some(icon) = button
                .child()
                .and_then(|content_box| content_box.first_child())
                .and_downcast::<gtk4::Label>()
            else {
                return;
            };
            icon.set_text("inventory");
            glib::timeout_add_local_once(COPY_RESET, move || icon.set_text("content_copy"));
        }
    });
    row.append(&copy_button);

    row.upcast()
}

pub(super) fn reply_field(
    notification: &Notification,
    reply: &Reply,
    notifications: &Notifications,
    theme: &SharedTheme,
) -> gtk4::Box {
    let id = notification.id;
    let entry = gtk4::Entry::new();
    entry.add_css_class("notif-reply");
    entry.set_hexpand(true);
    let placeholder = [&reply.placeholder, &reply.label]
        .into_iter()
        .find(|candidate| !candidate.is_empty())
        .cloned()
        .unwrap_or_else(|| tr("Reply"));
    entry.set_placeholder_text(Some(&placeholder));
    let send_button = action_button(theme, None, Some("send"), notification.urgency == 2);

    let field = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    field.append(&entry);
    field.append(&send_button);

    let submit: Rc<dyn Fn()> = {
        let entry = entry.downgrade();
        let notifications = notifications.clone();
        Rc::new(move || {
            let Some(entry) = entry.upgrade() else {
                return;
            };
            let text = entry.text();
            let text = text.trim();
            if !text.is_empty() {
                notifications.reply(id, text);
            }
        })
    };
    entry.connect_activate({
        let submit = submit.clone();
        move |_| submit()
    });
    send_button.connect_clicked(move |_| submit());
    entry.connect_changed({
        let notifications = notifications.clone();
        move |entry| {
            if !entry.is_mapped() {
                return;
            }
            let text = entry.text();
            if text.is_empty() {
                notifications.drop_draft(id);
            } else {
                notifications.set_draft(id, &text);
            }
        }
    });

    let keyboard_window: Rc<Held> = Rc::default();
    let focus_gesture = gtk4::GestureClick::new();
    focus_gesture.set_propagation_phase(gtk4::PropagationPhase::Capture);
    focus_gesture.connect_pressed({
        let keyboard_window = Rc::downgrade(&keyboard_window);
        move |gesture, _, _, _| {
            let Some(keyboard_window) = keyboard_window.upgrade() else {
                return;
            };
            let window = gesture
                .widget()
                .and_then(|entry| entry.root())
                .and_downcast::<gtk4::Window>()
                .filter(|window| {
                    window.is_layer_window() && window.keyboard_mode() == KeyboardMode::None
                });
            if let Some(window) = window {
                window.set_keyboard_mode(KeyboardMode::Exclusive);
                keyboard_window.replace(Some(window.downgrade()));
            }
        }
    });
    entry.add_controller(focus_gesture);

    let key_controller = gtk4::EventControllerKey::new();
    key_controller.connect_key_pressed({
        let entry = entry.downgrade();
        let keyboard_window = Rc::downgrade(&keyboard_window);
        move |_, key, _, _| {
            let Some(entry) = entry.upgrade().filter(|_| key == gdk::Key::Escape) else {
                return glib::Propagation::Proceed;
            };
            let had_text = !entry.text().is_empty();
            entry.set_text("");
            let was_released = keyboard_window
                .upgrade()
                .is_some_and(|keyboard_window| release_keyboard(&keyboard_window));
            if had_text || was_released {
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        }
    });
    entry.add_controller(key_controller);

    if let Some(text) = notifications.draft(id) {
        entry.set_text(&text);
    }
    let entry = entry.downgrade();
    field.connect_map(move |_| {
        if let Some(entry) = entry.upgrade() {
            entry.grab_focus_without_selecting();
            entry.set_position(-1);
        }
    });
    field.connect_unmap(move |_| {
        release_keyboard(&keyboard_window);
    });
    field
}

type Held = RefCell<Option<glib::WeakRef<gtk4::Window>>>;

fn release_keyboard(keyboard_window: &Held) -> bool {
    let Some(window) = keyboard_window.take().and_then(|window| window.upgrade()) else {
        return false;
    };
    let was_raised = window.keyboard_mode() != KeyboardMode::None;
    window.set_keyboard_mode(KeyboardMode::None);
    was_raised
}

fn action_button(
    theme: &SharedTheme,
    text: Option<&str>,
    icon: Option<&str>,
    is_urgent: bool,
) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_size_request(-1, ACTION_HEIGHT);
    button.set_look(if is_urgent {
        Look {
            background: |theme| theme.colors.col_secondary_container,
            hover: |theme| theme.colors.col_secondary_container_hover,
            ripple: |theme| theme.colors.col_secondary_container_active,
            ..Look::default()
        }
    } else {
        Look {
            background: |theme| theme.colors.col_layer4,
            hover: |theme| theme.colors.col_layer4_hover,
            ripple: |theme| theme.colors.col_layer4_active,
            ..Look::default()
        }
    });
    let color = if is_urgent {
        "m3onSurfaceVariant"
    } else {
        "m3onSurface"
    };
    let label = match (text, icon) {
        (Some(text), _) => {
            let label = text::styled(text);
            label.set_ellipsize(pango::EllipsizeMode::End);
            label
        }
        _ => text::symbol(icon.unwrap_or_default(), pixel_size::LARGER as f64),
    };
    text::set_color(&label, color);
    button.set_content(&Centered::new(&label), ACTION_PADDING, 0);
    button
}
