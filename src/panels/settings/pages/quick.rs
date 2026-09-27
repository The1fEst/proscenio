use gtk4::gdk;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::{config, tools};
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::services::session;
use crate::theming::switchwall;
use crate::ui::anim::{self, Fader};
use crate::ui::image;
use crate::ui::theme::{pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;

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
const FIELD_MARGIN: i32 = 8;
const PERCENT: f64 = 100.0;
const BOTTOM_BIT: i64 = 1;
const VERTICAL_BIT: i64 = 2;
const ENABLE_TRANSPARENCY: &str = "/appearance/transparency/enable";
const AUTOMATIC_TRANSPARENCY: &str = "/appearance/transparency/automatic";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let colors = page.section("format_paint", "Wallpaper & Colors");
    let top = Row::new(LAYOUT_SPACING);
    top.append(&preview(&page));
    top.append(&wallpaper_controls(&page));
    colors.append(&top);

    page.selection(
        &colors,
        vec![
            choice("Auto", "auto"),
            choice("Content", "scheme-content"),
            choice("Expressive", "scheme-expressive"),
            choice("Fidelity", "scheme-fidelity"),
            choice("Fruit Salad", "scheme-fruit-salad"),
            choice("Monochrome", "scheme-monochrome"),
            choice("Neutral", "scheme-neutral"),
            choice("Rainbow", "scheme-rainbow"),
            choice("Tonal Spot", "scheme-tonal-spot"),
        ],
        "/appearance/palette/type",
        Value::from("auto"),
        |value| {
            config::store_value("/appearance/palette/type", value);
            switchwall::detach(&["--noswitch"]);
        },
    );

    let accent = page.text_field(
        &colors,
        Style::Outlined,
        "Accent color (e.g. #8caaee, empty to use the wallpaper's)",
        || config::value_str("/appearance/palette/accentColor").unwrap_or_default(),
        |text| {
            config::store_value("/appearance/palette/accentColor", Value::from(text.trim()));
            switchwall::detach(&["--noswitch"]);
        },
    );
    accent.root.set_margin_start(FIELD_MARGIN);
    accent.root.set_margin_end(FIELD_MARGIN);
    page.refresh_text_on("/appearance/palette/accentColor", &accent);

    let tint = page.config_switch(
        &colors,
        "invert_colors",
        "Extra background tint",
        "/appearance/extraBackgroundTint",
        true,
    );
    page.tip(
        &tint.button,
        "Tints backgrounds of shell surfaces more strongly with the accent color",
    );
    page.config_switch(
        &colors,
        "ev_shadow",
        "Transparency",
        ENABLE_TRANSPARENCY,
        false,
    );
    let automatic = page.config_switch(
        &colors,
        "auto_awesome",
        "Automatic transparency values",
        AUTOMATIC_TRANSPARENCY,
        true,
    );
    page.tip(
        &automatic.button,
        "Derives the values below from the wallpaper instead of using them as entered",
    );
    let amounts = page.uniform_row(&colors);
    let (background_row, background) = page.config_spin_scaled(
        &amounts,
        "background_replace",
        "Background (%)",
        "/appearance/transparency/backgroundTransparency",
        0.11,
        PERCENT,
        (0, 100),
        1,
    );
    let (content_row, content) = page.config_spin_scaled(
        &amounts,
        "select_window",
        "Content (%)",
        "/appearance/transparency/contentTransparency",
        0.57,
        PERCENT,
        (0, 100),
        1,
    );
    page.tip(
        &content_row,
        "Affects how surfaces are layered even when transparency is off",
    );
    let follow = move || {
        let enabled = config::value_bool(ENABLE_TRANSPARENCY, false);
        let automatic = config::value_bool(AUTOMATIC_TRANSPARENCY, true);
        Page::set_spin_row_enabled(&background_row, &background, enabled && !automatic);
        Page::set_spin_row_enabled(&content_row, &content, !automatic);
    };
    follow();
    let follow = Rc::new(follow);
    for pointer in [ENABLE_TRANSPARENCY, AUTOMATIC_TRANSPARENCY] {
        let follow = follow.clone();
        page.watch(pointer, move || follow());
    }

    let screen = page.section("screenshot_monitor", "Bar & screen");
    let bar = page.row(&screen);
    let position = page.subsection(&bar, "Bar position", "");
    bar_position(&page, &position);
    let style = page.subsection(&bar, "Bar style", "");
    corner_style(&page, &style);

    let corners = page.row(&screen);
    let rounding = page.subsection(&corners, "Screen round corner", "");
    page.selection(
        &rounding,
        vec![
            icon_choice("No", "close", 0),
            icon_choice("Yes", "check", 1),
            icon_choice("When not fullscreen", "fullscreen_exit", 2),
        ],
        "/appearance/fakeScreenRounding",
        Value::from(2),
        |value| config::store_value("/appearance/fakeScreenRounding", value),
    );
    page
}

fn choice(label: &str, value: &str) -> Choice {
    Choice {
        label: label.to_owned(),
        icon: "",
        value: Value::from(value),
    }
}

pub fn bar_position(page: &Page, parent: &gtk4::Box) {
    page.selection_of(
        parent,
        vec![
            icon_choice("Top", "arrow_upward", 0),
            icon_choice("Left", "arrow_back", VERTICAL_BIT),
            icon_choice("Bottom", "arrow_downward", BOTTOM_BIT),
            icon_choice("Right", "arrow_forward", BOTTOM_BIT | VERTICAL_BIT),
        ],
        &["/bar/bottom", "/bar/vertical"],
        || {
            let bottom = config::value_bool("/bar/bottom", false);
            let vertical = config::value_bool("/bar/vertical", false);
            Value::from(
                if bottom { BOTTOM_BIT } else { 0 } | if vertical { VERTICAL_BIT } else { 0 },
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
            icon_choice("Hug", "line_curve", 0),
            icon_choice("Float", "page_header", 1),
            icon_choice("Rect", "toolbar", 2),
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

fn preview(page: &Page) -> Paint {
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

    let load = {
        let paint = paint.downgrade();
        move || {
            let path = config::value_str("/background/wallpaperPath").unwrap_or_default();
            let (paint, texture, fader) = (paint.clone(), texture.clone(), fader.clone());
            glib::spawn_future_local(async move {
                let loaded =
                    image::cover_texture(PathBuf::from(path), (PREVIEW_WIDTH, PREVIEW_HEIGHT))
                        .await;
                let Some(paint) = paint.upgrade() else {
                    return;
                };
                fader.show(loaded.is_some());
                texture.replace(loaded);
                paint.queue_draw();
            });
        }
    };
    load();
    page.watch("/background/wallpaperPath", load);
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
    let light = mode_button(page, false);
    let dark = mode_button(page, true);
    modes.append(&light.0);
    modes.append(&dark.0);
    column.append(&modes);

    let show = move |now_dark: bool| {
        for ((button, parts), is_dark) in [(&light, false), (&dark, true)] {
            let toggled = now_dark == is_dark;
            button.set_toggled(toggled);
            for part in parts {
                text::set_color(
                    part,
                    if toggled {
                        "colOnPrimary"
                    } else {
                        "colOnLayer2"
                    },
                );
            }
        }
    };
    show(session::dark_mode());
    if let Some(monitor) = session::watch_dark_mode(show) {
        page.keep(monitor);
    }
    column
}

pub fn choose_wallpaper(page: &Page) -> RippleButton {
    let choose = shortcut_button(
        page,
        "wallpaper",
        "Choose file",
        &["Ctrl", &super_key()],
        "T",
    );
    choose.connect_clicked(|_| switchwall::detach(&[]));
    let missing = tools::missing(&[&tools::KDIALOG]);
    if missing.is_empty() {
        page.tip(&choose, "Pick wallpaper image on your system");
    } else {
        choose.set_sensitive(false);
        page.tip(
            &choose,
            &tools::missing_message(&missing, "there is no file picker"),
        );
    }
    choose
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
    last: &str,
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
    content.append(&Centred::integral(&symbol));
    let main = Row::new(BUTTON_CONTENT_SPACING);
    main.set_hexpand(true);
    let name = text::styled(label);
    text::set_color(&name, "colOnSecondaryContainer");
    main.append(&Centred::new(&name));
    let keys = Row::new(KEY_SPACING);
    for modifier in modifiers {
        keys.append(&key(modifier));
    }
    keys.append(&Centred::new(&text::styled("+")));
    keys.append(&key(last));
    main.append(&keys);
    content.append(&main);
    button.set_content(&content, BUTTON_PADDING, 0);
    button
}

fn mode_button(page: &Page, dark: bool) -> (RippleButton, [gtk4::Label; 2]) {
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
        if dark { "dark_mode" } else { "light_mode" },
        MODE_ICON_SIZE,
    );
    let icon_box = Centred::integral(&icon);
    icon_box.set_halign(gtk4::Align::Center);
    column.append(&icon_box);
    let name = text::styled_sized(if dark { "Dark" } else { "Light" }, pixel_size::SMALLER);
    let name_box = Centred::new(&name);
    name_box.set_halign(gtk4::Align::Center);
    column.append(&name_box);
    button.set_content(&column, MODE_PADDING, MODE_PADDING);
    let mode = if dark { "dark" } else { "light" };
    button.connect_clicked(move |_| switchwall::detach(&["--mode", mode, "--noswitch"]));
    (button, [icon, name])
}

pub fn key(name: &str) -> gtk4::Widget {
    let label = text::styled(name);
    text::set_font(&label, text::Family::Monospace, KEY_TEXT_SIZE, "wght=450");
    let face = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    face.add_css_class("settings-key-face");
    face.append(&Centred::new(&label));
    let cap = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    cap.add_css_class("settings-key");
    cap.set_valign(gtk4::Align::Center);
    cap.append(&face);
    cap.upcast()
}
