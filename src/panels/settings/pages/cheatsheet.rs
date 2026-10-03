use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::panels::choice;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::text;

const SUPER_KEY: &str = "/cheatsheet/superKey";
const SUPER_KEYS: [&str; 19] = [
    "\u{f05b3}",
    "\u{e8e5}",
    "\u{f0a21}",
    "\u{ebc6}",
    "\u{f033d}",
    "\u{f08c7}",
    "\u{f322}",
    "\u{f312}",
    "\u{e7e6}",
    "\u{e77d}",
    "\u{ef72}",
    "\u{f111b}",
    "\u{e7d9}",
    "\u{f1b6}",
    "\u{e711}",
    "\u{2318}",
    "\u{f0032}",
    "\u{f07cd}",
    "\u{f268}",
];

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let cheatsheet = page.section("", "");
    let super_key = page.subsection(
        &cheatsheet,
        &tr("Super key symbol"),
        &tr("You can also manually edit cheatsheet.superKey"),
    );
    let keys = Selection::with_family(
        &page.theme,
        SUPER_KEYS.iter().map(|key| choice(key, "", key)).collect(),
        text::Family::Nerd,
        |value| config::store_value(SUPER_KEY, value),
    );
    let show_key = {
        let keys = Rc::downgrade(&keys);
        move || {
            if let Some(keys) = keys.upgrade() {
                keys.set_current(&config::value(SUPER_KEY).unwrap_or(Value::from("")));
            }
        }
    };
    show_key();
    page.watch(SUPER_KEY, show_key);
    super_key.append(&keys.root);
    page.keep(keys);

    let symbols = page.subsection(&cheatsheet, &tr("Symbols"), "");
    for (icon, label, pointer, tip) in [
        (
            "\u{f0635}",
            "Use macOS-like symbols for mods keys",
            "/cheatsheet/useMacSymbol",
            "e.g. \u{f0634}  for Ctrl, \u{f0635}  for Alt, \u{f0636}  for Shift, etc",
        ),
        (
            "\u{f12b6}",
            "Use symbols for function keys",
            "/cheatsheet/useFnSymbol",
            "e.g. \u{f12ab} for F1, \u{f12b6}  for F12",
        ),
        (
            "\u{f037d}",
            "Use symbols for mouse",
            "/cheatsheet/useMouseSymbol",
            "Replace \u{f1550}   for \"Scroll ↓\", \u{f1551}   \"Scroll ↑\", L\u{f037d}   \"LMB\", R\u{f037d}   \"RMB\", \u{f1552}   \"Scroll ↑/↓\" and ⇞/⇟ for \"Page_↑/↓\"",
        ),
    ] {
        let switch = page.config_switch(&symbols, icon, &tr(label), pointer, false);
        page.tip(&switch.button, &tr(tip));
    }
    let keycaps = page.subsection(&cheatsheet, &tr("Keycaps"), "");
    let split = page.config_switch(
        &keycaps,
        "highlight_keyboard_focus",
        &tr("Split buttons"),
        "/cheatsheet/splitButtons",
        false,
    );
    page.tip(
        &split.button,
        &tr(
            "Display modifiers and keys in multiple keycap (e.g., \"Ctrl + A\" instead of \"Ctrl A\" or \"\u{f0634} + A\" instead of \"\u{f0634} A\")",
        ),
    );
    let sizes = page.uniform_row(&keycaps);
    page.config_spin(
        &sizes,
        "",
        &tr("Keybind font size"),
        "/cheatsheet/fontSize/key",
        12,
        (8, 30),
        1,
    );
    page.config_spin(
        &sizes,
        "",
        &tr("Description font size"),
        "/cheatsheet/fontSize/comment",
        12,
        (8, 30),
        1,
    );
    page
}
