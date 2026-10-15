use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::settings::content::{Choice, Context, Page};
use crate::services::session;
use crate::theming::switchwall;
use crate::ui::anim::{self, Fader};
use crate::ui::image;
use crate::ui::theme::{pixel_size, rounding};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

const LAYOUT_SPACING: i32 = 5;
const PREVIEW_WIDTH: i32 = 340;
const PREVIEW_HEIGHT: i32 = 200;
const PREVIEW_MASK_WIDTH: f32 = 360.0;
const PREVIEW_FADE: f64 = 400.0;
const BUTTON_HEIGHT: i32 = 35;
const BUTTON_PADDING: i32 = 10;
const BUTTON_CONTENT_SPACING: i32 = 10;
const KEY_SPACING: i32 = 3;
const KEY_TEXT_SIZE: f64 = 12.0;
const SUPER_FALLBACK: &str = "󰖳";
const MODE_PADDING: i32 = 5;
const MODE_ICON_SIZE: f64 = 30.0;
const BOTTOM_BIT: i64 = 1;
const VERTICAL_BIT: i64 = 2;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let colors_section = page.section("format_paint", &tr("Wallpaper & Colors"));
    let top_row = Row::new(LAYOUT_SPACING);
    top_row.append(&preview(&page));
    top_row.append(&wallpaper_controls(&page));
    colors_section.append(&top_row);
    super::desktop::appearance::colors::add_palette(&page, &colors_section);
    super::desktop::appearance::colors::add_shell_surfaces(&page, &colors_section);

    let screen_section = page.section("screenshot_monitor", &tr("Bar & screen"));
    let bar_row = page.row(&screen_section);
    let position_section = page.subsection(&bar_row, &tr("Bar position"), "");
    bar_position(&page, &position_section);
    let style_section = page.subsection(&bar_row, &tr("Bar style"), "");
    corner_style(&page, &style_section);

    let corners_row = page.row(&screen_section);
    let rounding_section = page.subsection(&corners_row, &tr("Screen round corner"), "");
    add_screen_rounding(&page, &rounding_section);
    page
}

pub fn add_screen_rounding(page: &Page, parent: &gtk4::Box) {
    page.selection(
        parent,
        vec![
            icon_choice(&tr("No"), "close", 0),
            icon_choice(&tr("Yes"), "check", 1),
            icon_choice(&tr("When not fullscreen"), "fullscreen_exit", 2),
        ],
        "/appearance/fakeScreenRounding",
        Value::from(2),
        |value| config::store_value("/appearance/fakeScreenRounding", value),
    );
}

pub fn bar_position(page: &Page, parent: &gtk4::Box) {
    page.selection_of(
        parent,
        vec![
            icon_choice(&tr("Top"), "arrow_upward", 0),
            icon_choice(&tr("Left"), "arrow_back", VERTICAL_BIT),
            icon_choice(&tr("Bottom"), "arrow_downward", BOTTOM_BIT),
            icon_choice(&tr("Right"), "arrow_forward", BOTTOM_BIT | VERTICAL_BIT),
        ],
        &["/bar/bottom", "/bar/vertical"],
        || {
            let is_bottom = config::value_bool("/bar/bottom", false);
            let is_vertical = config::value_bool("/bar/vertical", false);
            Value::from(
                if is_bottom { BOTTOM_BIT } else { 0 } | if is_vertical { VERTICAL_BIT } else { 0 },
            )
        },
        |value| {
            let bits = value.as_i64().unwrap_or(0);
            config::store_value("/bar/bottom", Value::Bool(bits & BOTTOM_BIT != 0));
            config::store_value("/bar/vertical", Value::Bool(bits & VERTICAL_BIT != 0));
        },
    );
}

pub fn corner_style(page: &Page, parent: &gtk4::Box) {
    page.selection(
        parent,
        vec![
            icon_choice(&tr("Hug"), "line_curve", 0),
            icon_choice(&tr("Float"), "page_header", 1),
            icon_choice(&tr("Rect"), "toolbar", 2),
        ],
        "/bar/cornerStyle",
        Value::from(0),
        |value| config::store_value("/bar/cornerStyle", value),
    );
}

fn icon_choice(label: &str, icon: &'static str, value: i64) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value: Value::from(value),
    }
}

pub(super) fn preview(page: &Page) -> Paint {
    let texture: Rc<RefCell<Option<gdk::Texture>>> = Rc::new(RefCell::new(None));
    let paint = Paint::new({
        let texture = texture.clone();
        move |snapshot, width, height| {
            let Some(texture) = texture.borrow().clone() else {
                return;
            };
            let bounds = graphene::Rect::new(0.0, 0.0, width, height);
            let radius = rounding::NORMAL as f32;
            let corner = graphene::Size::new(radius * width / PREVIEW_MASK_WIDTH, radius);
            snapshot.push_rounded_clip(&gsk::RoundedRect::new(
                bounds, corner, corner, corner, corner,
            ));
            snapshot.append_texture(&texture, &bounds);
            snapshot.pop();
        }
    });
    paint.set_size_request(PREVIEW_WIDTH, PREVIEW_HEIGHT);
    paint.set_valign(gtk4::Align::Center);
    let fader = Fader::new(&paint, false, PREVIEW_FADE, anim::EMPHASIZED_DECEL);
    paint.set_visible(true);
    page.keep(fader.clone());

    let load_preview = {
        let paint = paint.downgrade();
        move || {
            let path = config::value_str("/background/wallpaperPath").unwrap_or_default();
            let (paint, texture, fader) = (paint.clone(), texture.clone(), fader.clone());
            glib::spawn_future_local(async move {
                let new_texture =
                    image::cover_texture(PathBuf::from(path), (PREVIEW_WIDTH, PREVIEW_HEIGHT))
                        .await;
                let Some(paint) = paint.upgrade() else {
                    return;
                };
                fader.show(new_texture.is_some());
                texture.replace(new_texture);
                paint.queue_draw();
            });
        }
    };
    load_preview();
    page.watch("/background/wallpaperPath", load_preview);
    paint
}

fn wallpaper_controls(page: &Page) -> gtk4::Box {
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, LAYOUT_SPACING);
    column.set_hexpand(true);

    column.append(&choose_wallpaper(page));

    let modes = gtk4::Box::new(gtk4::Orientation::Horizontal, LAYOUT_SPACING);
    modes.set_homogeneous(true);
    modes.set_vexpand(true);
    modes.set_size_request(-1, PREVIEW_HEIGHT - BUTTON_HEIGHT - LAYOUT_SPACING);
    let light_mode = mode_button(page, false);
    let dark_mode = mode_button(page, true);
    modes.append(&light_mode.0);
    modes.append(&dark_mode.0);
    column.append(&modes);

    let show_mode = move |is_dark_now: bool| {
        for ((button, parts), is_dark) in [(&light_mode, false), (&dark_mode, true)] {
            let is_toggled = is_dark_now == is_dark;
            button.set_toggled(is_toggled);
            for part in parts {
                text::set_color(
                    part,
                    if is_toggled {
                        "colOnPrimary"
                    } else {
                        "colOnLayer2"
                    },
                );
            }
        }
    };
    show_mode(session::read_dark_mode());
    if let Some(monitor) = session::watch_dark_mode(show_mode) {
        page.keep(monitor);
    }
    column
}

pub fn choose_wallpaper(page: &Page) -> RippleButton {
    let choose_button = shortcut_button(
        page,
        "wallpaper",
        &tr("Choose file"),
        &["Ctrl", &super_key()],
        "T",
    );
    choose_button.connect_clicked(|_| switchwall::detach(&[]));
    let missing_tools = tools::find_missing(&[&tools::KDIALOG]);
    if missing_tools.is_empty() {
        page.tip(&choose_button, &tr("Pick wallpaper image on your system"));
    } else {
        choose_button.set_sensitive(false);
        page.tip(
            &choose_button,
            &tools::missing_message(&missing_tools, &tr("there is no file picker")),
        );
    }
    choose_button
}

pub fn super_key() -> String {
    config::value_str("/cheatsheet/superKey")
        .filter(|key| !key.is_empty())
        .unwrap_or_else(|| SUPER_FALLBACK.to_owned())
}

pub fn shortcut_button(
    page: &Page,
    icon: &str,
    label: &str,
    modifiers: &[&str],
    last_key: &str,
) -> RippleButton {
    let button = RippleButton::new(&page.theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_look(Look {
        background: |theme| theme.colors.col_layer2,
        ..Look::default()
    });
    button.set_size_request(-1, BUTTON_HEIGHT);
    let content = Row::new(LAYOUT_SPACING);
    let symbol = text::symbol_filled(icon, pixel_size::LARGER as f64, 1.0);
    text::set_color(&symbol, "colOnSecondaryContainer");
    content.append(&Centered::integral(&symbol));
    let main_row = Row::new(BUTTON_CONTENT_SPACING);
    main_row.set_hexpand(true);
    let name_label = text::styled(label);
    text::set_color(&name_label, "colOnSecondaryContainer");
    main_row.append(&Centered::new(&name_label));
    let keys = Row::new(KEY_SPACING);
    for modifier in modifiers {
        keys.append(&key(modifier));
    }
    keys.append(&Centered::new(&text::styled("+")));
    keys.append(&key(last_key));
    main_row.append(&keys);
    content.append(&main_row);
    button.set_content(&content, BUTTON_PADDING, 0);
    button
}

fn mode_button(page: &Page, is_dark: bool) -> (RippleButton, [gtk4::Label; 2]) {
    let button = RippleButton::new(&page.theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_look(Look {
        background: |theme| theme.colors.col_layer2,
        ..Look::default()
    });
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.set_halign(gtk4::Align::Center);
    column.set_valign(gtk4::Align::Center);
    column.set_hexpand(true);
    let icon = text::symbol(
        if is_dark { "dark_mode" } else { "light_mode" },
        MODE_ICON_SIZE,
    );
    let icon_box = Centered::integral(&icon);
    icon_box.set_halign(gtk4::Align::Center);
    column.append(&icon_box);
    let name_label = text::styled_sized(
        &if is_dark { tr("Dark") } else { tr("Light") },
        pixel_size::SMALLER,
    );
    let name_box = Centered::new(&name_label);
    name_box.set_halign(gtk4::Align::Center);
    column.append(&name_box);
    button.set_content(&column, MODE_PADDING, MODE_PADDING);
    let mode = if is_dark { "dark" } else { "light" };
    button.connect_clicked(move |_| switchwall::detach(&["--mode", mode, "--noswitch"]));
    (button, [icon, name_label])
}

pub fn key(name: &str) -> gtk4::Widget {
    let label = text::styled(name);
    text::set_font(&label, text::Family::Monospace, KEY_TEXT_SIZE, "wght=450");
    let face = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    face.add_css_class("settings-key-face");
    face.append(&Centered::new(&label));
    let cap = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    cap.add_css_class("settings-key");
    cap.set_valign(gtk4::Align::Center);
    cap.append(&face);
    cap.upcast()
}
