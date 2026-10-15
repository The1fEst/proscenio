pub mod cheatsheet;
pub mod dock;
pub mod sidebars;

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

    let panels_section = page.section("", "");
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
            &panels_section,
            icon,
            &tr(title),
            &tr(subtitle),
            context.subpage_opener(id),
        );
    }

    let osk_section = page.section("keyboard", &tr("On-screen keyboard"));
    page.tools_notice(
        &osk_section,
        &[&tools::YDOTOOL],
        &tr("the on-screen keyboard types nothing"),
    );
    page.config_switch(
        &osk_section,
        "keep",
        &tr("Pinned on startup"),
        "/osk/pinnedOnStartup",
        false,
    );
    let layout_section = page.subsection(&osk_section, &tr("Layout"), "");
    let layout_combo = page.combo(&layout_section, "keyboard_alt");
    let names: Vec<String> = LAYOUTS
        .iter()
        .map(|layout| layout.name.to_owned())
        .collect();
    let show_layout = {
        let layout_combo = Rc::downgrade(&layout_combo);
        move || {
            if let Some(layout_combo) = layout_combo.upgrade() {
                let current_layout = config::value_str(OSK_LAYOUT).unwrap_or_default();
                layout_combo.set_items_showing(&names, &current_layout);
            }
        }
    };
    show_layout();
    page.watch(OSK_LAYOUT, show_layout);
    layout_combo.connect_activated(|index| {
        config::store_value(OSK_LAYOUT, Value::from(LAYOUTS[index].name));
    });
    page
}
