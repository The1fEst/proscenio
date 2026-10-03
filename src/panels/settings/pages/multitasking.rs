use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::platform::hyprconfig;
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::selection::Selection;

pub const OPTIONS: [&str; 38] = [
    "general:layout",
    "general:gaps_in",
    "general:gaps_out",
    "general:border_size",
    "general:gaps_workspaces",
    "general:snap:enabled",
    "general:snap:window_gap",
    "general:snap:monitor_gap",
    "general:resize_on_border",
    "general:extend_border_grab_area",
    "scrolling:column_width",
    "scrolling:direction",
    "scrolling:fullscreen_on_one_column",
    "scrolling:follow_focus",
    "scrolling:focus_fit_method",
    "input:follow_mouse",
    "input:focus_on_close",
    "misc:focus_on_activate",
    "dwindle:preserve_split",
    "dwindle:smart_split",
    "dwindle:smart_resizing",
    "dwindle:default_split_ratio",
    "master:new_status",
    "master:orientation",
    "master:new_on_top",
    "master:mfact",
    "binds:workspace_back_and_forth",
    "binds:allow_workspace_cycles",
    "binds:hide_special_on_workspace_change",
    "animations:workspace_wraparound",
    "misc:close_special_on_empty",
    "gestures:workspace_swipe_distance",
    "gestures:workspace_swipe_cancel_ratio",
    "gestures:workspace_swipe_min_speed_to_force",
    "gestures:workspace_swipe_direction_lock",
    "gestures:workspace_swipe_direction_lock_threshold",
    "gestures:workspace_swipe_create_new",
    "gestures:workspace_swipe_forever",
];
const MASTER_PLACES: [(&str, &str); 3] = [
    ("A new window becomes the master", "master"),
    ("A new window joins the stack", "slave"),
    (
        "A new window takes the place of the one it opened from",
        "inherit",
    ),
];
const MASTER_SIDES: [(&str, &str); 5] = [
    ("Left", "left"),
    ("Right", "right"),
    ("Top", "top"),
    ("Bottom", "bottom"),
    ("Centre", "center"),
];
const SCROLL_DIRECTIONS: [(&str, &str); 4] = [
    ("New windows open to the right", "right"),
    ("New windows open to the left", "left"),
    ("New windows open below", "down"),
    ("New windows open above", "up"),
];
const FIT_METHODS: [(&str, &str); 2] = [
    ("Center the focused column", "0"),
    ("Scroll just enough to show the focused column", "1"),
];
const FOLLOW_MOUSE: [(&str, &str); 4] = [
    ("Focus follows the pointer", "1"),
    ("Click to focus", "0"),
    (
        "Click to focus; the window under the pointer still gets hover and scroll",
        "2",
    ),
    (
        "The pointer never moves keyboard focus, not even a click",
        "3",
    ),
];
const FOCUS_ON_CLOSE: [(&str, &str); 3] = [
    ("After a close, focus the next window", "0"),
    ("After a close, focus the window under the pointer", "1"),
    ("After a close, focus the window used last", "2"),
];

fn layout(options: &HyprOptions) -> String {
    let layout = options.text("general:layout");
    if layout.is_empty() {
        "dwindle".to_owned()
    } else {
        layout
    }
}

pub(super) fn choice(label: &str, icon: &'static str, value: Value) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value,
    }
}

pub(super) fn spin(
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
    let options = HyprOptions::new(hyprconfig::Area::Multitasking, &OPTIONS);

    let tiling = page.section("grid_view", &tr("Tiling"));
    let layout_group = page.subsection(&tiling, &tr("Layout"), "");
    let layouts = Selection::new(
        &page.theme,
        vec![
            choice(&tr("Dwindle"), "splitscreen_right", Value::from("dwindle")),
            choice(&tr("Master"), "splitscreen_left", Value::from("master")),
            choice(&tr("Scrolling"), "view_week", Value::from("scrolling")),
            choice(&tr("Monocle"), "fullscreen", Value::from("monocle")),
        ],
        {
            let options = Rc::downgrade(&options);
            move |value| {
                if let (Some(options), Some(name)) = (options.upgrade(), value.as_str()) {
                    options.set("general:layout", name);
                }
            }
        },
    );
    layout_group.append(&layouts.root);
    page.keep(layouts.clone());

    let spacing = page.subsection(&tiling, &tr("Spacing"), "");
    for spec in [
        spin(
            "width",
            "Gap between windows",
            "general:gaps_in",
            1.0,
            (0, 100),
            1,
        ),
        spin(
            "fit_screen",
            "Gap around the screen",
            "general:gaps_out",
            1.0,
            (0, 200),
            1,
        ),
        spin(
            "border_outer",
            "Border width",
            "general:border_size",
            1.0,
            (0, 20),
            1,
        ),
    ] {
        hyprrows::spin(&page, &spacing, &options, &spec);
    }
    let smart = hyprrows::lines_switch(
        &page,
        &spacing,
        "crop_free",
        &tr("Smart gaps"),
        &hyprconfig::SMART_GAPS,
    );
    page.tip(
        &smart.button,
        &tr("No gaps, border or rounding around a workspace's only tiled window, or a maximized one"),
    );

    let dwindle = page.subsection(&tiling, &tr("Dwindle"), "");
    hyprrows::switch(
        &page,
        &dwindle,
        &options,
        "splitscreen",
        &tr("Keep the split direction when a window closes"),
        "dwindle:preserve_split",
    );
    hyprrows::switch(
        &page,
        &dwindle,
        &options,
        "aspect_ratio",
        &tr("Split along the longer side"),
        "dwindle:smart_split",
    );
    hyprrows::switch(
        &page,
        &dwindle,
        &options,
        "open_in_full",
        &tr("Resize towards the edge being dragged"),
        "dwindle:smart_resizing",
    );
    hyprrows::spin(
        &page,
        &dwindle,
        &options,
        &spin(
            "vertical_split",
            "New window takes (%)",
            "dwindle:default_split_ratio",
            100.0,
            (10, 190),
            5,
        ),
    );

    let master = page.subsection(&tiling, &tr("Master"), "");
    hyprrows::combo(
        &page,
        &master,
        &options,
        "open_with",
        ("master:new_status", "slave"),
        &MASTER_PLACES,
    );
    hyprrows::combo(
        &page,
        &master,
        &options,
        "chevron_left",
        ("master:orientation", "left"),
        &MASTER_SIDES,
    );
    hyprrows::switch(
        &page,
        &master,
        &options,
        "vertical_align_top",
        &tr("New windows join at the top"),
        "master:new_on_top",
    );
    hyprrows::spin(
        &page,
        &master,
        &options,
        &spin(
            "width",
            "Master takes (%)",
            "master:mfact",
            100.0,
            (10, 90),
            5,
        ),
    );

    let scrolling = page.subsection(&tiling, &tr("Scrolling"), "");
    hyprrows::spin(
        &page,
        &scrolling,
        &options,
        &spin(
            "width",
            "Column width (%)",
            "scrolling:column_width",
            100.0,
            (10, 100),
            5,
        ),
    );
    hyprrows::combo(
        &page,
        &scrolling,
        &options,
        "arrow_forward",
        ("scrolling:direction", "right"),
        &SCROLL_DIRECTIONS,
    );
    hyprrows::combo(
        &page,
        &scrolling,
        &options,
        "fit_width",
        ("scrolling:focus_fit_method", "1"),
        &FIT_METHODS,
    );
    hyprrows::switch(
        &page,
        &scrolling,
        &options,
        "center_focus_strong",
        &tr("Scroll to the focused window"),
        "scrolling:follow_focus",
    );
    hyprrows::switch(
        &page,
        &scrolling,
        &options,
        "fullscreen",
        &tr("A single column fills the screen"),
        "scrolling:fullscreen_on_one_column",
    );

    let snapping = page.subsection(
        &tiling,
        &tr("Snapping"),
        &tr("Applies to floating windows being dragged"),
    );
    hyprrows::switch(
        &page,
        &snapping,
        &options,
        "grid_goldenratio",
        &tr("Snap to other windows and to the screen"),
        "general:snap:enabled",
    );
    let gaps = page.uniform_row(&snapping);
    let window_gap = hyprrows::spin(
        &page,
        &gaps,
        &options,
        &spin(
            "width",
            "To windows (px)",
            "general:snap:window_gap",
            1.0,
            (0, 100),
            1,
        ),
    );
    let monitor_gap = hyprrows::spin(
        &page,
        &gaps,
        &options,
        &spin(
            "fit_screen",
            "To the screen (px)",
            "general:snap:monitor_gap",
            1.0,
            (0, 100),
            1,
        ),
    );

    let resizing = page.subsection(&tiling, &tr("Resizing"), "");
    hyprrows::switch(
        &page,
        &resizing,
        &options,
        "resize",
        &tr("Resize windows by dragging their borders"),
        "general:resize_on_border",
    );
    let grab_area = hyprrows::spin(
        &page,
        &resizing,
        &options,
        &spin(
            "width",
            "Grab area around the border (px)",
            "general:extend_border_grab_area",
            1.0,
            (0, 100),
            1,
        ),
    );

    let focus = page.section("arrow_selector_tool", &tr("Focus"));
    hyprrows::combo(
        &page,
        &focus,
        &options,
        "arrow_selector_tool",
        ("input:follow_mouse", "1"),
        &FOLLOW_MOUSE,
    );
    hyprrows::combo(
        &page,
        &focus,
        &options,
        "close",
        ("input:focus_on_close", "0"),
        &FOCUS_ON_CLOSE,
    );
    let activate = hyprrows::switch(
        &page,
        &focus,
        &options,
        "open_in_new",
        &tr("Let apps take focus when they ask for it"),
        "misc:focus_on_activate",
    );
    page.tip(
        &activate.button,
        &tr("Exceptions for single apps: Apps › Window rules"),
    );

    let overview = page.section("", "");
    page.link_row(
        &overview,
        "overview_key",
        &tr("Overview"),
        &tr("The workspace grid, its size, order and looks"),
        context.subpage_opener("overview"),
    );

    let workspaces = page.section("select_window_2", &tr("Workspaces"));
    for (icon, label, option) in [
        (
            "history",
            "Switching to the current workspace goes back to the last one",
            "binds:workspace_back_and_forth",
        ),
        (
            "repeat",
            "Moving past the last workspace wraps around",
            "binds:allow_workspace_cycles",
        ),
        (
            "animation",
            "Animate the wrap around",
            "animations:workspace_wraparound",
        ),
        (
            "layers_clear",
            "Hide the special workspace when switching",
            "binds:hide_special_on_workspace_change",
        ),
        (
            "close",
            "Close the special workspace once it is empty",
            "misc:close_special_on_empty",
        ),
    ] {
        hyprrows::switch(&page, &workspaces, &options, icon, &tr(label), option);
    }

    page.link_row(
        &workspaces,
        "swipe",
        &tr("Swiping between workspaces"),
        &tr("How far a swipe goes, when it gives up and when it locks"),
        context.subpage_opener("swiping"),
    );
    let distance = page.subsection(
        &workspaces,
        &tr("Distance between workspaces"),
        &tr("How far apart two workspaces sit while the switch is animating"),
    );
    hyprrows::spin(
        &page,
        &distance,
        &options,
        &spin(
            "width",
            "Gap (px)",
            "general:gaps_workspaces",
            1.0,
            (0, 500),
            10,
        ),
    );

    let follow = {
        let options = Rc::downgrade(&options);
        let (dwindle, master, scrolling) = (dwindle.parent(), master.parent(), scrolling.parent());
        move || {
            let Some(options) = options.upgrade() else {
                return;
            };
            let current = layout(&options);
            layouts.set_current(&Value::from(current.clone()));
            if let Some(dwindle) = &dwindle {
                dwindle.set_visible(current == "dwindle");
            }
            if let Some(master) = &master {
                master.set_visible(current == "master");
            }
            if let Some(scrolling) = &scrolling {
                scrolling.set_visible(current == "scrolling");
            }
            let resize = options.flag("general:resize_on_border");
            Page::set_spin_row_enabled(&grab_area.0, &grab_area.1, resize);
            let snap = options.flag("general:snap:enabled");
            Page::set_spin_row_enabled(&window_gap.0, &window_gap.1, snap);
            Page::set_spin_row_enabled(&monitor_gap.0, &monitor_gap.1, snap);
        }
    };
    follow();
    options.connect_changed(follow);
    page.keep(options);
    page
}
