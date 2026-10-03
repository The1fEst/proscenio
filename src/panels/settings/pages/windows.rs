use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::platform::{hypr, hyprconfig};
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::spinbox::SpinBox;

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

    let windows = page.section("", "");
    let corners = page.subsection(&windows, &tr("Corners"), "");
    hyprrows::spin(
        &page,
        &corners,
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
    let (shape, _) = hyprrows::spin(
        &page,
        &corners,
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
        &shape,
        &tr("2 is a circle, higher squares the corner off while keeping it smooth"),
    );

    let blur = page.subsection(&windows, &tr("Blur"), "");
    hyprrows::switch(
        &page,
        &blur,
        &options,
        "blur_on",
        &tr("Blur behind windows"),
        "decoration:blur:enabled",
    );
    let blur_row = page.row(&blur);
    let radius = hyprrows::spin(
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
    let passes = hyprrows::spin(
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
    let xray = hyprrows::switch(
        &page,
        &blur,
        &options,
        "hide_image",
        &tr("X-ray"),
        "decoration:blur:xray",
    );
    page.tip(
        &xray.button,
        &tr("A floating window blurs the wallpaper rather than the windows behind it"),
    );

    let opacity = page.subsection(&windows, &tr("Opacity"), "");
    let opacity_row = page.row(&opacity);
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
    let opaque = hyprrows::lines_switch(
        &page,
        &opacity,
        "fullscreen",
        &tr("Keep fullscreen windows opaque"),
        &hyprconfig::OPAQUE_FULLSCREEN,
    );
    page.tip(
        &opaque.button,
        &tr("Maximized windows too, whether focused or not"),
    );

    let borders = page.subsection(
        &windows,
        &tr("Borders"),
        &tr("The border keeps the color generated from the wallpaper"),
    );
    hyprrows::spin(
        &page,
        &borders,
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
    let border_row = page.row(&borders);
    for (label, border) in [
        ("Focused window (%)", &hyprconfig::ACTIVE_BORDER),
        ("Other windows (%)", &hyprconfig::INACTIVE_BORDER),
    ] {
        let spin = SpinBox::new(&page.theme, 0, 100, 5, 0);
        spin.set_value(
            (f64::from(hyprconfig::border_alpha(border)) * 100.0 / 255.0).round() as i64,
        );
        spin.connect_changed(move |percent| {
            let alpha = (percent as f64 * 255.0 / 100.0).round() as u8;
            if hyprconfig::border_alpha(border) == alpha {
                return;
            }
            let _ = hyprconfig::set_border_alpha(border, alpha);
            hypr::request("reload");
        });
        page.spin_row(&border_row, "opacity", &tr(label), &spin);
    }

    let shadows = page.subsection(&windows, &tr("Shadows"), "");
    hyprrows::switch(
        &page,
        &shadows,
        &options,
        "shadow",
        &tr("Drop shadows under windows"),
        "decoration:shadow:enabled",
    );
    let shadow_row = page.row(&shadows);
    let shadow_size = hyprrows::spin(
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
    let shadow_falloff = hyprrows::spin(
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
    let sharp = hyprrows::switch(
        &page,
        &shadows,
        &options,
        "crop_square",
        &tr("Sharp edge"),
        "decoration:shadow:sharp",
    );

    let dimming = page.subsection(&windows, &tr("Dimming"), "");
    hyprrows::switch(
        &page,
        &dimming,
        &options,
        "brightness_4",
        &tr("Dim windows out of focus"),
        "decoration:dim_inactive",
    );
    let undimmed = hyprrows::lines_switch(
        &page,
        &dimming,
        "fullscreen",
        &tr("Keep fullscreen windows undimmed"),
        &hyprconfig::UNDIMMED_FULLSCREEN,
    );
    page.tip(&undimmed.button, &tr("Maximized windows too"));
    let dim_strength = hyprrows::spin(
        &page,
        &dimming,
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
        &dimming,
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

    let blur_follows = {
        let options = Rc::downgrade(&options);
        move || {
            let Some(options) = options.upgrade() else {
                return;
            };
            let on = options.flag("decoration:blur:enabled");
            Page::set_spin_row_enabled(&radius.0, &radius.1, on);
            Page::set_spin_row_enabled(&passes.0, &passes.1, on);
            xray.set_enabled(on);
            let shadowed = options.flag("decoration:shadow:enabled");
            Page::set_spin_row_enabled(&shadow_size.0, &shadow_size.1, shadowed);
            Page::set_spin_row_enabled(&shadow_falloff.0, &shadow_falloff.1, shadowed);
            sharp.set_enabled(shadowed);
            let dimmed = options.flag("decoration:dim_inactive");
            Page::set_spin_row_enabled(&dim_strength.0, &dim_strength.1, dimmed);
            undimmed.set_enabled(dimmed);
        }
    };
    blur_follows();
    options.connect_changed(blur_follows);

    page.keep(options);
    page
}
