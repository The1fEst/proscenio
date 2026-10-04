use gtk4::gdk;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::tr;
use crate::panels::notifications::icon;
use crate::services::notifications::{Group, Notification, Notifications, Reply};
use crate::ui::anim;
use crate::ui::theme::{SharedTheme, mix, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::fixedheight::FixedHeight;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::row::Row;
use crate::ui::widgets::slide::{DragList, Slide};
use crate::ui::widgets::text;

const GROUP_OVERSHOOT: f64 = 20.0;
const ITEM_OVERSHOOT: f64 = 38.0 + 20.0;
const ACTION_HEIGHT: i32 = 34;
const ACTION_PADDING: i32 = 15;
const COLLAPSED: i32 = 80;
const PADDING: i32 = 10;
const COPY_RESET: Duration = Duration::from_millis(1500);
const BODY_IMAGE_WIDTH: i32 = 300;
const BODY_IMAGE_HEIGHT: i32 = 200;

pub struct Placement {
    pub popup: bool,
    pub expanded: Rc<Cell<bool>>,
    pub groups: Rc<DragList>,
}

pub fn build(
    group: &Group,
    notifications: &Notifications,
    theme: &SharedTheme,
    alive: &Rc<Cell<bool>>,
    placement: Placement,
) -> Slide {
    let Placement {
        popup,
        expanded,
        groups,
    } = placement;
    let many = group.notifications.len() > 1;
    let drags = DragList::new();

    let items = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    let head = top_row(group, many, theme);
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
    if popup {
        card.add_css_class("popup");
    }
    card.set_overflow(gtk4::Overflow::Hidden);
    card.append(&clip);

    let height = anim::Motion::new(&clip, 0.0, 200.0, anim::EXPRESSIVE_EFFECTS);
    let apply: Rc<dyn Fn(bool, bool)> = {
        let items = items.downgrade();
        let clip = clip.downgrade();
        let row = row.downgrade();
        let chevron = head.chevron.clone();
        let content = content.downgrade();
        let group = clone_group(group);
        let notifications = notifications.clone();
        let theme = theme.clone();
        let alive = alive.clone();
        let drags = drags.clone();
        Rc::new(move |open, animate| {
            let (Some(items), Some(clip), Some(row), Some(content)) = (
                items.upgrade(),
                clip.upgrade(),
                row.upgrade(),
                content.upgrade(),
            ) else {
                return;
            };
            chevron.rotate_to(if open { 180.0 } else { 0.0 });
            let before = clip.height() as f64;
            items.set_spacing(if open { 5 } else { 3 });
            let pictured = group
                .notifications
                .last()
                .is_some_and(|entry| !entry.image.is_empty());
            content.set_spacing(match (open, group.notifications.len() > 1, pictured) {
                (true, true, true) => 35,
                (true, true, false) => 5,
                _ => 0,
            });
            fill(&items, &group, &notifications, &theme, &alive, open, &drags);
            let cap = (!open).then_some(COLLAPSED);
            if !animate {
                clip.follow(cap);
                return;
            }
            let after = measure(&clip, &row, open);
            height.jump(before);
            height.to(after as f64);
            let height = height.clone();
            clip.add_tick_callback(move |widget, _| {
                let Some(clip) = widget.downcast_ref::<FixedHeight>() else {
                    return glib::ControlFlow::Break;
                };
                if height.running() {
                    clip.set_height(height.get().round() as i32);
                    return glib::ControlFlow::Continue;
                }
                clip.follow(cap);
                glib::ControlFlow::Break
            });
        })
    };
    apply(expanded.get(), false);

    let toggle: Rc<dyn Fn()> = {
        let expanded = expanded.clone();
        let apply = apply.clone();
        let alive = alive.clone();
        Rc::new(move || {
            if !alive.get() {
                return;
            }
            let open = !expanded.get();
            expanded.set(open);
            apply(open, !open);
        })
    };
    head.expand.connect_clicked({
        let toggle = toggle.clone();
        move |_| toggle()
    });
    head.expand.connect_alt({
        let toggle = toggle.clone();
        move |_| toggle()
    });

    let slide = Slide::new(&card);
    groups.push(&slide);

    let destroy: Rc<dyn Fn(bool)> = {
        let slide = slide.downgrade();
        let notifications = notifications.clone();
        let alive = alive.clone();
        let ids = group.ids();
        Rc::new(move |left| {
            let Some(slide) = slide.upgrade() else {
                return;
            };
            let notifications = notifications.clone();
            let alive = alive.clone();
            let ids = ids.clone();
            slide.slide_out(left, GROUP_OVERSHOOT, move || {
                if !alive.get() {
                    return;
                }
                for id in &ids {
                    notifications.dismiss(*id);
                }
            });
        })
    };
    groups.attach(
        &slide,
        &slide,
        {
            let expanded = expanded.clone();
            Rc::new(move || !expanded.get())
        },
        destroy.clone(),
    );

    let expand = gtk4::GestureClick::new();
    expand.set_button(gdk::BUTTON_SECONDARY);
    expand.connect_pressed(move |_, _, _, _| toggle());
    card.add_controller(expand);

    let sweep = gtk4::GestureClick::new();
    sweep.set_button(gdk::BUTTON_MIDDLE);
    sweep.connect_released(move |gesture, _, x, y| {
        let inside = gesture.widget().is_some_and(|card| {
            x >= 0.0 && y >= 0.0 && x < card.width() as f64 && y < card.height() as f64
        });
        if inside {
            destroy(false);
        }
    });
    card.add_controller(sweep);

    if popup {
        let hover = gtk4::EventControllerMotion::new();
        hover.connect_enter({
            let notifications = notifications.clone();
            let alive = alive.clone();
            let ids = group.ids();
            move |_, _, _| {
                if alive.get() {
                    notifications.hold(&ids);
                }
            }
        });
        hover.connect_leave({
            let notifications = notifications.clone();
            let alive = alive.clone();
            let ids = group.ids();
            move |_| {
                if alive.get() {
                    notifications.release(&ids);
                }
            }
        });
        card.add_controller(hover);
    }

    slide
}

struct Head {
    row: gtk4::Box,
    chevron: Centred,
    expand: RippleButton,
}

fn measure(clip: &FixedHeight, row: &gtk4::Box, expanded: bool) -> i32 {
    let width = clip.width();
    let natural = row
        .measure(
            gtk4::Orientation::Vertical,
            if width > 0 { width } else { -1 },
        )
        .1;
    if expanded {
        natural
    } else {
        natural.min(COLLAPSED)
    }
}

fn clone_group(group: &Group) -> Group {
    Group {
        app_name: group.app_name.clone(),
        app_icon: group.app_icon.clone(),
        time: group.time,
        notifications: group.notifications.clone(),
    }
}

fn top_row(group: &Group, many: bool, theme: &SharedTheme) -> Head {
    let title = gtk4::Label::new(Some(&if many {
        group.app_name.clone()
    } else {
        group
            .notifications
            .first()
            .map(|entry| entry.summary.clone())
            .unwrap_or_default()
    }));
    title.add_css_class(if many { "notif-app" } else { "notif-summary" });
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(pango::EllipsizeMode::End);

    let time = gtk4::Label::new(Some(&friendly(group.time)));
    time.add_css_class("notif-time");
    time.set_margin_end(10);

    let arrow = text::symbol("keyboard_arrow_down", pixel_size::NORMAL as f64);
    text::set_color(&arrow, "colOnLayer2");
    let chevron = Centred::new(&arrow);

    let inside = Row::new(3);
    if many {
        let count = text::styled_sized(&group.notifications.len().to_string(), pixel_size::SMALLER);
        count.set_margin_start(4);
        inside.append(&count);
    }
    inside.append(&chevron);

    let expand = RippleButton::new(theme);
    expand.set_radius(rounding::FULL as f64);
    expand.set_look(Look {
        background: |theme| mix(theme.colors.col_layer2, theme.colors.col_layer2_hover, 0.5),
        hover: |theme| theme.colors.col_layer2_hover,
        ripple: |theme| theme.colors.col_layer2_active,
        ..Look::default()
    });
    let holder = Centred::new(&inside);
    expand.set_content(&holder, 0, 0);
    let width = (inside.measure(gtk4::Orientation::Horizontal, -1).1 + 10).max(30);
    expand.set_size_request(width, pixel_size::SMALLER + 8);
    expand.set_valign(gtk4::Align::Center);

    let texts = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    texts.set_hexpand(true);
    texts.append(&title);
    texts.append(&time);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.append(&texts);
    row.append(&expand);

    Head {
        row,
        chevron,
        expand,
    }
}

fn fill(
    items: &gtk4::Box,
    group: &Group,
    notifications: &Notifications,
    theme: &SharedTheme,
    alive: &Rc<Cell<bool>>,
    expanded: bool,
    drags: &Rc<DragList>,
) {
    while let Some(child) = items.first_child() {
        items.remove(&child);
    }

    let only = group.notifications.len() == 1;
    let newest: Vec<&Notification> = group.notifications.iter().rev().collect();
    let shown = if expanded {
        newest.len()
    } else {
        2.min(newest.len())
    };

    drags.clear();
    for (index, notification) in newest.iter().take(shown).enumerate() {
        let holder: Rc<glib::WeakRef<Slide>> = Rc::new(glib::WeakRef::new());
        let dismiss: Rc<dyn Fn(bool)> = {
            let holder = holder.clone();
            let notifications = notifications.clone();
            let alive = alive.clone();
            let id = notification.id;
            Rc::new(move |left| {
                let Some(slide) = holder.upgrade() else {
                    return;
                };
                let notifications = notifications.clone();
                let alive = alive.clone();
                slide.slide_out(left, ITEM_OVERSHOOT, move || {
                    if alive.get() {
                        notifications.dismiss(id);
                    }
                });
            })
        };
        let item = entry(
            notification,
            notifications,
            theme,
            expanded,
            only,
            dismiss.clone(),
        );
        if !expanded && index == 1 && group.notifications.len() > 2 {
            item.set_opacity(0.5);
        }
        let slide = Slide::new(&item);
        drags.push(&slide);
        drags.attach(&slide, &slide, Rc::new(move || expanded), dismiss);
        holder.set(Some(&slide));
        items.append(&slide);
    }
}

fn entry(
    notification: &Notification,
    notifications: &Notifications,
    theme: &SharedTheme,
    expanded: bool,
    only: bool,
    dismiss: Rc<dyn Fn(bool)>,
) -> gtk4::Widget {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 3);
    column.add_css_class("notif-item");
    if expanded && !only {
        column.add_css_class("open");
        if notification.urgency == 2 {
            column.add_css_class("urgent");
        }
    }

    if !only {
        let summary = gtk4::Label::new(Some(&notification.summary));
        summary.add_css_class("notif-item-summary");
        summary.set_xalign(0.0);
        summary.set_ellipsize(pango::EllipsizeMode::End);
        if expanded {
            column.append(&summary);
        } else {
            let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
            summary.set_hexpand(false);
            line.append(&summary);
            line.append(&body_label(notification, false));
            column.append(&line);
        }
    } else if !expanded {
        column.append(&body_label(notification, false));
    }

    if expanded {
        let open = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
        open.append(&body_label(notification, true));
        for path in body_images(&body_text(&notification.body, &notification.app_name)) {
            open.append(&body_image(path));
        }
        let field = notification
            .reply
            .as_ref()
            .map(|reply| reply_field(notification, reply, notifications, theme));
        open.append(&actions(
            notification,
            notifications,
            theme,
            dismiss,
            field.as_ref(),
        ));
        if let Some(field) = &field {
            open.append(field);
        }
        column.append(&open);
    }

    column.upcast()
}

fn body_label(notification: &Notification, wrap: bool) -> gtk4::Widget {
    let label = gtk4::Label::new(None);
    label.add_css_class("notif-body");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    let text = body_text(&notification.body, &notification.app_name);
    match markup(&text) {
        Some(markup) => label.set_markup(&markup),
        None => label.set_text(&text),
    }
    label.connect_activate_link(|_, address| {
        let _ =
            gtk4::gio::AppInfo::launch_default_for_uri(address, gtk4::gio::AppLaunchContext::NONE);
        glib::Propagation::Stop
    });
    if wrap {
        label.set_wrap(true);
        label.set_wrap_mode(pango::WrapMode::WordChar);
    } else {
        label.set_ellipsize(pango::EllipsizeMode::End);
        label.set_lines(1);
        label.set_single_line_mode(true);
    }
    label.upcast()
}

fn actions(
    notification: &Notification,
    notifications: &Notifications,
    theme: &SharedTheme,
    dismiss: Rc<dyn Fn(bool)>,
    field: Option<&gtk4::Box>,
) -> gtk4::Widget {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    let bare = notification.actions.is_empty() && field.is_none();
    row.set_homogeneous(bare);
    if !bare {
        row.set_halign(gtk4::Align::Start);
    }
    let urgent = notification.urgency == 2;

    let close = action_button(theme, None, Some("close"), urgent);
    close.connect_clicked(move |_| dismiss(false));
    row.append(&close);

    for (identifier, label) in &notification.actions {
        let button = action_button(theme, Some(label), None, urgent);
        button.connect_clicked({
            let notifications = notifications.clone();
            let id = notification.id;
            let identifier = identifier.clone();
            move |_| notifications.invoke(id, &identifier)
        });
        row.append(&button);
    }

    if let (Some(reply), Some(field)) = (&notification.reply, field) {
        let label = if reply.label.is_empty() {
            tr("Reply")
        } else {
            reply.label.clone()
        };
        let button = action_button(theme, Some(&label), None, urgent);
        button.connect_clicked({
            let field = field.downgrade();
            let notifications = notifications.clone();
            let id = notification.id;
            move |_| {
                let Some(field) = field.upgrade() else {
                    return;
                };
                if notifications.draft(id).is_none() {
                    notifications.set_draft(id, "");
                }
                field.set_visible(true);
            }
        });
        row.append(&button);
    }

    let copy = action_button(theme, None, Some("content_copy"), urgent);
    copy.connect_clicked({
        let body = notification.body.clone();
        move |button| {
            if let Some(display) = gdk::Display::default() {
                display.clipboard().set_text(&body);
            }
            let Some(icon) = button
                .child()
                .and_then(|holder| holder.first_child())
                .and_downcast::<gtk4::Label>()
            else {
                return;
            };
            icon.set_text("inventory");
            glib::timeout_add_local_once(COPY_RESET, move || icon.set_text("content_copy"));
        }
    });
    row.append(&copy);

    row.upcast()
}

fn reply_field(
    notification: &Notification,
    reply: &Reply,
    notifications: &Notifications,
    theme: &SharedTheme,
) -> gtk4::Box {
    let id = notification.id;
    let entry = gtk4::Entry::new();
    entry.add_css_class("notif-reply");
    entry.set_hexpand(true);
    entry.set_placeholder_text(Some(&if reply.placeholder.is_empty() {
        tr("Reply")
    } else {
        reply.placeholder.clone()
    }));
    let send = action_button(theme, None, Some("send"), notification.urgency == 2);

    let field = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    field.append(&entry);
    field.append(&send);
    field.set_visible(false);

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
    send.connect_clicked(move |_| submit());
    entry.connect_changed({
        let notifications = notifications.clone();
        move |entry| {
            if entry.is_mapped() {
                notifications.set_draft(id, &entry.text());
            }
        }
    });

    let keys = gtk4::EventControllerKey::new();
    keys.connect_key_pressed({
        let field = field.downgrade();
        let notifications = notifications.clone();
        move |_, key, _, _| {
            let Some(field) = field.upgrade().filter(|_| key == gdk::Key::Escape) else {
                return glib::Propagation::Proceed;
            };
            notifications.drop_draft(id);
            field.set_visible(false);
            glib::Propagation::Stop
        }
    });
    entry.add_controller(keys);

    let raised: Rc<RefCell<Option<glib::WeakRef<gtk4::Window>>>> = Rc::default();
    field.connect_map({
        let raised = raised.clone();
        let entry = entry.downgrade();
        move |field| {
            let window = field
                .root()
                .and_downcast::<gtk4::Window>()
                .filter(|window| {
                    window.is_layer_window() && window.keyboard_mode() == KeyboardMode::None
                });
            if let Some(window) = window {
                window.set_keyboard_mode(KeyboardMode::Exclusive);
                raised.replace(Some(window.downgrade()));
            }
            if let Some(entry) = entry.upgrade() {
                entry.grab_focus_without_selecting();
                entry.set_position(-1);
            }
        }
    });
    field.connect_unmap(move |_| {
        if let Some(window) = raised.take().and_then(|window| window.upgrade()) {
            window.set_keyboard_mode(KeyboardMode::None);
        }
    });

    if let Some(text) = notifications.draft(id) {
        entry.set_text(&text);
        field.set_visible(true);
    }
    field
}

fn action_button(
    theme: &SharedTheme,
    text: Option<&str>,
    icon: Option<&str>,
    urgent: bool,
) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_size_request(-1, ACTION_HEIGHT);
    button.set_look(if urgent {
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
    let colour = if urgent {
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
    text::set_color(&label, colour);
    button.set_content(&Centred::new(&label), ACTION_PADDING, 0);
    button
}

pub fn body_text(body: &str, app_name: &str) -> String {
    const CHROMIUM: &[&str] = &[
        "brave",
        "chrome",
        "chromium",
        "vivaldi",
        "opera",
        "microsoft edge",
    ];
    let lower = app_name.to_lowercase();
    if CHROMIUM.iter().any(|name| lower.contains(name)) {
        let parts: Vec<&str> = body.split("\n\n").collect();
        if parts.len() > 1 && parts[0].starts_with("<a") {
            return parts[1..].join("\n\n");
        }
    }
    body.to_owned()
}

fn markup(text: &str) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut checked = String::with_capacity(text.len());
    let mut linked = false;
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let plain = glib::markup_escape_text(&rest[..start]);
        out.push_str(&plain);
        checked.push_str(&plain);
        let Some(end) = rest[start..].find('>') else {
            let tail = glib::markup_escape_text(&rest[start..]);
            out.push_str(&tail);
            checked.push_str(&tail);
            return check(out, &checked);
        };
        let tag = &rest[start + 1..start + end];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        match name.as_str() {
            "b" | "i" | "u" | "s" => {
                let kept = format!("<{tag}>");
                out.push_str(&kept);
                checked.push_str(&kept);
            }
            "a" if closing && linked => {
                linked = false;
                out.push_str("</a>");
                checked.push_str("</span>");
            }
            "a" if !closing && !linked => {
                if let Some(address) = attribute(tag, "href") {
                    linked = true;
                    let address = glib::markup_escape_text(&unescape(&address));
                    out.push_str(&format!("<a href=\"{address}\">"));
                    checked.push_str("<span>");
                }
            }
            "img" if !closing => {
                if let Some(alt) = attribute(tag, "alt") {
                    let alt = glib::markup_escape_text(&unescape(&alt));
                    out.push_str(&alt);
                    checked.push_str(&alt);
                }
            }
            _ => {}
        }
        rest = &rest[start + end + 1..];
    }
    let tail = glib::markup_escape_text(rest);
    out.push_str(&tail);
    checked.push_str(&tail);
    if linked {
        out.push_str("</a>");
        checked.push_str("</span>");
    }
    check(out, &checked)
}

fn body_images(text: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        let tag = &rest[start + 1..start + end];
        let image = tag
            .split_whitespace()
            .next()
            .is_some_and(|name| name.eq_ignore_ascii_case("img"));
        if let Some(source) = attribute(tag, "src").filter(|_| image) {
            let source = unescape(&source);
            if source.starts_with("file:") {
                found.extend(gtk4::gio::File::for_uri(&source).path());
            } else if source.starts_with('/') {
                found.push(PathBuf::from(source));
            }
        }
        rest = &rest[start + end + 1..];
    }
    found
}

fn body_image(path: PathBuf) -> gtk4::Widget {
    let picture = gtk4::Picture::new();
    picture.add_css_class("notif-body-image");
    picture.set_overflow(gtk4::Overflow::Hidden);
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk4::ContentFit::ScaleDown);
    picture.set_halign(gtk4::Align::Start);
    picture.set_visible(false);
    let scale = gdk::Display::default()
        .map(|display| {
            display
                .monitors()
                .iter::<gdk::Monitor>()
                .flatten()
                .map(|monitor| monitor.scale_factor())
                .max()
                .unwrap_or(1)
        })
        .unwrap_or(1);
    let size = (BODY_IMAGE_WIDTH * scale, BODY_IMAGE_HEIGHT * scale);
    let target = picture.downgrade();
    glib::spawn_future_local(async move {
        let texture = crate::ui::image::texture(path, size).await;
        if let (Some(picture), Some(texture)) = (target.upgrade(), texture) {
            picture.set_paintable(Some(&texture));
            picture.set_visible(true);
        }
    });
    picture.upcast()
}

fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let mut rest = tag.split_once(char::is_whitespace)?.1;
    loop {
        let (name, after) = rest.split_once('=')?;
        let after = after.trim_start();
        let quote = after
            .chars()
            .next()
            .filter(|mark| matches!(mark, '"' | '\''))?;
        let (value, next) = after[1..].split_once(quote)?;
        if name.trim().eq_ignore_ascii_case(wanted) {
            return Some(value.to_owned());
        }
        rest = next;
    }
}

fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

fn check(markup: String, checked: &str) -> Option<String> {
    pango::parse_markup(checked, '\u{0}').ok().map(|_| markup)
}

#[cfg(test)]
mod tests {
    use super::{body_images, body_text, markup};
    use crate::services::notifications::CAPABILITIES;
    use std::path::PathBuf;

    #[test]
    fn chromium_notifications_lose_their_link_line() {
        let body = "<a href=\"https://example.invalid\">example.invalid</a>\n\nThe actual text";
        assert_eq!(body_text(body, "Brave"), "The actual text");
        assert_eq!(body_text(body, "Thunderbird"), body);
    }

    #[test]
    fn a_body_without_a_link_line_is_left_alone() {
        let body = "First paragraph\n\nSecond paragraph";
        assert_eq!(body_text(body, "chromium"), body);
    }

    #[test]
    fn markup_keeps_emphasis_and_links_and_drops_everything_else() {
        assert_eq!(
            markup("<b>bold</b> <a href=\"https://x.invalid\">link</a> <span>plain</span>"),
            Some("<b>bold</b> <a href=\"https://x.invalid\">link</a> plain".to_owned())
        );
    }

    #[test]
    fn a_link_keeps_only_its_address_escaped_once() {
        assert_eq!(
            markup("<a class='c' href='https://x.invalid/?a=1&amp;b=\"2\"'>link</a>"),
            Some("<a href=\"https://x.invalid/?a=1&amp;b=&quot;2&quot;\">link</a>".to_owned())
        );
    }

    #[test]
    fn a_link_without_an_address_becomes_its_text() {
        assert_eq!(
            markup("<a name=\"top\">link</a> text"),
            Some("link text".to_owned())
        );
    }

    #[test]
    fn links_and_images_are_advertised_only_if_the_body_keeps_them() {
        let kept = markup("<a href=\"x\">link</a>").unwrap_or_default();
        assert_eq!(
            CAPABILITIES.contains(&"body-hyperlinks"),
            kept.contains("<a")
        );
        assert_eq!(
            CAPABILITIES.contains(&"body-images"),
            !body_images("<img src=\"/tmp/picture.png\"/>").is_empty()
        );
    }

    #[test]
    fn an_image_reads_as_its_alt_text_in_the_line() {
        assert_eq!(
            markup("see <img src=\"/tmp/a.png\" alt=\"a cat\"/> and <img src=\"/tmp/b.png\"/>."),
            Some("see a cat and .".to_owned())
        );
    }

    #[test]
    fn body_images_are_local_files_only() {
        assert_eq!(
            body_images(
                "<img src=\"file:///tmp/a%20b.png\"/> <img alt='c' src='/tmp/c.png'> \
                 <img src=\"https://x.invalid/d.png\"/> <img src=\"e.png\"/>"
            ),
            vec![PathBuf::from("/tmp/a b.png"), PathBuf::from("/tmp/c.png")]
        );
    }

    #[test]
    fn markup_escapes_bare_ampersands() {
        assert_eq!(
            markup("tea & biscuits"),
            Some("tea &amp; biscuits".to_owned())
        );
    }
}

fn friendly(time: i64) -> String {
    let now = glib::real_time() / 1000;
    let difference = now - time;
    if difference < 60_000 {
        return tr("Now");
    }
    let Ok(then) = glib::DateTime::from_unix_local(time / 1000) else {
        return String::new();
    };
    let Ok(today) = glib::DateTime::now_local() else {
        return String::new();
    };
    if then.day_of_year() == today.day_of_year() && then.year() == today.year() {
        let hours = difference / 3_600_000;
        if hours > 0 {
            return format!("{hours}h");
        }
        return format!("{}m", difference / 60_000);
    }
    if let Ok(yesterday) = today.add_days(-1)
        && then.day_of_year() == yesterday.day_of_year()
        && then.year() == yesterday.year()
    {
        return tr("Yesterday");
    }
    then.format("%B %d").map(Into::into).unwrap_or_default()
}
