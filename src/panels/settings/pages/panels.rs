use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, tools};
use crate::panels::osk::layouts::LAYOUTS;
use crate::panels::settings::content::{Choice, Context, Page};

const OSK_LAYOUT: &str = "/osk/layout";

pub(super) fn choice(label: &str, icon: &'static str, value: &str) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value: Value::from(value),
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let panels = page.section("", "");
    for (icon, title, subtitle, id) in [
        (
            "call_to_action",
            "Dock",
            "Reveal, looks, pinned and ignored apps",
            "dock",
        ),
        (
            "side_navigation",
            "Sidebars",
            "Quick toggles, sliders and opening from the corners",
            "sidebars",
        ),
        (
            "keyboard_keys",
            "Cheat sheet",
            "Key symbols and keycap sizes",
            "cheatsheet",
        ),
    ] {
        page.link_row(
            &panels,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let selector = page.section("wallpaper_slideshow", &tr("Wallpaper selector"));
    page.tools_notice(
        &selector,
        &[&tools::KDIALOG],
        &tr("the system file picker does not open"),
    );
    page.config_switch(
        &selector,
        "ad",
        &tr("Use system file picker"),
        "/wallpaperSelector/useSystemFileDialog",
        false,
    );

    let osk = page.section("keyboard", &tr("On-screen keyboard"));
    page.tools_notice(
        &osk,
        &[&tools::YDOTOOL],
        &tr("the on-screen keyboard types nothing"),
    );
    page.config_switch(
        &osk,
        "keep",
        &tr("Pinned on startup"),
        "/osk/pinnedOnStartup",
        false,
    );
    let layout = page.subsection(&osk, &tr("Layout"), "");
    let layouts = page.combo(&layout, "keyboard_alt");
    let names: Vec<String> = LAYOUTS
        .iter()
        .map(|layout| layout.name.to_owned())
        .collect();
    let show_layout = {
        let layouts = Rc::downgrade(&layouts);
        move || {
            if let Some(layouts) = layouts.upgrade() {
                let current = config::value_str(OSK_LAYOUT).unwrap_or_default();
                layouts.set_items_showing(&names, &current);
            }
        }
    };
    show_layout();
    page.watch(OSK_LAYOUT, show_layout);
    layouts.connect_activated(|index| {
        config::store_value(OSK_LAYOUT, Value::from(LAYOUTS[index].name));
    });
    page
}
