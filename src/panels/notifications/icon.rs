use gtk4::glib;
use gtk4::prelude::*;

use crate::services::notifications::Group;
use crate::ui::shapes::Shape;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::materialshape::MaterialShape;
use crate::ui::widgets::text;

const SIZE: i32 = 38;
const SYMBOL_SCALE: f64 = 0.57;
const APP_SCALE: f64 = 0.8;
const BADGE_SCALE: f64 = 0.49;
const URGENT_SHAPES: [Shape; 2] = [Shape::VerySunny, Shape::SoftBurst];

pub fn build(group: &Group, theme: &SharedTheme) -> gtk4::Widget {
    let urgent = group.urgent();
    let latest = group.notifications.last();
    let image = if group.notifications.len() > 1 {
        String::new()
    } else {
        group
            .notifications
            .first()
            .map(|entry| entry.image.clone())
            .unwrap_or_default()
    };

    let shape = if urgent {
        URGENT_SHAPES[glib::random_int_range(0, URGENT_SHAPES.len() as i32) as usize]
    } else {
        Shape::Circle
    };
    let shape = MaterialShape::new(
        theme,
        shape,
        SIZE,
        if urgent {
            |theme| theme.colors.col_primary_container
        } else {
            |theme| theme.colors.col_secondary_container
        },
    );

    let overlay = gtk4::Overlay::new();
    overlay.set_valign(gtk4::Align::Start);
    overlay.set_halign(gtk4::Align::Start);
    overlay.set_child(Some(&shape.area));

    if !image.is_empty() {
        overlay.add_overlay(&picture(&image));
        if !group.app_icon.is_empty() {
            let badge = app_icon(&group.app_icon);
            badge.set_pixel_size((SIZE as f64 * BADGE_SCALE).round() as i32);
            badge.set_halign(gtk4::Align::End);
            badge.set_valign(gtk4::Align::End);
            overlay.add_overlay(&badge);
        }
    } else if group.app_icon.starts_with('/') {
        overlay.add_overlay(&picture(&group.app_icon));
    } else if !group.app_icon.is_empty() {
        let icon = app_icon(&group.app_icon);
        icon.set_pixel_size((SIZE as f64 * APP_SCALE).round() as i32);
        overlay.add_overlay(&Centred::new(&icon));
    } else {
        let summary = latest.map(|entry| entry.summary.as_str()).unwrap_or("");
        let symbol = text::symbol(glyph(summary, urgent), SIZE as f64 * SYMBOL_SCALE);
        text::set_color(
            &symbol,
            if urgent {
                "colOnPrimaryContainer"
            } else {
                "colOnSecondaryContainer"
            },
        );
        overlay.add_overlay(&Centred::new(&symbol));
    }

    overlay.upcast()
}

fn app_icon(icon: &str) -> gtk4::Image {
    if icon.starts_with('/') {
        gtk4::Image::from_file(icon)
    } else {
        gtk4::Image::from_icon_name(icon)
    }
}

fn picture(image: &str) -> gtk4::Widget {
    let frame = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    frame.add_css_class("notif-image");
    frame.set_overflow(gtk4::Overflow::Hidden);
    frame.set_size_request(SIZE, SIZE);
    if image.starts_with('/') {
        let picture = gtk4::Picture::for_filename(image);
        picture.set_content_fit(gtk4::ContentFit::Cover);
        picture.set_hexpand(true);
        frame.append(&picture);
    } else {
        let icon = gtk4::Image::from_icon_name(image);
        icon.set_pixel_size(SIZE);
        frame.append(&icon);
    }
    frame.upcast()
}

pub fn glyph(summary: &str, urgent: bool) -> &'static str {
    let guessed = suitable(summary);
    if urgent && guessed == "chat" {
        return "priority_high";
    }
    guessed
}

fn suitable(summary: &str) -> &'static str {
    let summary = summary.to_lowercase();
    if summary.is_empty() {
        return "chat";
    }
    if summary.starts_with("file") {
        return "folder_copy";
    }
    const KEYWORDS: &[(&str, &str)] = &[
        ("reboot", "restart_alt"),
        ("record", "screen_record"),
        ("battery", "power"),
        ("power", "power"),
        ("screenshot", "screenshot_monitor"),
        ("welcome", "waving_hand"),
        ("time", "scheduleb"),
        ("installed", "download"),
        ("configuration reloaded", "reset_wrench"),
        ("unable", "question_mark"),
        ("couldn't", "question_mark"),
        ("config", "reset_wrench"),
        ("update", "update"),
        ("ai response", "neurology"),
        ("control", "settings"),
        ("upsca", "compare"),
        ("music", "queue_music"),
        ("install", "deployed_code_update"),
        ("input", "keyboard_alt"),
        ("preedit", "keyboard_alt"),
    ];
    for (keyword, symbol) in KEYWORDS {
        if summary.contains(keyword) {
            return symbol;
        }
    }
    "chat"
}
