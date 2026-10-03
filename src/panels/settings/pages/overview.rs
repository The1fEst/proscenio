use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::multitasking::choice;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let overview = page.section("", "");
    page.config_switch(&overview, "check", &tr("Enable"), "/overview/enable", true);
    let looks = page.subsection(&overview, &tr("Looks"), "");
    page.config_switch(
        &looks,
        "center_focus_strong",
        &tr("Center icons"),
        "/overview/centerIcons",
        true,
    );
    page.config_spin_scaled(
        &looks,
        "loupe",
        &tr("Scale (%)"),
        "/overview/scale",
        0.18,
        100.0,
        (1, 100),
        1,
    );
    let grid = page.subsection(&overview, &tr("Workspace grid"), "");
    let size = page.uniform_row(&grid);
    page.config_spin(
        &size,
        "splitscreen_bottom",
        &tr("Rows"),
        "/overview/rows",
        2,
        (1, 20),
        1,
    );
    page.config_spin(
        &size,
        "splitscreen_right",
        &tr("Columns"),
        "/overview/columns",
        5,
        (1, 20),
        1,
    );
    let order = page.uniform_row(&grid);
    for (pointer, first, second) in [
        (
            "/overview/orderRightLeft",
            ("Left to right", "arrow_forward"),
            ("Right to left", "arrow_back"),
        ),
        (
            "/overview/orderBottomUp",
            ("Top-down", "arrow_downward"),
            ("Bottom-up", "arrow_upward"),
        ),
    ] {
        page.selection_of(
            &order,
            vec![
                choice(&tr(first.0), first.1, Value::from(0)),
                choice(&tr(second.0), second.1, Value::from(1)),
            ],
            &[pointer],
            move || Value::from(i64::from(config::value_bool(pointer, false))),
            move |value| config::store_value(pointer, Value::Bool(value.as_i64() == Some(1))),
        );
    }
    page
}
