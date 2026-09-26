use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::panels::settings::content::{Choice, Context, Page};
use crate::panels::settings::hyprrows::{self, Spin};
use crate::services::hyproptions::HyprOptions;
use crate::ui::widgets::selection::Selection;

const OPTIONS: [&str; 28] = [
    "general:layout",
    "general:gaps_in",
    "general:gaps_out",
    "general:border_size",
    "general:gaps_workspaces",
    "general:snap:enabled",
    "general:snap:window_gap",
    "general:snap:monitor_gap",
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

fn layout(options: &HyprOptions) -> String {
    let layout = options.text("general:layout");
    if layout.is_empty() {
        "dwindle".to_owned()
    } else {
        layout
    }
}

fn choice(label: &str, icon: &'static str, value: Value) -> Choice {
    Choice {
        label: label.to_owned(),
        icon,
        value,
    }
}

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

    let tiling = page.section("grid_view", "Tiling");
    let layout_group = page.subsection(&tiling, "Layout", "");
    let layouts = Selection::new(
        &page.theme,
        vec![
            choice("Dwindle", "splitscreen_right", Value::from("dwindle")),
            choice("Master", "splitscreen_left", Value::from("master")),
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

    let spacing = page.subsection(&tiling, "Spacing", "");
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

    let dwindle = page.subsection(&tiling, "Dwindle", "");
    hyprrows::switch(
        &page,
        &dwindle,
        &options,
        "splitscreen",
        "Keep the split direction when a window closes",
        "dwindle:preserve_split",
    );
    hyprrows::switch(
        &page,
        &dwindle,
        &options,
        "aspect_ratio",
        "Split along the longer side",
        "dwindle:smart_split",
    );
    hyprrows::switch(
        &page,
        &dwindle,
        &options,
        "open_in_full",
        "Resize towards the edge being dragged",
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

    let master = page.subsection(&tiling, "Master", "");
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
        "New windows join at the top",
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

    let snapping = page.subsection(
        &tiling,
        "Snapping",
        "Applies to floating windows being dragged",
    );
    hyprrows::switch(
        &page,
        &snapping,
        &options,
        "grid_goldenratio",
        "Snap to other windows and to the screen",
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

    let overview = page.section("overview_key", "Overview");
    page.config_switch(&overview, "check", "Enable", "/overview/enable", true);
    let looks = page.subsection(&overview, "Looks", "");
    page.config_switch(
        &looks,
        "center_focus_strong",
        "Center icons",
        "/overview/centerIcons",
        true,
    );
    page.config_spin_scaled(
        &looks,
        "loupe",
        "Scale (%)",
        "/overview/scale",
        0.18,
        100.0,
        (1, 100),
        1,
    );
    let grid = page.subsection(&overview, "Workspace grid", "");
    let size = page.uniform_row(&grid);
    page.config_spin(
        &size,
        "splitscreen_bottom",
        "Rows",
        "/overview/rows",
        2,
        (1, 20),
        1,
    );
    page.config_spin(
        &size,
        "splitscreen_right",
        "Columns",
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
                choice(first.0, first.1, Value::from(0)),
                choice(second.0, second.1, Value::from(1)),
            ],
            &[pointer],
            move || Value::from(i64::from(config::value_bool(pointer, false))),
            move |value| config::store_value(pointer, Value::Bool(value.as_i64() == Some(1))),
        );
    }

    let workspaces = page.section("select_window_2", "Workspaces");
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
        hyprrows::switch(&page, &workspaces, &options, icon, label, option);
    }

    let swiping = page.subsection(
        &workspaces,
        "Swiping between workspaces",
        "Which fingers do the swiping is set in the Hyprland configuration; these are the numbers behind it",
    );
    let swipe_row = page.uniform_row(&swiping);
    hyprrows::spin(
        &page,
        &swipe_row,
        &options,
        &spin(
            "swipe",
            "Full swipe (px)",
            "gestures:workspace_swipe_distance",
            1.0,
            (100, 2000),
            50,
        ),
    );
    hyprrows::spin(
        &page,
        &swipe_row,
        &options,
        &spin(
            "undo",
            "Give up under (%)",
            "gestures:workspace_swipe_cancel_ratio",
            100.0,
            (0, 100),
            5,
        ),
    );
    hyprrows::spin(
        &page,
        &swiping,
        &options,
        &spin(
            "speed",
            "Flick speed that switches anyway",
            "gestures:workspace_swipe_min_speed_to_force",
            1.0,
            (0, 100),
            1,
        ),
    );
    hyprrows::switch(
        &page,
        &swiping,
        &options,
        "swap_horiz",
        "A swipe keeps the direction it started in",
        "gestures:workspace_swipe_direction_lock",
    );
    let lock_after = hyprrows::spin(
        &page,
        &swiping,
        &options,
        &spin(
            "straighten",
            "Locks after (px)",
            "gestures:workspace_swipe_direction_lock_threshold",
            1.0,
            (0, 200),
            5,
        ),
    );
    hyprrows::switch(
        &page,
        &swiping,
        &options,
        "add_box",
        "Swiping past the last workspace makes a new one",
        "gestures:workspace_swipe_create_new",
    );
    hyprrows::switch(
        &page,
        &swiping,
        &options,
        "all_inclusive",
        "Keep swiping without lifting the fingers",
        "gestures:workspace_swipe_forever",
    );
    let distance = page.subsection(
        &workspaces,
        "Distance between workspaces",
        "How far apart two workspaces sit while the switch is animating",
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
        let (dwindle, master) = (dwindle.parent(), master.parent());
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
            let snap = options.flag("general:snap:enabled");
            Page::set_spin_row_enabled(&window_gap.0, &window_gap.1, snap);
            Page::set_spin_row_enabled(&monitor_gap.0, &monitor_gap.1, snap);
            let locked = options.flag("gestures:workspace_swipe_direction_lock");
            Page::set_spin_row_enabled(&lock_after.0, &lock_after.1, locked);
        }
    };
    follow();
    options.connect_changed(follow);
    page.keep(options);
    page
}
