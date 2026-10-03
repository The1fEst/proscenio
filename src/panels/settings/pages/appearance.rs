use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::hyprrows;
use crate::platform::hyprconfig::Area;
use crate::services::appearance::DesktopAppearance;
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::spinbox::SpinBox;

pub const OPTIONS: [&str; 1] = ["cursor:enable_hyprcursor"];

pub(super) fn follow(
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

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let appearance = DesktopAppearance::new();
    let options = HyprOptions::new(Area::Appearance, &OPTIONS);

    let desktop = page.section("wallpaper", &tr("Desktop"));
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
        page.link_row(
            &desktop,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let theme = page.section("palette", &tr("Theme"));
    let gtk = page.subsection(&theme, &tr("GTK theme"), "");
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
    let qt = page.subsection(&theme, &tr("Qt style"), "");
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
        &tr("Icon theme"),
        &tr("Applied to GTK, Qt and the shell at once."),
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

    let colors = page.section("colors", &tr("Color generation"));
    let themed = page.subsection(&colors, &tr("What gets themed"), "");
    page.tools_notice(
        &themed,
        &[&tools::MATUGEN],
        &tr("apps themed through matugen templates keep their colors"),
    );
    page.config_switch(
        &themed,
        "hardware",
        &tr("Shell & utilities"),
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
        let switch = page.config_switch(&themed, icon, &tr(label), pointer, true);
        page.tip(
            &switch.button,
            &tr("Shell & utilities theming must also be enabled"),
        );
    }
    let terminal = page.subsection(
        &colors,
        &tr("Terminal colors"),
        &tr("Ignored if terminal theming is not enabled"),
    );
    page.config_switch(
        &terminal,
        "dark_mode",
        &tr("Force dark mode in terminal"),
        "/appearance/wallpaperTheming/terminalGenerationProps/forceDarkMode",
        false,
    );
    page.config_spin_scaled(
        &terminal,
        "invert_colors",
        &tr("Harmony (%)"),
        "/appearance/wallpaperTheming/terminalGenerationProps/harmony",
        0.6,
        100.0,
        (0, 100),
        10,
    );
    page.config_spin(
        &terminal,
        "gradient",
        &tr("Harmonize threshold"),
        "/appearance/wallpaperTheming/terminalGenerationProps/harmonizeThreshold",
        100,
        (0, 100),
        10,
    );
    page.config_spin_scaled(
        &terminal,
        "format_color_text",
        &tr("Foreground boost (%)"),
        "/appearance/wallpaperTheming/terminalGenerationProps/termFgBoost",
        0.35,
        100.0,
        (0, 100),
        10,
    );

    let looks = page.section("", "");
    for (icon, title, subtitle, id) in [
        (
            "text_format",
            "Fonts",
            "Families and sizes for apps and panels",
            "fonts",
        ),
        (
            "select_window",
            "Windows",
            "Corners, borders, blur, opacity, shadows and dimming",
            "windows",
        ),
    ] {
        page.link_row(
            &looks,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let pointer = page.section("mouse", &tr("Pointer"));
    let cursor = page.subsection(
        &pointer,
        &tr("Cursor theme"),
        &tr("Applied to Wayland, XWayland, GTK and Qt at once."),
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
    let size = SpinBox::new(&page.theme, 8, 128, 4, 0);
    page.spin_row(&pointer, "height", &tr("Cursor size"), &size);
    follow(&page, &appearance, {
        let size = Rc::downgrade(&size);
        move |appearance| {
            if let Some(size) = size.upgrade() {
                size.set_value(appearance.state().cursor_size);
            }
        }
    });
    size.connect_changed({
        let appearance = Rc::downgrade(&appearance);
        move |size| {
            if let Some(appearance) = appearance.upgrade() {
                let theme = appearance.state().cursor_theme.clone();
                appearance.set_cursor(&theme, size);
            }
        }
    });
    hyprrows::switch(
        &page,
        &pointer,
        &options,
        "animated_images",
        &tr("Use hyprcursor themes"),
        "cursor:enable_hyprcursor",
    );

    let shell = page.section("select_window_2", &tr("Shell windows"));
    let titlebar = page.config_switch(
        &shell,
        "toolbar",
        &tr("Show title bar"),
        "/windows/showTitlebar",
        true,
    );
    page.tip(
        &titlebar.button,
        &tr("Client-side decorations for shell apps like this one"),
    );
    let center = page.config_switch(
        &shell,
        "format_align_center",
        &tr("Center title"),
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
