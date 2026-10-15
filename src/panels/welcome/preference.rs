use gtk4::gdk::RGBA;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::Page;
use crate::services::session;
use crate::theming::switchwall;
use crate::ui::theme::{SharedTheme, Theme, mix, queue_draw_all, rounding};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::progress::{Colors, ProgressBar};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;

const PADDING: i32 = 5;
const PREVIEW_WIDTH: i32 = 250;
const SKELETON_MARGIN: i32 = 10;
const SKELETON_SPACING: i32 = 10;
const AVATAR: i32 = 50;
const LINE_SPACING: i32 = 4;
const TITLE_LINE: i32 = 22;
const SUBTITLE_LINE: i32 = 18;
const SUBTITLE_SHORTER: i32 = 45;
const BAR_MARGIN: i32 = 5;
const BUTTON_ROW_SPACING: i32 = 2;
const BUTTON_ROW: i32 = 30;
const CHECK_SIZE: f64 = 20.0;
const DARK_BASE: RGBA = rgb(0x3f, 0x38, 0x38);
const LIGHT_BASE: RGBA = rgb(0xf7, 0xf9, 0xff);
const LIGHT_INK: RGBA = rgb(0x29, 0x29, 0x29);

const fn rgb(red: u8, green: u8, blue: u8) -> RGBA {
    RGBA::new(
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
        1.0,
    )
}

type Shade = Rc<dyn Fn(&Theme) -> RGBA>;

pub fn build(page: &Page, previews_dark_mode: bool) -> RippleButton {
    let theme = page.theme.clone();
    let is_toggled = move |theme: &Theme| theme.m3.is_dark_mode == previews_dark_mode;
    let background: Shade =
        Rc::new(move |theme: &Theme| preview_background(theme, previews_dark_mode));
    let foreground: Shade =
        Rc::new(move |theme: &Theme| preview_foreground(theme, previews_dark_mode));

    let button = RippleButton::new(&theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_look(Look {
        background: |theme| theme.colors.col_layer2,
        ..Look::default()
    });
    button.set_hexpand(true);

    let skeleton = gtk4::Box::new(gtk4::Orientation::Vertical, SKELETON_SPACING);
    skeleton.set_margin_top(SKELETON_MARGIN);
    skeleton.set_margin_bottom(SKELETON_MARGIN);
    skeleton.set_margin_start(SKELETON_MARGIN);
    skeleton.set_margin_end(SKELETON_MARGIN);

    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, 5);
    header.append(&block(
        &theme,
        AVATAR,
        AVATAR,
        Corners::all(rounding::FULL),
        &foreground,
    ));
    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, LINE_SPACING);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let title_block = block(
        &theme,
        -1,
        TITLE_LINE,
        Corners::all(rounding::UNSHARPENMORE),
        &foreground,
    );
    lines.append(&title_block);
    let subtitle_block = block(
        &theme,
        -1,
        SUBTITLE_LINE,
        Corners::all(rounding::UNSHARPENMORE),
        &foreground,
    );
    subtitle_block.set_margin_end(SUBTITLE_SHORTER);
    lines.append(&subtitle_block);
    header.append(&lines);
    skeleton.append(&header);

    let progress_bar = ProgressBar::new();
    progress_bar.set_margin_top(BAR_MARGIN);
    progress_bar.set_margin_bottom(BAR_MARGIN);
    progress_bar.set_hexpand(true);
    progress_bar.set_wavy(true);
    progress_bar.set_value(0.7);
    skeleton.append(&progress_bar);

    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_ROW_SPACING);
    buttons.set_homogeneous(true);
    let accent_shade: Shade = {
        let foreground = foreground.clone();
        Rc::new(move |theme: &Theme| {
            if is_toggled(theme) {
                theme.m3.primary
            } else {
                foreground(theme)
            }
        })
    };
    let secondary_shade: Shade = {
        let foreground = foreground.clone();
        Rc::new(move |theme: &Theme| {
            if is_toggled(theme) {
                theme.m3.secondary_container
            } else {
                foreground(theme)
            }
        })
    };
    let accent_overlay = gtk4::Overlay::new();
    accent_overlay.set_child(Some(&block(
        &theme,
        -1,
        BUTTON_ROW,
        Corners::all(rounding::FULL),
        &accent_shade,
    )));
    let check_symbol = text::symbol("check", CHECK_SIZE);
    text::set_color(&check_symbol, "m3onPrimary");
    let check_box = Centered::new(&check_symbol);
    accent_overlay.add_overlay(&check_box);
    buttons.append(&accent_overlay);
    buttons.append(&block(
        &theme,
        -1,
        BUTTON_ROW,
        Corners::all(rounding::UNSHARPENMORE),
        &secondary_shade,
    ));
    buttons.append(&block(
        &theme,
        -1,
        BUTTON_ROW,
        Corners {
            left: rounding::UNSHARPENMORE,
            right: rounding::FULL,
        },
        &secondary_shade,
    ));
    skeleton.append(&buttons);

    let preview = gtk4::Overlay::new();
    preview.set_halign(gtk4::Align::Center);
    preview.set_size_request(PREVIEW_WIDTH, -1);
    let frame = Paint::new({
        let theme = theme.clone();
        let background = background.clone();
        move |snapshot, width, height| {
            let theme = theme.borrow();
            let radius = (rounding::SMALL - PADDING) as f32;
            let shape =
                gsk::RoundedRect::from_rect(graphene::Rect::new(0.0, 0.0, width, height), radius);
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&background(&theme), shape.bounds());
            snapshot.pop();
            let edge_color = theme.m3.outline_variant;
            snapshot.append_border(&shape, &[1.0; 4], &[edge_color; 4]);
        }
    });
    preview.set_child(Some(&frame));
    preview.add_overlay(&skeleton);
    preview.set_measure_overlay(&skeleton, true);

    let name_label = text::styled(&tr(if previews_dark_mode { "Dark" } else { "Light" }));
    name_label.set_halign(gtk4::Align::Center);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 5);
    column.set_halign(gtk4::Align::Center);
    column.set_valign(gtk4::Align::Center);
    column.append(&preview);
    column.append(&Centered::new(&name_label));
    button.set_content(&column, PADDING, PADDING);
    let mode = if previews_dark_mode { "dark" } else { "light" };
    button.connect_clicked(move |_| switchwall::detach(&["--mode", mode, "--noswitch"]));

    let show = {
        let (button, progress_bar, check_box, name_label, theme) = (
            button.downgrade(),
            progress_bar.downgrade(),
            check_box.downgrade(),
            name_label.downgrade(),
            theme.clone(),
        );
        move || {
            let (Some(button), Some(progress_bar), Some(check_box), Some(name_label)) = (
                button.upgrade(),
                progress_bar.upgrade(),
                check_box.upgrade(),
                name_label.upgrade(),
            ) else {
                return;
            };
            let theme = theme.borrow();
            let is_on = is_toggled(&theme);
            button.set_toggled(is_on);
            check_box.set_visible(is_on);
            text::set_color(
                &name_label,
                if is_on { "m3onPrimary" } else { "colOnLayer2" },
            );
            let (fill, ink) = (
                preview_background(&theme, previews_dark_mode),
                preview_foreground(&theme, previews_dark_mode),
            );
            progress_bar.set_colors(Colors {
                highlight: if is_on { theme.m3.primary } else { ink },
                track: mix(fill, ink, 0.5),
            });
            progress_bar.set_wave_moving(is_on);
            queue_draw_all(button.upcast_ref());
        }
    };
    show();
    if let Some(monitor) = session::watch_dark_mode({
        let show = show.clone();
        move |_| show()
    }) {
        page.keep(monitor);
    }
    button
}

#[derive(Clone, Copy)]
struct Corners {
    left: i32,
    right: i32,
}

impl Corners {
    fn all(radius: i32) -> Self {
        Corners {
            left: radius,
            right: radius,
        }
    }
}

fn block(theme: &SharedTheme, width: i32, height: i32, corners: Corners, shade: &Shade) -> Paint {
    let paint = Paint::new({
        let theme = theme.clone();
        let shade = shade.clone();
        move |snapshot, width, height| {
            let limit = width.min(height) / 2.0;
            let left = (corners.left as f32).min(limit);
            let right = (corners.right as f32).min(limit);
            let bounds = graphene::Rect::new(0.0, 0.0, width, height);
            let shape = gsk::RoundedRect::new(
                bounds,
                graphene::Size::new(left, left),
                graphene::Size::new(right, right),
                graphene::Size::new(right, right),
                graphene::Size::new(left, left),
            );
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&shade(&theme.borrow()), &bounds);
            snapshot.pop();
        }
    });
    paint.set_size_request(width, height);
    if width < 0 {
        paint.set_hexpand(true);
    }
    paint
}

fn preview_background(theme: &Theme, previews_dark_mode: bool) -> RGBA {
    with_hue_of(
        if previews_dark_mode {
            DARK_BASE
        } else {
            LIGHT_BASE
        },
        theme.m3.primary,
    )
}

fn preview_foreground(theme: &Theme, previews_dark_mode: bool) -> RGBA {
    let background = preview_background(theme, previews_dark_mode);
    if previews_dark_mode {
        lighter(background, 2.2)
    } else {
        mix(background, LIGHT_INK, 0.85)
    }
}

fn to_hsv(color: RGBA) -> (f32, f32, f32) {
    let (red, green, blue) = (color.red(), color.green(), color.blue());
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let saturation = if max > 0.0 { delta / max } else { 0.0 };
    let hue = if delta <= 0.0 {
        0.0
    } else if max == red {
        ((green - blue) / delta).rem_euclid(6.0) / 6.0
    } else if max == green {
        ((blue - red) / delta + 2.0) / 6.0
    } else {
        ((red - green) / delta + 4.0) / 6.0
    };
    (hue, saturation, max)
}

fn from_hsv(hue: f32, saturation: f32, value: f32, alpha: f32) -> RGBA {
    let sector = (hue.rem_euclid(1.0)) * 6.0;
    let chroma = value * saturation;
    let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match sector as u32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let lift = value - chroma;
    RGBA::new(red + lift, green + lift, blue + lift, alpha)
}

fn with_hue_of(color: RGBA, source: RGBA) -> RGBA {
    let (_, saturation, value) = to_hsv(color);
    let (hue, _, _) = to_hsv(source);
    from_hsv(hue, saturation, value, color.alpha())
}

fn lighter(color: RGBA, factor: f32) -> RGBA {
    let (hue, mut saturation, mut value) = to_hsv(color);
    value *= factor;
    if value > 1.0 {
        saturation = (saturation - (value - 1.0)).max(0.0);
        value = 1.0;
    }
    from_hsv(hue, saturation, value, color.alpha())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(color: RGBA) -> [u8; 3] {
        [color.red(), color.green(), color.blue()].map(|part| (part * 255.0).round() as u8)
    }

    #[test]
    fn preview_colors_follow_qt_hue_and_lighter() {
        let primary = RGBA::new(0.8, 0.6, 0.9, 1.0);
        let background = with_hue_of(DARK_BASE, primary);
        assert_eq!(bytes(background), [0x3d, 0x38, 0x3f]);
        assert_eq!(bytes(lighter(background, 2.2)), [0x85, 0x7b, 0x8b]);
        assert_eq!(
            bytes(lighter(RGBA::new(0.9, 0.5, 0.5, 1.0), 2.2)),
            [255, 255, 255]
        );
    }
}
