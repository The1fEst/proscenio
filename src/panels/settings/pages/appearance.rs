use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::{config, tools};
use crate::panels::settings::content::{Context, Page, Parent};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::platform::appearance::Parts;
use crate::platform::hyprconfig;
use crate::services::appearance::DesktopAppearance;
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::spinbox::SpinBox;
use crate::ui::widgets::text;

const OPTIONS: [&str; 16] = [
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
    "general:allow_tearing",
];
const FONT_LABEL_WIDTH: i32 = 110;
const FONT_LABEL_START: i32 = 2;
const FONT_SIZES: (i64, i64) = (5, 72);
const FONT_ROLES: [(&str, &[&str], &str); 6] = [
    ("general", &["main", "reading"], "General"),
    ("fixed", &["monospace"], "Fixed width"),
    ("title", &["title"], "Titles"),
    ("small", &[], "Small"),
    ("toolbar", &[], "Toolbar"),
    ("menu", &[], "Menu"),
];
const SHELL_FONTS: [(&str, &str, &str); 2] = [
    (
        "/appearance/fonts/iconNerd",
        "JetBrains Mono NF",
        "Nerd icons",
    ),
    (
        "/appearance/fonts/expressive",
        "Space Grotesk",
        "Expressive",
    ),
];
const BULK_SHELL_KEYS: [&str; 3] = ["main", "reading", "title"];

pub fn family_options(current: &str, families: Vec<String>) -> Vec<String> {
    if current.is_empty() || families.iter().any(|family| family == current) {
        return families;
    }
    std::iter::once(current.to_owned())
        .chain(families)
        .collect()
}

fn store_shell_fonts(keys: &[&str], family: &str) {
    for key in keys {
        config::store_value(&format!("/appearance/fonts/{key}"), Value::from(family));
    }
}

fn follow(
    page: &Page,
    appearance: &Rc<DesktopAppearance>,
    update: impl Fn(&DesktopAppearance) + 'static,
) {
    update(appearance);
    page.keep(appearance.watch({
        let appearance = Rc::downgrade(appearance);
        move || {
            if let Some(appearance) = appearance.upgrade() {
                update(&appearance);
            }
        }
    }));
}

fn font_label(parent: &impl Parent, name: &str) {
    let label = text::styled(name);
    text::set_color(&label, "colSubtext");
    label.set_xalign(0.0);
    let holder = Centred::filling_width(&label);
    holder.set_size_request(FONT_LABEL_WIDTH, -1);
    holder.set_margin_start(FONT_LABEL_START);
    parent.add(&holder);
}

fn size_spin(page: &Page, parent: &impl Parent) -> (gtk4::Box, Rc<SpinBox>) {
    let spin = SpinBox::new(&page.theme, FONT_SIZES.0, FONT_SIZES.1, 1, 0);
    let row = page.spin_row(parent, "", "", &spin);
    (row, spin)
}

fn theme_combo(
    page: &Page,
    parent: &gtk4::Box,
    appearance: &Rc<DesktopAppearance>,
    icon: &str,
    listed: impl Fn(&DesktopAppearance) -> (Vec<String>, String) + 'static,
    chosen: impl Fn(&Rc<DesktopAppearance>, &str) + 'static,
) {
    let combo = page.combo(parent, icon);
    follow(page, appearance, {
        let combo = Rc::downgrade(&combo);
        move |appearance| {
            if let Some(combo) = combo.upgrade() {
                let (items, current) = listed(appearance);
                combo.set_items_showing(&items, &current);
            }
        }
    });
    combo.connect_activated({
        let appearance = Rc::downgrade(appearance);
        let combo = Rc::downgrade(&combo);
        move |index| {
            let (Some(appearance), Some(combo)) = (appearance.upgrade(), combo.upgrade()) else {
                return;
            };
            if let Some(value) = combo.item(index) {
                chosen(&appearance, &value);
            }
        }
    });
}

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
    let appearance = DesktopAppearance::new();
    let options = HyprOptions::new(&OPTIONS);

    let desktop = page.section("wallpaper", "Desktop");
    for (icon, title, subtitle, id) in [
        (
            "texture",
            "Background",
            "Wallpaper, clock and the widgets on it",
            "background",
        ),
        (
            "toast",
            "Bar",
            "What the bar shows and where it sits",
            "bar",
        ),
        (
            "bottom_app_bar",
            "Panels",
            "Dock, sidebars and the things that pop up",
            "panels",
        ),
    ] {
        page.link_row(&desktop, icon, title, subtitle, context.subpage_opener(id));
    }

    let theme = page.section("palette", "Theme");
    let gtk = page.subsection(&theme, "GTK theme", "");
    theme_combo(
        &page,
        &gtk,
        &appearance,
        "web_asset",
        |appearance| {
            let state = appearance.state();
            (state.gtk_themes.clone(), state.gtk_theme.clone())
        },
        |appearance, value| {
            let qt = appearance.state().qt_style.clone();
            appearance.set_themes(value, &qt);
        },
    );
    let qt = page.subsection(&theme, "Qt style", "");
    theme_combo(
        &page,
        &qt,
        &appearance,
        "format_paint",
        |appearance| {
            let state = appearance.state();
            (state.qt_styles.clone(), state.qt_style.clone())
        },
        |appearance, value| {
            let gtk = appearance.state().gtk_theme.clone();
            appearance.set_themes(&gtk, value);
        },
    );
    let icons = page.subsection(
        &theme,
        "Icon theme",
        "Applied to GTK, Qt and the shell at once.",
    );
    theme_combo(
        &page,
        &icons,
        &appearance,
        "imagesmode",
        |appearance| {
            let state = appearance.state();
            (state.icon_themes.clone(), state.icon_theme.clone())
        },
        |appearance, value| appearance.set_icons(value),
    );

    let colors = page.section("colors", "Color generation");
    let themed = page.subsection(&colors, "What gets themed", "");
    page.tools_notice(
        &themed,
        &[&tools::MATUGEN],
        "apps themed through matugen templates keep their colors",
    );
    let material_you_in_venv =
        std::env::var_os("ILLOGICAL_IMPULSE_VIRTUAL_ENV").is_some_and(|venv| {
            std::path::Path::new(&venv)
                .join("bin")
                .join(tools::MATERIAL_YOU.program)
                .is_file()
        });
    if !material_you_in_venv {
        page.tools_notice(
            &themed,
            &[&tools::MATERIAL_YOU],
            "Qt apps keep their colors",
        );
    }
    page.config_switch(
        &themed,
        "hardware",
        "Shell & utilities",
        "/appearance/wallpaperTheming/enableAppsAndShell",
        true,
    );
    for (icon, label, pointer) in [
        (
            "tv_options_input_settings",
            "Qt apps",
            "/appearance/wallpaperTheming/enableQtApps",
        ),
        (
            "terminal",
            "Terminal",
            "/appearance/wallpaperTheming/enableTerminal",
        ),
    ] {
        let switch = page.config_switch(&themed, icon, label, pointer, true);
        page.tip(
            &switch.button,
            "Shell & utilities theming must also be enabled",
        );
    }
    let terminal = page.subsection(
        &colors,
        "Terminal colors",
        "Ignored if terminal theming is not enabled",
    );
    page.config_switch(
        &terminal,
        "dark_mode",
        "Force dark mode in terminal",
        "/appearance/wallpaperTheming/terminalGenerationProps/forceDarkMode",
        false,
    );
    page.config_spin_scaled(
        &terminal,
        "invert_colors",
        "Harmony (%)",
        "/appearance/wallpaperTheming/terminalGenerationProps/harmony",
        0.6,
        100.0,
        (0, 100),
        10,
    );
    page.config_spin(
        &terminal,
        "gradient",
        "Harmonize threshold",
        "/appearance/wallpaperTheming/terminalGenerationProps/harmonizeThreshold",
        100,
        (0, 100),
        10,
    );
    page.config_spin_scaled(
        &terminal,
        "format_color_text",
        "Foreground boost (%)",
        "/appearance/wallpaperTheming/terminalGenerationProps/termFgBoost",
        0.35,
        100.0,
        (0, 100),
        10,
    );

    let fonts = page.section("text_format", "Fonts");
    let roles = page.subsection(
        &fonts,
        "Apps & panels",
        "Sizes apply to GTK and Qt apps; panels scale their own.",
    );
    for (role, shell_keys, name) in FONT_ROLES {
        let row = page.row(&roles);
        font_label(&row, name);
        let combo = page.combo(&row, "font_download");
        let (_, size) = size_spin(&page, &row);
        follow(&page, &appearance, {
            let combo = Rc::downgrade(&combo);
            let size = Rc::downgrade(&size);
            move |appearance| {
                let (Some(combo), Some(size)) = (combo.upgrade(), size.upgrade()) else {
                    return;
                };
                let font = appearance.font(role);
                combo.set_items_showing(
                    &family_options(&font.family, appearance.family_names()),
                    &font.family,
                );
                size.set_value(font.size);
            }
        });
        combo.connect_activated({
            let appearance = Rc::downgrade(&appearance);
            let combo = Rc::downgrade(&combo);
            move |index| {
                let (Some(appearance), Some(combo)) = (appearance.upgrade(), combo.upgrade())
                else {
                    return;
                };
                let Some(family) = combo.item(index) else {
                    return;
                };
                appearance.set_font(
                    role,
                    Parts {
                        family: Some(family.clone()),
                        ..Parts::default()
                    },
                );
                store_shell_fonts(shell_keys, &family);
            }
        });
        size.connect_changed({
            let appearance = Rc::downgrade(&appearance);
            move |value| {
                if let Some(appearance) = appearance.upgrade() {
                    appearance.set_font(
                        role,
                        Parts {
                            size: Some(value),
                            ..Parts::default()
                        },
                    );
                }
            }
        });
    }

    let bulk = page.subsection(
        &fonts,
        "Adjust all",
        "Sets every role above at once. Fixed width keeps its own family so code stays monospaced.",
    );
    let changes_family = Rc::new(Cell::new(true));
    let changes_size = Rc::new(Cell::new(false));
    let bulk_family: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let bulk_size: Rc<Cell<Option<i64>>> = Rc::new(Cell::new(None));
    let chosen_family = {
        let bulk_family = bulk_family.clone();
        move |appearance: &DesktopAppearance| {
            bulk_family
                .borrow()
                .clone()
                .unwrap_or_else(|| appearance.font("general").family)
        }
    };
    let chosen_size = {
        let bulk_size = bulk_size.clone();
        move |appearance: &DesktopAppearance| {
            bulk_size
                .get()
                .unwrap_or_else(|| appearance.font("general").size)
        }
    };
    let what = page.uniform_row(&bulk);
    let family_switch = page.switch(&what, "font_download", "Family", {
        let changes_family = changes_family.clone();
        move |on| changes_family.set(on)
    });
    family_switch.bind({
        let changes_family = changes_family.clone();
        move || changes_family.get()
    });
    let size_switch = page.switch(&what, "format_size", "Size", {
        let changes_size = changes_size.clone();
        move |on| changes_size.set(on)
    });
    size_switch.bind({
        let changes_size = changes_size.clone();
        move || changes_size.get()
    });
    let values = page.row(&bulk);
    let family_combo = page.combo(&values, "font_download");
    let (size_row, size_box) = size_spin(&page, &values);
    let refresh_bulk = {
        let family_combo = Rc::downgrade(&family_combo);
        let size_box = Rc::downgrade(&size_box);
        let size_row = size_row.clone();
        let changes_family = changes_family.clone();
        let changes_size = changes_size.clone();
        let chosen_family = chosen_family.clone();
        let chosen_size = chosen_size.clone();
        move |appearance: &DesktopAppearance| {
            let (Some(combo), Some(size)) = (family_combo.upgrade(), size_box.upgrade()) else {
                return;
            };
            let family = chosen_family(appearance);
            combo.set_items_showing(&family_options(&family, appearance.family_names()), &family);
            combo.set_enabled(changes_family.get());
            size.set_value(chosen_size(appearance));
            Page::set_spin_row_enabled(&size_row, &size, changes_size.get());
        }
    };
    let refresh_bulk = Rc::new(refresh_bulk);
    follow(&page, &appearance, {
        let refresh_bulk = refresh_bulk.clone();
        move |appearance| refresh_bulk(appearance)
    });
    for switch in [&family_switch, &size_switch] {
        switch.button.connect_clicked({
            let appearance = Rc::downgrade(&appearance);
            let refresh_bulk = refresh_bulk.clone();
            move |_| {
                if let Some(appearance) = appearance.upgrade() {
                    refresh_bulk(&appearance);
                }
            }
        });
    }
    family_combo.connect_activated({
        let appearance = Rc::downgrade(&appearance);
        let combo = Rc::downgrade(&family_combo);
        let bulk_family = bulk_family.clone();
        let refresh_bulk = refresh_bulk.clone();
        move |index| {
            let (Some(appearance), Some(combo)) = (appearance.upgrade(), combo.upgrade()) else {
                return;
            };
            bulk_family.replace(combo.item(index));
            refresh_bulk(&appearance);
        }
    });
    size_box.connect_changed({
        let bulk_size = bulk_size.clone();
        move |value| bulk_size.set(Some(value))
    });
    let (apply, _) = page.icon_button("done_all", true, "Apply to all fonts", {
        let appearance = Rc::downgrade(&appearance);
        move || {
            let Some(appearance) = appearance.upgrade() else {
                return;
            };
            let family = chosen_family(&appearance);
            let parts = Parts {
                family: (changes_family.get() && !family.is_empty()).then_some(family),
                size: changes_size.get().then(|| chosen_size(&appearance)),
                ..Parts::default()
            };
            let family = parts.family.clone();
            appearance.set_font("all", parts);
            if let Some(family) = family {
                store_shell_fonts(&BULK_SHELL_KEYS, &family);
            }
        }
    });
    bulk.append(&apply);

    let shell_only = page.subsection(
        &fonts,
        "Panels only",
        "Faces the shell uses that GTK and Qt have no equivalent for",
    );
    for (pointer, default, name) in SHELL_FONTS {
        let row = page.row(&shell_only);
        font_label(&row, name);
        let combo = page.combo(&row, "font_download");
        let family = move || config::value_str(pointer).unwrap_or_else(|| default.to_owned());
        let update = Rc::new({
            let combo = Rc::downgrade(&combo);
            let appearance = Rc::downgrade(&appearance);
            move || {
                let (Some(combo), Some(appearance)) = (combo.upgrade(), appearance.upgrade())
                else {
                    return;
                };
                let family = family();
                combo.set_items_showing(
                    &family_options(&family, appearance.family_names()),
                    &family,
                );
            }
        });
        update();
        page.keep(appearance.watch({
            let update = update.clone();
            move || update()
        }));
        page.watch(pointer, move || update());
        combo.connect_activated({
            let combo = Rc::downgrade(&combo);
            move |index| {
                if let Some(family) = combo.upgrade().and_then(|combo| combo.item(index)) {
                    config::store_value(pointer, Value::from(family));
                }
            }
        });
    }

    let pointer = page.section("mouse", "Pointer");
    let cursor = page.subsection(
        &pointer,
        "Cursor theme",
        "Applied to Wayland, XWayland, GTK and Qt at once.",
    );
    theme_combo(
        &page,
        &cursor,
        &appearance,
        "mouse",
        |appearance| {
            let state = appearance.state();
            (state.cursor_themes.clone(), state.cursor_theme.clone())
        },
        |appearance, value| {
            let size = appearance.state().cursor_size;
            appearance.set_cursor(value, size);
        },
    );

    let windows = page.section("select_window", "Windows");
    let corners = page.subsection(&windows, "Corners", "");
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
        "2 is a circle, higher squares the corner off while keeping it smooth",
    );

    let blur = page.subsection(&windows, "Blur", "");
    hyprrows::switch(
        &page,
        &blur,
        &options,
        "blur_on",
        "Blur behind windows",
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
        "X-ray",
        "decoration:blur:xray",
    );
    page.tip(
        &xray.button,
        "A floating window blurs the wallpaper rather than the windows behind it",
    );

    let opacity = page.subsection(&windows, "Opacity", "");
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
        "Keep fullscreen windows opaque",
        hyprconfig::OPAQUE_FULLSCREEN,
    );
    page.tip(
        &opaque.button,
        "Maximized windows too, whether focused or not",
    );

    let shadows = page.subsection(&windows, "Shadows", "");
    hyprrows::switch(
        &page,
        &shadows,
        &options,
        "shadow",
        "Drop shadows under windows",
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
        "Sharp edge",
        "decoration:shadow:sharp",
    );

    let dimming = page.subsection(&windows, "Dimming", "");
    hyprrows::switch(
        &page,
        &dimming,
        &options,
        "brightness_4",
        "Dim windows out of focus",
        "decoration:dim_inactive",
    );
    let undimmed = hyprrows::lines_switch(
        &page,
        &dimming,
        "fullscreen",
        "Keep fullscreen windows undimmed",
        hyprconfig::UNDIMMED_FULLSCREEN,
    );
    page.tip(&undimmed.button, "Maximized windows too");
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

    let rendering = page.subsection(&windows, "Rendering", "");
    let tearing = hyprrows::switch(
        &page,
        &rendering,
        &options,
        "screenshot_monitor",
        "Allow tearing",
        "general:allow_tearing",
    );
    page.tip(
        &tearing.button,
        "Lets a game draw a frame before the display is ready for it, trading a torn line for latency",
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

    let shell = page.section("select_window_2", "Shell windows");
    let titlebar = page.config_switch(
        &shell,
        "toolbar",
        "Show title bar",
        "/windows/showTitlebar",
        true,
    );
    page.tip(
        &titlebar.button,
        "Client-side decorations for shell apps like this one",
    );
    let center = page.config_switch(
        &shell,
        "format_align_center",
        "Center title",
        "/windows/centerTitle",
        true,
    );
    let center_follows =
        move || center.set_enabled(config::value_bool("/windows/showTitlebar", true));
    center_follows();
    page.watch("/windows/showTitlebar", center_follows);

    page.keep(appearance);
    page.keep(options);
    page
}
