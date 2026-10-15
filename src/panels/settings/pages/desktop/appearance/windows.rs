use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::platform::hyprland::{hypr, hyprconfig};
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::ui::widgets::input::spinbox::SpinBox;

pub const OPTIONS: [&str; 16] = [
    "general:border_size",
    "decoration:rounding",
    "decoration:rounding_power",
    "decoration:blur:enabled",
    "decoration:blur:size",
    "decoration:blur:passes",
    "decoration:blur:xray",
    "decoration:active_opacity",
    "decoration:inactive_opacity",
    "decoration:shadow:enabled",
    "decoration:shadow:range",
    "decoration:shadow:render_power",
    "decoration:shadow:sharp",
    "decoration:dim_inactive",
    "decoration:dim_strength",
    "decoration:dim_special",
];

const BORDER_COLORS: [(&str, &str); 5] = [
    ("Outline", "outline"),
    ("Outline variant", "outline_variant"),
    ("Primary", "primary"),
    ("Secondary", "secondary"),
    ("Tertiary", "tertiary"),
];

fn spin(
    icon: &'static str,
    label: &'static str,
    option: &'static str,
    factor: f64,
    range: (i64, i64),
    decimals: u32,
) -> Spin {
    Spin {
        icon,
        label,
        option,
        factor,
        range,
        step: 1,
        decimals,
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(hyprconfig::Area::Appearance, &OPTIONS);

    let windows_section = page.section("", "");
    let corners_section = page.subsection(&windows_section, &tr("Corners"), "");
    hyprrows::spin(
        &page,
        &corners_section,
        &options,
        &spin(
            "rounded_corner",
            "Corner rounding",
            "decoration:rounding",
            1.0,
            (0, 40),
            0,
        ),
    );
    let (shape_row, _) = hyprrows::spin(
        &page,
        &corners_section,
        &options,
        &spin(
            "line_curve",
            "Corner shape",
            "decoration:rounding_power",
            10.0,
            (20, 100),
            1,
        ),
    );
    page.tip(
        &shape_row,
        &tr("2 is a circle, higher squares the corner off while keeping it smooth"),
    );

    let blur_section = page.subsection(&windows_section, &tr("Blur"), "");
    hyprrows::switch(
        &page,
        &blur_section,
        &options,
        "blur_on",
        &tr("Blur behind windows"),
        "decoration:blur:enabled",
    );
    let blur_row = page.row(&blur_section);
    let (radius_row, radius_spin) = hyprrows::spin(
        &page,
        &blur_row,
        &options,
        &spin(
            "blur_circular",
            "Radius",
            "decoration:blur:size",
            1.0,
            (1, 40),
            0,
        ),
    );
    let (passes_row, passes_spin) = hyprrows::spin(
        &page,
        &blur_row,
        &options,
        &spin(
            "layers",
            "Passes",
            "decoration:blur:passes",
            1.0,
            (1, 10),
            0,
        ),
    );
    let xray_switch = hyprrows::switch(
        &page,
        &blur_section,
        &options,
        "hide_image",
        &tr("X-ray"),
        "decoration:blur:xray",
    );
    page.tip(
        &xray_switch.button,
        &tr("A floating window blurs the wallpaper rather than the windows behind it"),
    );

    let opacity_section = page.subsection(&windows_section, &tr("Opacity"), "");
    let opacity_row = page.row(&opacity_section);
    for (label, option) in [
        ("Focused window (%)", "decoration:active_opacity"),
        ("Other windows (%)", "decoration:inactive_opacity"),
    ] {
        hyprrows::spin(
            &page,
            &opacity_row,
            &options,
            &spin("opacity", label, option, 100.0, (10, 100), 0),
        );
    }
    let opaque_switch = hyprrows::lines_switch(
        &page,
        &opacity_section,
        "fullscreen",
        &tr("Keep fullscreen windows opaque"),
        &hyprconfig::OPAQUE_FULLSCREEN,
    );
    page.tip(
        &opaque_switch.button,
        &tr("Maximized windows too, whether focused or not"),
    );

    let borders_section = page.subsection(
        &windows_section,
        &tr("Borders"),
        &tr("Border colors are taken from the palette generated from the wallpaper"),
    );
    hyprrows::spin(
        &page,
        &borders_section,
        &options,
        &spin(
            "border_outer",
            "Border width",
            "general:border_size",
            1.0,
            (0, 20),
            0,
        ),
    );
    for (label, border) in [
        ("Focused window", &hyprconfig::ACTIVE_BORDER),
        ("Other windows", &hyprconfig::INACTIVE_BORDER),
    ] {
        let color_combo = page.combo_row(&borders_section, "palette", &tr(label));
        let keys: Vec<&str> = std::iter::once(border.color)
            .chain(BORDER_COLORS.iter().map(|(_, key)| *key))
            .collect();
        let names: Vec<String> = std::iter::once("Default")
            .chain(BORDER_COLORS.iter().map(|(name, _)| *name))
            .map(tr)
            .collect();
        let current_key = hyprconfig::border_color(border).key;
        let index = keys.iter().position(|key| *key == current_key).unwrap_or(0);
        color_combo.set_items(&names, index as i32);
        color_combo.connect_activated({
            let color_combo = Rc::downgrade(&color_combo);
            move |index| {
                let mut color = hyprconfig::border_color(border);
                if color.key == keys[index] {
                    return;
                }
                color.key = keys[index].to_owned();
                let _ = hyprconfig::set_border_color(border, &color);
                hypr::request("reload");
                if let Some(color_combo) = color_combo.upgrade() {
                    color_combo.set_items(&names, index as i32);
                }
            }
        });
    }
    let border_row = page.row(&borders_section);
    for (label, border) in [
        ("Focused window (%)", &hyprconfig::ACTIVE_BORDER),
        ("Other windows (%)", &hyprconfig::INACTIVE_BORDER),
    ] {
        let alpha_spin = SpinBox::new(&page.theme, 0, 100, 5, 0);
        alpha_spin.set_value(
            (f64::from(hyprconfig::border_color(border).alpha) * 100.0 / 255.0).round() as i64,
        );
        alpha_spin.connect_changed(move |percent| {
            let mut color = hyprconfig::border_color(border);
            let alpha = (percent as f64 * 255.0 / 100.0).round() as u8;
            if color.alpha == alpha {
                return;
            }
            color.alpha = alpha;
            let _ = hyprconfig::set_border_color(border, &color);
            hypr::request("reload");
        });
        page.spin_row(&border_row, "opacity", &tr(label), &alpha_spin);
    }

    let shadows_section = page.subsection(&windows_section, &tr("Shadows"), "");
    hyprrows::switch(
        &page,
        &shadows_section,
        &options,
        "shadow",
        &tr("Drop shadows under windows"),
        "decoration:shadow:enabled",
    );
    let shadow_row = page.row(&shadows_section);
    let (shadow_size_row, shadow_size_spin) = hyprrows::spin(
        &page,
        &shadow_row,
        &options,
        &spin(
            "width",
            "Size (px)",
            "decoration:shadow:range",
            1.0,
            (0, 100),
            0,
        ),
    );
    let (shadow_falloff_row, shadow_falloff_spin) = hyprrows::spin(
        &page,
        &shadow_row,
        &options,
        &spin(
            "gradient",
            "Falloff",
            "decoration:shadow:render_power",
            1.0,
            (1, 4),
            0,
        ),
    );
    let sharp_edge_switch = hyprrows::switch(
        &page,
        &shadows_section,
        &options,
        "crop_square",
        &tr("Sharp edge"),
        "decoration:shadow:sharp",
    );

    let dimming_section = page.subsection(&windows_section, &tr("Dimming"), "");
    hyprrows::switch(
        &page,
        &dimming_section,
        &options,
        "brightness_4",
        &tr("Dim windows out of focus"),
        "decoration:dim_inactive",
    );
    let undimmed_switch = hyprrows::lines_switch(
        &page,
        &dimming_section,
        "fullscreen",
        &tr("Keep fullscreen windows undimmed"),
        &hyprconfig::NO_DIM_FULLSCREEN,
    );
    page.tip(&undimmed_switch.button, &tr("Maximized windows too"));
    let (dim_strength_row, dim_strength_spin) = hyprrows::spin(
        &page,
        &dimming_section,
        &options,
        &spin(
            "contrast",
            "Dim by (%)",
            "decoration:dim_strength",
            100.0,
            (0, 100),
            0,
        ),
    );
    hyprrows::spin(
        &page,
        &dimming_section,
        &options,
        &spin(
            "select_window_2",
            "Dim around the special workspace by (%)",
            "decoration:dim_special",
            100.0,
            (0, 100),
            0,
        ),
    );

    let update_dependent_controls = {
        let options = Rc::downgrade(&options);
        move || {
            let Some(options) = options.upgrade() else {
                return;
            };
            let is_blur_on = options.flag("decoration:blur:enabled");
            Page::set_spin_row_enabled(&radius_row, &radius_spin, is_blur_on);
            Page::set_spin_row_enabled(&passes_row, &passes_spin, is_blur_on);
            xray_switch.set_enabled(is_blur_on);
            let is_shadow_on = options.flag("decoration:shadow:enabled");
            Page::set_spin_row_enabled(&shadow_size_row, &shadow_size_spin, is_shadow_on);
            Page::set_spin_row_enabled(&shadow_falloff_row, &shadow_falloff_spin, is_shadow_on);
            sharp_edge_switch.set_enabled(is_shadow_on);
            let is_dim_on = options.flag("decoration:dim_inactive");
            Page::set_spin_row_enabled(&dim_strength_row, &dim_strength_spin, is_dim_on);
            undimmed_switch.set_enabled(is_dim_on);
        }
    };
    update_dependent_controls();
    options.connect_changed(update_dependent_controls);

    page.keep(options);
    page
}
