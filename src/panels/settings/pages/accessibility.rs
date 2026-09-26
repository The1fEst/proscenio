use std::rc::Rc;

use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::services::appearance::DesktopAppearance;
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::spinbox::SpinBox;

const OPTIONS: [&str; 7] = [
    "animations:enabled",
    "misc:animate_manual_resizes",
    "misc:animate_mouse_windowdragging",
    "input:repeat_delay",
    "input:repeat_rate",
    "cursor:zoom_factor",
    "cursor:zoom_disable_aa",
];

fn spin(
    icon: &'static str,
    label: &'static str,
    option: &'static str,
    factor: f64,
    range: (i64, i64),
    step: i64,
) -> Spin {
    Spin {
        icon,
        label,
        option,
        factor,
        range,
        step,
        decimals: 0,
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(&OPTIONS);
    let appearance = DesktopAppearance::new();

    let seeing = page.section("visibility", "Seeing");
    let cursor = SpinBox::new(&page.theme, 8, 128, 4, 0);
    page.spin_row(&seeing, "height", "Cursor size", &cursor);
    let show_cursor = {
        let cursor = Rc::downgrade(&cursor);
        let appearance = Rc::downgrade(&appearance);
        move || {
            if let (Some(cursor), Some(appearance)) = (cursor.upgrade(), appearance.upgrade()) {
                cursor.set_value(appearance.state().cursor_size);
            }
        }
    };
    show_cursor();
    page.keep(appearance.watch(show_cursor));
    cursor.connect_changed({
        let appearance = Rc::downgrade(&appearance);
        move |size| {
            if let Some(appearance) = appearance.upgrade() {
                let theme = appearance.state().cursor_theme.clone();
                appearance.set_cursor(&theme, size);
            }
        }
    });
    let reduced = hyprrows::option_switch(
        &page,
        &seeing,
        &options,
        ("animation", "Reduced motion"),
        |options| !options.flag("animations:enabled"),
        |options, reduced| options.set("animations:enabled", &(!reduced).to_string()),
    );
    page.tip(
        &reduced.button,
        "Windows and workspaces appear at once instead of moving.",
    );
    hyprrows::switch(
        &page,
        &seeing,
        &options,
        "open_with",
        "Animate manual resizes",
        "misc:animate_manual_resizes",
    );
    hyprrows::switch(
        &page,
        &seeing,
        &options,
        "drag_pan",
        "Animate windows being dragged",
        "misc:animate_mouse_windowdragging",
    );

    let typing = page.section("keyboard", "Typing");
    let repeat = page.subsection(
        &typing,
        "Repeat keys",
        "Key presses repeat when the key is held down",
    );
    let repeat_row = page.uniform_row(&repeat);
    hyprrows::spin(
        &page,
        &repeat_row,
        &options,
        &spin(
            "timer",
            "Delay (ms)",
            "input:repeat_delay",
            1.0,
            (100, 2000),
            25,
        ),
    );
    hyprrows::spin(
        &page,
        &repeat_row,
        &options,
        &spin(
            "speed",
            "Rate (per second)",
            "input:repeat_rate",
            1.0,
            (1, 100),
            1,
        ),
    );

    let zoom = page.section("zoom_in", "Zoom");
    let magnifier = page.subsection(
        &zoom,
        "Magnifier",
        "The whole screen, magnified around the pointer. 100% is no magnification.",
    );
    hyprrows::spin(
        &page,
        &magnifier,
        &options,
        &spin(
            "zoom_in",
            "Magnification (%)",
            "cursor:zoom_factor",
            100.0,
            (100, 500),
            10,
        ),
    );
    hyprrows::switch(
        &page,
        &magnifier,
        &options,
        "grid_on",
        "Keep the magnified image sharp",
        "cursor:zoom_disable_aa",
    );

    page.keep(appearance);
    page.keep(options);
    page
}
