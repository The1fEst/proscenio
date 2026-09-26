use gtk4::cairo;
use gtk4::gdk::RGBA;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::services::notifications::Notifications;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::{reveal, text};

const GLYPH: f64 = 19.0;
const DOT: f64 = 8.0;
const DOT_MARGIN: f64 = 1.0;
const DOT_TOP: f64 = 3.0;
const COUNT_SIZE: f64 = 10.0;

pub fn build(
    notifications: &Notifications,
    theme: &SharedTheme,
    config: &Config,
    vertical: bool,
    scope: &Scope,
) -> gtk4::Widget {
    let area = gtk4::DrawingArea::new();
    area.add_css_class("notify-icon");
    let (width, height) = symbol_layout(&area.create_pango_context(), "notifications").pixel_size();
    area.set_content_width(width);
    area.set_content_height(height);
    area.set_valign(gtk4::Align::Center);

    let unread = Rc::new(Cell::new(0u32));
    let silent = Rc::new(Cell::new(false));
    let show_count = config.notifications_show_unread_count;

    area.set_draw_func({
        let theme = theme.clone();
        let unread = unread.clone();
        let silent = silent.clone();
        move |widget, cr, _, height| {
            let theme = theme.borrow();
            draw(
                cr,
                height,
                Reading {
                    unread: unread.get(),
                    silent: silent.get(),
                    show_count,
                },
                widget.color(),
                theme.colors.col_on_layer0,
                theme.colors.col_layer0,
            );
        }
    });

    let (holder, set) = if vertical {
        area.set_halign(gtk4::Align::Center);
        reveal::vertical(
            &area,
            false,
            crate::panels::bar::VERTICAL_INDICATOR_SPACING as f64,
        )
    } else {
        reveal::wrap(&area, false)
    };
    let refresh = {
        let notifications = notifications.clone();
        let area = area.clone();
        move || {
            unread.set(notifications.unread.get());
            silent.set(notifications.silent.get());
            set(silent.get() || unread.get() > 0);
            area.queue_draw();
        }
    };
    refresh();
    scope.keep(notifications.subscribe(refresh));

    holder.upcast()
}

struct Reading {
    unread: u32,
    silent: bool,
    show_count: bool,
}

fn draw(
    cr: &cairo::Context,
    height: i32,
    reading: Reading,
    ink: RGBA,
    badge: RGBA,
    on_badge: RGBA,
) {
    let layout = symbol_layout(
        &pangocairo::functions::create_context(cr),
        if reading.silent {
            "notifications_paused"
        } else {
            "notifications"
        },
    );
    let (glyph_width, glyph_height) = layout.pixel_size();
    let top = ((height as f64 - glyph_height as f64) / 2.0).round();

    set_source(cr, ink);
    cr.move_to(0.0, top);
    pangocairo::functions::show_layout(cr, &layout);

    if reading.silent || reading.unread == 0 {
        return;
    }

    let counter = pangocairo::functions::create_layout(cr);
    counter.set_font_description(Some(&text::font(
        text::Family::Main,
        COUNT_SIZE,
        "wght=450",
    )));
    counter.set_text(&reading.unread.to_string());
    let (count_width, count_height) = counter.pixel_size();

    let size = if reading.show_count {
        count_width.max(count_height) as f64
    } else {
        DOT
    };
    let right = glyph_width as f64 - if reading.show_count { 0.0 } else { DOT_MARGIN };
    let badge_top = top + if reading.show_count { 0.0 } else { DOT_TOP };

    set_source(cr, badge);
    cr.arc(
        right - size / 2.0,
        badge_top + size / 2.0,
        size / 2.0,
        0.0,
        2.0 * std::f64::consts::PI,
    );
    let _ = cr.fill();

    if !reading.show_count {
        return;
    }
    set_source(cr, on_badge);
    cr.move_to(
        right - size + ((size - count_width as f64) / 2.0).round(),
        badge_top + ((size - count_height as f64) / 2.0).round(),
    );
    pangocairo::functions::show_layout(cr, &counter);
}

fn symbol_layout(context: &pango::Context, name: &str) -> pango::Layout {
    let layout = pango::Layout::new(context);
    let mut symbol = pango::FontDescription::from_string("Material Symbols Rounded");
    symbol.set_absolute_size(GLYPH * pango::SCALE as f64);
    symbol.set_variations(Some(&format!("FILL=0,opsz={GLYPH},wght=400")));
    layout.set_font_description(Some(&symbol));
    layout.set_text(name);
    layout
}

fn set_source(cr: &cairo::Context, color: RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}
