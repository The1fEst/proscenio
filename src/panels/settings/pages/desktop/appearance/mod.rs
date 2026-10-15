pub mod background;
pub mod bar;
pub mod colors;
pub mod fonts;
pub mod panels;
pub mod windows;

use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::hyprrows;
use crate::platform::hyprland::hyprconfig::Area;
use crate::services::hyprland::hyproptions::HyprOptions;
use crate::services::session::appearance::DesktopAppearance;
use crate::ui::widgets::input::spinbox::SpinBox;

pub const OPTIONS: [&str; 1] = ["cursor:enable_hyprcursor"];

pub(super) fn follow(
    page: &Page,
    appearance: &Rc<DesktopAppearance>,
    on_changed: impl Fn(&DesktopAppearance) + 'static,
) {
    on_changed(appearance);
    page.keep(appearance.watch({
        let appearance = Rc::downgrade(appearance);
        move || {
            if let Some(appearance) = appearance.upgrade() {
                on_changed(&appearance);
            }
        }
    }));
}

fn theme_combo(
    page: &Page,
    parent: &gtk4::Box,
    appearance: &Rc<DesktopAppearance>,
    icon: &str,
    read_items_and_current: impl Fn(&DesktopAppearance) -> (Vec<String>, String) + 'static,
    on_selected: impl Fn(&Rc<DesktopAppearance>, &str) + 'static,
) {
    let combo = page.combo(parent, icon);
    follow(page, appearance, {
        let combo = Rc::downgrade(&combo);
        move |appearance| {
            if let Some(combo) = combo.upgrade() {
                let (items, current_item) = read_items_and_current(appearance);
                combo.set_items_showing(&items, &current_item);
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
                on_selected(&appearance, &value);
            }
        }
    });
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let appearance = DesktopAppearance::new();
    let options = HyprOptions::new(Area::Appearance, &OPTIONS);

    let desktop_section = page.section("wallpaper", &tr("Desktop"));
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
            &desktop_section,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let theme_section = page.section("palette", &tr("Theme"));
    let gtk_section = page.subsection(&theme_section, &tr("GTK theme"), "");
    theme_combo(
        &page,
        &gtk_section,
        &appearance,
        "web_asset",
        |appearance| {
            let state = appearance.state();
            (state.gtk_themes.clone(), state.gtk_theme.clone())
        },
        |appearance, value| {
            let qt_style = appearance.state().qt_style.clone();
            appearance.set_themes(value, &qt_style);
        },
    );
    let qt_section = page.subsection(&theme_section, &tr("Qt style"), "");
    theme_combo(
        &page,
        &qt_section,
        &appearance,
        "format_paint",
        |appearance| {
            let state = appearance.state();
            (state.qt_styles.clone(), state.qt_style.clone())
        },
        |appearance, value| {
            let gtk_theme = appearance.state().gtk_theme.clone();
            appearance.set_themes(&gtk_theme, value);
        },
    );
    let icons_section = page.subsection(
        &theme_section,
        &tr("Icon theme"),
        &tr("Applied to GTK, Qt and the shell at once."),
    );
    theme_combo(
        &page,
        &icons_section,
        &appearance,
        "imagesmode",
        |appearance| {
            let state = appearance.state();
            (state.icon_themes.clone(), state.icon_theme.clone())
        },
        |appearance, value| appearance.set_icons(value),
    );

    let looks_section = page.section("", "");
    for (icon, title, subtitle, id) in [
        (
            "format_paint",
            "Colors",
            "Scheme, accent, transparency and what gets themed",
            "colors",
        ),
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
            &looks_section,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let pointer_section = page.section("mouse", &tr("Pointer"));
    let cursor_section = page.subsection(
        &pointer_section,
        &tr("Cursor theme"),
        &tr("Applied to Wayland, XWayland, GTK and Qt at once."),
    );
    theme_combo(
        &page,
        &cursor_section,
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
    let size_spin = SpinBox::new(&page.theme, 8, 128, 4, 0);
    page.spin_row(&pointer_section, "height", &tr("Cursor size"), &size_spin);
    follow(&page, &appearance, {
        let size_spin = Rc::downgrade(&size_spin);
        move |appearance| {
            if let Some(size_spin) = size_spin.upgrade() {
                size_spin.set_value(appearance.state().cursor_size);
            }
        }
    });
    size_spin.connect_changed({
        let appearance = Rc::downgrade(&appearance);
        move |size| {
            if let Some(appearance) = appearance.upgrade() {
                let cursor_theme = appearance.state().cursor_theme.clone();
                appearance.set_cursor(&cursor_theme, size);
            }
        }
    });
    hyprrows::switch(
        &page,
        &pointer_section,
        &options,
        "animated_images",
        &tr("Use hyprcursor themes"),
        "cursor:enable_hyprcursor",
    );

    let shell_section = page.section("select_window_2", &tr("Shell windows"));
    let titlebar_switch = page.config_switch(
        &shell_section,
        "toolbar",
        &tr("Show title bar"),
        "/windows/showTitlebar",
        true,
    );
    page.tip(
        &titlebar_switch.button,
        &tr("Client-side decorations for shell apps like this one"),
    );
    let center_switch = page.config_switch(
        &shell_section,
        "format_align_center",
        &tr("Center title"),
        "/windows/centerTitle",
        true,
    );
    let update_center_switch =
        move || center_switch.set_enabled(config::value_bool("/windows/showTitlebar", true));
    update_center_switch();
    page.watch("/windows/showTitlebar", update_center_switch);

    page.keep(appearance);
    page.keep(options);
    page
}
