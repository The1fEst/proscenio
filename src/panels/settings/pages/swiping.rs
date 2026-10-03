use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::hyprrows;
use crate::panels::settings::pages::multitasking::{OPTIONS, spin};
use crate::platform::hyprconfig;
use crate::services::hyproptions::HyprOptions;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = HyprOptions::new(hyprconfig::Area::Multitasking, &OPTIONS);

    let swiping = page.section("", "");
    page.notice(
        &swiping,
        "info",
        &tr("Which fingers do the swiping is set in the Hyprland configuration; these are the numbers behind it"),
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
        &tr("A swipe keeps the direction it started in"),
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
        &tr("Swiping past the last workspace makes a new one"),
        "gestures:workspace_swipe_create_new",
    );
    hyprrows::switch(
        &page,
        &swiping,
        &options,
        "all_inclusive",
        &tr("Keep swiping without lifting the fingers"),
        "gestures:workspace_swipe_forever",
    );

    let follow = {
        let options = Rc::downgrade(&options);
        move || {
            let Some(options) = options.upgrade() else {
                return;
            };
            let locked = options.flag("gestures:workspace_swipe_direction_lock");
            Page::set_spin_row_enabled(&lock_after.0, &lock_after.1, locked);
        }
    };
    follow();
    options.connect_changed(follow);
    page.keep(options);
    page
}
