use gtk4::gio;
use gtk4::glib;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::core::i18n::tr;
use crate::platform::appearance::with_ini_value;
use crate::platform::{dbus, privileged};

pub const COMMAND: &str = "power-settings";
const LOGIN: &str = "org.freedesktop.login1";
const LOGIN_PATH: &str = "/org/freedesktop/login1";
const LOGIN_MANAGER: &str = "org.freedesktop.login1.Manager";
const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DROP_IN: &str = "/etc/systemd/logind.conf.d/50-proscenio.conf";
const SECTION: &str = "[Login]";
const SUPPLIES: &str = "/sys/class/power_supply";
const THRESHOLD: &str = "charge_control_end_threshold";
const TMPFILES: &str = "/etc/tmpfiles.d/proscenio-charge-limit.conf";
pub const POWER_KEY: &str = "HandlePowerKey";
pub const LID_KEYS: [&str; 3] = [
    "HandleLidSwitch",
    "HandleLidSwitchExternalPower",
    "HandleLidSwitchDocked",
];
pub const ACTIONS: [&str; 5] = ["ignore", "lock", "suspend", "hibernate", "poweroff"];
pub const LIMIT_RANGE: (i64, i64) = (50, 100);

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Buttons {
    pub actions: Vec<(String, String)>,
    pub lid: bool,
    pub hibernate: bool,
}

impl Buttons {
    pub fn action(&self, key: &str) -> String {
        self.actions
            .iter()
            .find(|(known, _)| known == key)
            .map(|(_, action)| action.clone())
            .unwrap_or_default()
    }
}

pub async fn buttons() -> Buttons {
    let Ok(system) = gio::bus_get_future(gio::BusType::System).await else {
        return Buttons::default();
    };
    let mut actions = Vec::new();
    for key in std::iter::once(POWER_KEY).chain(LID_KEYS) {
        if let Some(action) =
            dbus::string_property(&system, LOGIN, LOGIN_PATH, LOGIN_MANAGER, key).await
        {
            actions.push((key.to_owned(), action));
        }
    }
    let hibernate = system
        .call_future(
            Some(LOGIN),
            LOGIN_PATH,
            LOGIN_MANAGER,
            "CanHibernate",
            None,
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
        .ok()
        .and_then(|reply| reply.child_value(0).str().map(|answer| answer == "yes"))
        .unwrap_or(false);
    let lid = dbus::bool_property(&system, UPOWER, UPOWER_PATH, UPOWER, "LidIsPresent")
        .await
        .unwrap_or(false);
    Buttons {
        actions,
        lid,
        hibernate,
    }
}

pub fn battery_with_limit() -> Option<(String, i64)> {
    let entries = std::fs::read_dir(SUPPLIES).ok()?;
    let mut names: Vec<String> = entries
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("BAT"))
        .collect();
    names.sort();
    names.into_iter().find_map(|name| {
        let limit = std::fs::read_to_string(Path::new(SUPPLIES).join(&name).join(THRESHOLD))
            .ok()?
            .trim()
            .parse()
            .ok()?;
        Some((name, limit))
    })
}

pub async fn set_button(key: &str, action: &str) -> Result<(), String> {
    privileged::request(
        COMMAND,
        vec!["button".to_owned(), key.to_owned(), action.to_owned()],
        tr("The setting was not changed"),
    )
    .await
}

pub async fn set_charge_limit(battery: &str, percent: i64) -> Result<(), String> {
    privileged::request(
        COMMAND,
        vec![
            "charge-limit".to_owned(),
            battery.to_owned(),
            percent.to_string(),
        ],
        tr("The charge limit was not changed"),
    )
    .await
}

pub fn with_button(text: &str, key: &str, action: &str) -> String {
    let text = if text.contains(SECTION) {
        text.to_owned()
    } else {
        format!("{SECTION}\n{text}")
    };
    with_ini_value(&text, key, action, SECTION)
}

pub fn tmpfiles_line(battery: &str, percent: i64) -> String {
    format!("w {SUPPLIES}/{battery}/{THRESHOLD} - - - - {percent}\n")
}

fn valid_battery(name: &str) -> bool {
    name.strip_prefix("BAT").is_some_and(|rest| {
        rest.chars()
            .all(|character| character.is_ascii_alphanumeric())
    })
}

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let done = |success: bool| {
        if success {
            glib::ExitCode::SUCCESS
        } else {
            glib::ExitCode::FAILURE
        }
    };
    match arguments {
        [verb, key, action]
            if verb == "button"
                && (key == POWER_KEY || LID_KEYS.contains(&key.as_str()))
                && ACTIONS.contains(&action.as_str()) =>
        {
            let path = PathBuf::from(DROP_IN);
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            let written = path
                .parent()
                .is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
                && std::fs::write(&path, with_button(&text, key, action)).is_ok();
            let reloaded = Command::new("systemctl")
                .args(["kill", "-s", "HUP", "systemd-logind"])
                .status()
                .is_ok_and(|status| status.success());
            done(written && reloaded)
        }
        [verb, battery, percent] if verb == "charge-limit" && valid_battery(battery) => {
            let Ok(percent) = percent.parse::<i64>() else {
                return glib::ExitCode::FAILURE;
            };
            if !(LIMIT_RANGE.0..=LIMIT_RANGE.1).contains(&percent) {
                return glib::ExitCode::FAILURE;
            }
            let threshold = Path::new(SUPPLIES).join(battery).join(THRESHOLD);
            if std::fs::write(&threshold, percent.to_string()).is_err() {
                eprintln!("ERROR: {}", tr("The battery refused this limit"));
                return glib::ExitCode::FAILURE;
            }
            let kept = if percent == LIMIT_RANGE.1 {
                std::fs::remove_file(TMPFILES).is_ok() || !Path::new(TMPFILES).exists()
            } else {
                std::fs::write(TMPFILES, tmpfiles_line(battery, percent)).is_ok()
            };
            done(kept)
        }
        _ => glib::ExitCode::FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_actions_land_in_the_login_section() {
        assert_eq!(
            with_button("", POWER_KEY, "suspend"),
            "[Login]\nHandlePowerKey=suspend\n"
        );
        assert_eq!(
            with_button(
                "[Login]\nHandlePowerKey=suspend\n",
                "HandleLidSwitch",
                "lock"
            ),
            "[Login]\nHandlePowerKey=suspend\nHandleLidSwitch=lock\n"
        );
        assert_eq!(
            with_button("[Login]\nHandlePowerKey=suspend\n", POWER_KEY, "poweroff"),
            "[Login]\nHandlePowerKey=poweroff\n"
        );
        assert_eq!(
            tmpfiles_line("BAT0", 80),
            "w /sys/class/power_supply/BAT0/charge_control_end_threshold - - - - 80\n"
        );
        assert!(valid_battery("BAT0"));
        assert!(!valid_battery("BAT0/../x"));
        assert!(!valid_battery("AC"));
    }
}
