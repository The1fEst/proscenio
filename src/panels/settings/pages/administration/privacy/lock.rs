use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::core::{config, shell, tools};
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::desktop::power::{
    IdleTimeout, check_hypridle_available, general_switch, idle_timeout_row,
};
use crate::platform::hyprland::hypridle::locks;
use crate::services::hyprland::idleoptions::IdleOptions;

const BLUR: &str = "/lock/blur/enable";
const BEFORE_SLEEP: &str = "before_sleep_cmd";
const LOCK_COMMAND: &str = "loginctl lock-session";
const SHELL_SYNTAX: &str = "'\"`\\(){}#\n";

fn before_sleep_command(current: Option<&str>, locks_before_sleep: bool) -> Option<String> {
    let current = current.map(str::trim).filter(|command| !command.is_empty());
    if locks_before_sleep {
        return Some(match current {
            Some(command) if locks(command) => command.to_owned(),
            Some(command) => format!("{LOCK_COMMAND}; {command}"),
            None => LOCK_COMMAND.to_owned(),
        });
    }
    let current = current?;
    if current.contains(|character| SHELL_SYNTAX.contains(character)) {
        return None;
    }
    let sequences: Vec<String> = current
        .split(';')
        .filter_map(|sequence| {
            let steps: Vec<&str> = sequence
                .split("&&")
                .map(str::trim)
                .filter(|step| !step.is_empty() && !locks(step))
                .collect();
            (!steps.is_empty()).then(|| steps.join(" && "))
        })
        .collect();
    (!sequences.is_empty()).then(|| sequences.join("; "))
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = IdleOptions::new();

    let main_section = page.section("", "");
    if check_hypridle_available(&page, &main_section) {
        idle_timeout_row(
            &page,
            &main_section,
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
            &main_section,
            &options,
            "bedtime",
            "Lock before sleep",
            |options| {
                options
                    .general(BEFORE_SLEEP)
                    .is_some_and(|command| locks(&command))
            },
            |options, is_on| {
                let command = before_sleep_command(options.general(BEFORE_SLEEP).as_deref(), is_on);
                options.set_general(BEFORE_SLEEP, command.as_deref());
            },
        );
    }
    page.tools_notice(
        &main_section,
        &[&tools::HYPRLOCK],
        &trf("the session always locks with %1", &[shell::name()]),
    );
    let hyprlock_switch = page.config_switch(
        &main_section,
        "water_drop",
        &trf("Use Hyprlock (instead of %1)", &[shell::name()]),
        "/lock/useHyprlock",
        false,
    );
    page.tip(
        &hyprlock_switch.button,
        &tr("Locks with Hyprlock and its own config instead of the shell's lock screen"),
    );
    page.config_switch(
        &main_section,
        "account_circle",
        &tr("Launch on startup"),
        "/lock/launchOnStartup",
        false,
    );

    let security_section = page.subsection(&main_section, &tr("Security"), "");
    let power_switch = page.config_switch(
        &security_section,
        "settings_power",
        &tr("Require password to power off/restart"),
        "/lock/security/requirePasswordToPower",
        false,
    );
    page.tip(
        &power_switch.button,
        &tr(
            "Remember that on most devices one can always hold the power button to force shutdown\nThis only makes it a tiny bit harder for accidents to happen",
        ),
    );
    page.tools_notice(
        &security_section,
        &[&tools::GNOME_KEYRING],
        &tr("unlocking leaves the keyring as it is"),
    );
    let keyring_switch = page.config_switch(
        &security_section,
        "key_vertical",
        &tr("Also unlock keyring"),
        "/lock/security/unlockKeyring",
        true,
    );
    page.tip(
        &keyring_switch.button,
        &tr(
            "This is usually safe and needed for your browser anyway\nMostly useful for those who use lock on startup instead of a display manager that does it (GDM, SDDM, etc.)",
        ),
    );

    let general_section = page.subsection(&main_section, &tr("Style: general"), "");
    page.config_switch(
        &general_section,
        "center_focus_weak",
        &tr("Center clock"),
        "/lock/centerClock",
        true,
    );
    page.config_switch(
        &general_section,
        "info",
        &tr("Show \"Locked\" text"),
        "/lock/showLockedText",
        true,
    );
    page.config_switch(
        &general_section,
        "shapes",
        &tr("Use varying shapes for password characters"),
        "/lock/materialShapeChars",
        true,
    );

    let blur_section = page.subsection(&main_section, &tr("Style: Blurred"), "");
    page.config_switch(&blur_section, "blur_on", &tr("Enable blur"), BLUR, true);
    let (zoom_row, zoom_spin) = page.config_spin_scaled(
        &blur_section,
        "loupe",
        &tr("Extra wallpaper zoom (%)"),
        "/lock/blur/extraZoom",
        1.1,
        100.0,
        (1, 150),
        2,
    );
    let (radius_row, radius_spin) = page.config_spin(
        &blur_section,
        "blur_circular",
        &tr("Blur radius"),
        "/lock/blur/radius",
        100,
        (0, 300),
        10,
    );
    let update_blur_rows = move || {
        let is_enabled = config::value_bool(BLUR, true);
        Page::set_spin_row_enabled(&zoom_row, &zoom_spin, is_enabled);
        Page::set_spin_row_enabled(&radius_row, &radius_spin, is_enabled);
    };
    update_blur_rows();
    page.watch(BLUR, update_blur_rows);
    page.keep(options);
    page
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_before_sleep_adds_and_removes_only_the_lock() {
        let command = |current: Option<&str>, locks_before_sleep| {
            before_sleep_command(current, locks_before_sleep)
        };
        assert_eq!(command(None, true).as_deref(), Some(LOCK_COMMAND));
        assert_eq!(command(Some(LOCK_COMMAND), false), None);
        assert_eq!(
            command(Some("loginctl lock-session && playerctl pause"), false).as_deref(),
            Some("playerctl pause")
        );
        assert_eq!(
            command(
                Some("playerctl pause; pidof hyprlock || hyprlock; sync"),
                false
            )
            .as_deref(),
            Some("playerctl pause; sync")
        );
        assert_eq!(
            command(
                Some("sh -c 'playerctl pause; loginctl lock-session'"),
                false
            ),
            None
        );
        assert_eq!(
            command(Some("playerctl pause"), true).as_deref(),
            Some("loginctl lock-session; playerctl pause")
        );
    }

    #[test]
    fn only_a_lock_command_counts_as_locking() {
        let command = |current: Option<&str>, locks_before_sleep| {
            before_sleep_command(current, locks_before_sleep)
        };
        assert_eq!(
            command(Some("rfkill block bluetooth"), true).as_deref(),
            Some("loginctl lock-session; rfkill block bluetooth")
        );
        assert_eq!(
            command(Some("loginctl lock-session; rfkill block bluetooth"), false).as_deref(),
            Some("rfkill block bluetooth")
        );
        assert_eq!(
            command(Some("$lock_cmd && flock /tmp/x sync"), false).as_deref(),
            Some("flock /tmp/x sync")
        );
        assert_eq!(
            command(Some("pidof hyprlock && playerctl pause"), false).as_deref(),
            Some("pidof hyprlock && playerctl pause")
        );
        for lock_command in [
            "loginctl lock-session",
            "loginctl --no-ask-password lock-sessions",
            "pidof hyprlock || hyprlock",
            "/usr/bin/swaylock -f",
            "$lock_cmd",
            "proscenio ipc call lock activate",
            "hyprctl dispatch 'hl.dsp.global(\"proscenio:lock\")'",
            "sh -c 'loginctl lock-session'",
        ] {
            assert!(locks(lock_command), "{lock_command}");
        }
        for other in [
            "rfkill block bluetooth",
            "flock /tmp/x sync",
            "clock",
            "unlock",
            "pkill hyprlock",
            "pidof hyprlock",
        ] {
            assert!(!locks(other), "{other}");
        }
    }
}
