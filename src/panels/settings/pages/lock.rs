use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::core::{config, shell, tools};
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::power::{
    IdleTimeout, general_switch, hypridle_available, idle_timeout_row,
};
use crate::services::idleoptions::IdleOptions;

const BLUR: &str = "/lock/blur/enable";
const BEFORE_SLEEP: &str = "before_sleep_cmd";
const LOCK_COMMAND: &str = "loginctl lock-session";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = IdleOptions::new();

    let main = page.section("", "");
    if hypridle_available(&page, &main) {
        idle_timeout_row(
            &page,
            &main,
            &options,
            &IdleTimeout {
                what: "lock",
                title: "Automatic Screen Lock",
                tip: "Locks the session after a period of inactivity",
                switch_icon: "lock_clock",
                switch_text: "Lock the session",
                fallback_minutes: 30,
            },
        );
        general_switch(
            &page,
            &main,
            &options,
            "bedtime",
            "Lock before sleep",
            |options| {
                options
                    .general(BEFORE_SLEEP)
                    .is_some_and(|command| command.contains("lock"))
            },
            |options, on| options.set_general(BEFORE_SLEEP, on.then_some(LOCK_COMMAND)),
        );
    }
    page.tools_notice(
        &main,
        &[&tools::HYPRLOCK],
        &trf("the session always locks with %1", &[shell::name()]),
    );
    let hyprlock = page.config_switch(
        &main,
        "water_drop",
        &trf("Use Hyprlock (instead of %1)", &[shell::name()]),
        "/lock/useHyprlock",
        false,
    );
    page.tip(
        &hyprlock.button,
        &tr("If you want to somehow use fingerprint unlock..."),
    );
    page.config_switch(
        &main,
        "account_circle",
        &tr("Launch on startup"),
        "/lock/launchOnStartup",
        false,
    );

    let security = page.subsection(&main, &tr("Security"), "");
    let power = page.config_switch(
        &security,
        "settings_power",
        &tr("Require password to power off/restart"),
        "/lock/security/requirePasswordToPower",
        false,
    );
    page.tip(
        &power.button,
        &tr(
            "Remember that on most devices one can always hold the power button to force shutdown\nThis only makes it a tiny bit harder for accidents to happen",
        ),
    );
    page.tools_notice(
        &security,
        &[&tools::GNOME_KEYRING],
        &tr("unlocking leaves the keyring as it is"),
    );
    let keyring = page.config_switch(
        &security,
        "key_vertical",
        &tr("Also unlock keyring"),
        "/lock/security/unlockKeyring",
        true,
    );
    page.tip(
        &keyring.button,
        &tr(
            "This is usually safe and needed for your browser anyway\nMostly useful for those who use lock on startup instead of a display manager that does it (GDM, SDDM, etc.)",
        ),
    );

    let general = page.subsection(&main, &tr("Style: general"), "");
    page.config_switch(
        &general,
        "center_focus_weak",
        &tr("Center clock"),
        "/lock/centerClock",
        true,
    );
    page.config_switch(
        &general,
        "info",
        &tr("Show \"Locked\" text"),
        "/lock/showLockedText",
        true,
    );
    page.config_switch(
        &general,
        "shapes",
        &tr("Use varying shapes for password characters"),
        "/lock/materialShapeChars",
        true,
    );

    let blurred = page.subsection(&main, &tr("Style: Blurred"), "");
    page.config_switch(&blurred, "blur_on", &tr("Enable blur"), BLUR, true);
    let (zoom_row, zoom) = page.config_spin_scaled(
        &blurred,
        "loupe",
        &tr("Extra wallpaper zoom (%)"),
        "/lock/blur/extraZoom",
        1.1,
        100.0,
        (1, 150),
        2,
    );
    let (radius_row, radius) = page.config_spin(
        &blurred,
        "blur_circular",
        &tr("Blur radius"),
        "/lock/blur/radius",
        100,
        (0, 300),
        10,
    );
    let follow = move || {
        let enabled = config::value_bool(BLUR, true);
        Page::set_spin_row_enabled(&zoom_row, &zoom, enabled);
        Page::set_spin_row_enabled(&radius_row, &radius, enabled);
    };
    follow();
    page.watch(BLUR, follow);
    page.keep(options);
    page
}
