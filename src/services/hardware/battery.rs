use gtk4::gio;
use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::i18n::{tr, trf};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::detach;
use crate::platform::dbus;
use crate::platform::dbus::notify::{self, Notification, Urgency};
use crate::services::hardware::power;
use crate::services::media::audio;

pub const PROFILE_ON_BATTERY: &str = "/battery/profileOnBattery";
pub const PROFILE_ON_CHARGER: &str = "/battery/profileOnCharger";
const BUS: &str = "org.freedesktop.UPower";
const ROOT: &str = "/org/freedesktop/UPower";
const DISPLAY: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const DEVICE: &str = "org.freedesktop.UPower.Device";

pub const BATTERY: u32 = 2;
pub const CHARGING: u32 = 1;
pub const FULLY_CHARGED: u32 = 4;
const PENDING_CHARGE: u32 = 5;

#[derive(Clone, Copy, Default)]
pub struct Charge {
    pub is_available: bool,
    pub state: u32,
    pub percentage: f64,
    pub rate: f64,
    pub time_to_empty: i64,
    pub time_to_full: i64,
    pub health: f64,
}

impl Charge {
    pub fn is_charging(&self) -> bool {
        self.state == CHARGING
    }
}

#[derive(Clone, Copy)]
struct Levels {
    low: f64,
    critical: f64,
    suspend: Option<f64>,
    full: f64,
}

impl Levels {
    fn of(config: &Config) -> Self {
        Levels {
            low: config.battery_low,
            critical: config.battery_critical,
            suspend: config
                .battery_automatic_suspend
                .then_some(config.battery_suspend),
            full: config.battery_full,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Alerts {
    is_low: bool,
    is_critical: bool,
    should_suspend: bool,
    is_full: bool,
    is_plugged: Option<bool>,
}

impl Alerts {
    fn of(charge: &Charge, levels: Levels) -> Self {
        if !charge.is_available {
            return Alerts::default();
        }
        let level = charge.percentage;
        let is_charging = charge.is_charging();
        Alerts {
            is_low: level <= levels.low && !is_charging,
            is_critical: level <= levels.critical && !is_charging,
            should_suspend: levels.suspend.is_some_and(|suspend| level <= suspend) && !is_charging,
            is_full: level >= levels.full && is_charging,
            is_plugged: Some(is_charging || charge.state == PENDING_CHARGE),
        }
    }
}

#[derive(Clone)]
pub struct Battery {
    pub charge: Rc<Cell<Charge>>,
    alerts: Rc<Cell<Alerts>>,
    system: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
}

impl Battery {
    pub fn new(system: Option<gio::DBusConnection>) -> Self {
        let battery = Battery {
            charge: Rc::new(Cell::new(Charge::default())),
            alerts: Rc::default(),
            system,
            listeners: Rc::default(),
        };
        battery.refresh();
        if let Some(system) = &battery.system {
            let subscription = dbus::subscribe_properties_changed(system, BUS, {
                let battery = battery.clone();
                move || battery.refresh()
            });
            std::mem::forget(subscription);
        }
        battery
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    fn refresh(&self) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let battery = self.clone();
        glib::spawn_future_local(async move {
            let kind = dbus::u32_property(&system, BUS, DISPLAY, DEVICE, "Type")
                .await
                .unwrap_or(0);
            let is_present = dbus::bool_property(&system, BUS, DISPLAY, DEVICE, "IsPresent")
                .await
                .unwrap_or(false);
            let charge = Charge {
                is_available: kind == BATTERY && is_present,
                state: dbus::u32_property(&system, BUS, DISPLAY, DEVICE, "State")
                    .await
                    .unwrap_or(0),
                percentage: number(&system, DISPLAY, "Percentage").await / 100.0,
                rate: number(&system, DISPLAY, "EnergyRate").await,
                time_to_empty: integer(&system, DISPLAY, "TimeToEmpty").await,
                time_to_full: integer(&system, DISPLAY, "TimeToFull").await,
                health: health(&system).await,
            };
            battery.charge.set(charge);
            battery.alert(&charge);
            battery.listeners.notify();
        });
    }

    fn alert(&self, charge: &Charge) {
        let config = config::current();
        let alerts = Alerts::of(charge, Levels::of(&config));
        let previous_alerts = self.alerts.replace(alerts);
        let play_sound = |name: &str| {
            if config.sounds_battery {
                audio::play_system_sound(&config.sounds_theme, name);
            }
        };
        let notify = |summary: &str, body: &str, urgency: Urgency| {
            notify::send(&Notification {
                app: "Shell",
                summary,
                body,
                urgency,
                is_transient: true,
                ..Default::default()
            });
        };
        if alerts.is_low && !previous_alerts.is_low {
            notify(
                &tr("Low battery"),
                &tr("Consider plugging in your device"),
                Urgency::Critical,
            );
            play_sound("dialog-warning");
        }
        if alerts.is_critical && !previous_alerts.is_critical {
            let body = trf(
                "Please charge!\nAutomatic suspend triggers at %1%",
                &[&(config.battery_suspend * 100.0).round().to_string()],
            );
            notify(&tr("Critically low battery"), &body, Urgency::Critical);
            play_sound("suspend-error");
        }
        if alerts.should_suspend && !previous_alerts.should_suspend {
            detach(&["bash", "-c", "systemctl suspend || loginctl suspend"]);
        }
        if alerts.is_full && !previous_alerts.is_full {
            notify(
                &tr("Battery full"),
                &tr("Please unplug the charger"),
                Urgency::Normal,
            );
            play_sound("complete");
        }
        if let (Some(was_plugged), Some(is_plugged)) =
            (previous_alerts.is_plugged, alerts.is_plugged)
        {
            if was_plugged != is_plugged {
                play_sound(if is_plugged {
                    "power-plug"
                } else {
                    "power-unplug"
                });
                let target_profile = config::value_str(if is_plugged {
                    PROFILE_ON_CHARGER
                } else {
                    PROFILE_ON_BATTERY
                });
                if let (Some(system), Some(profile)) = (
                    &self.system,
                    target_profile.filter(|profile| !profile.is_empty()),
                ) {
                    power::set_active_profile(system, &profile);
                }
            }
        }
    }
}

async fn health(system: &gio::DBusConnection) -> f64 {
    let Ok(reply) = system
        .call_future(
            Some(BUS),
            ROOT,
            BUS,
            "EnumerateDevices",
            None,
            None,
            gio::DBusCallFlags::NONE,
            2000,
        )
        .await
    else {
        return 0.0;
    };
    for entry in reply.child_value(0).iter() {
        let Some(path) = entry.get::<glib::variant::ObjectPath>() else {
            continue;
        };
        let path = path.as_str();
        if dbus::u32_property(system, BUS, path, DEVICE, "Type").await != Some(BATTERY) {
            continue;
        }
        let capacity = number(system, path, "Capacity").await;
        if capacity > 0.0 {
            return capacity;
        }
    }
    0.0
}

async fn number(system: &gio::DBusConnection, path: &str, name: &str) -> f64 {
    dbus::property(system, BUS, path, DEVICE, name)
        .await
        .and_then(|value| value.get::<f64>())
        .unwrap_or(0.0)
}

async fn integer(system: &gio::DBusConnection, path: &str, name: &str) -> i64 {
    dbus::property(system, BUS, path, DEVICE, name)
        .await
        .and_then(|value| value.get::<i64>())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEVELS: Levels = Levels {
        low: 0.2,
        critical: 0.05,
        suspend: Some(0.03),
        full: 0.95,
    };

    fn charge(percentage: f64, state: u32) -> Charge {
        Charge {
            is_available: true,
            state,
            percentage,
            ..Charge::default()
        }
    }

    #[test]
    fn alerts_follow_the_level_and_the_charger() {
        let discharging_state = 2;
        assert_eq!(
            Alerts::of(&charge(0.04, discharging_state), LEVELS),
            Alerts {
                is_low: true,
                is_critical: true,
                should_suspend: false,
                is_full: false,
                is_plugged: Some(false),
            }
        );
        assert_eq!(
            Alerts::of(&charge(0.02, CHARGING), LEVELS),
            Alerts {
                is_plugged: Some(true),
                ..Alerts::default()
            }
        );
        assert_eq!(
            Alerts::of(&charge(0.97, CHARGING), LEVELS),
            Alerts {
                is_full: true,
                is_plugged: Some(true),
                ..Alerts::default()
            }
        );
        assert!(Alerts::of(&charge(0.02, discharging_state), LEVELS).should_suspend);
        let manual_levels = Levels {
            suspend: None,
            ..LEVELS
        };
        assert!(!Alerts::of(&charge(0.02, discharging_state), manual_levels).should_suspend);
        let absent_charge = Charge {
            is_available: false,
            ..charge(0.02, discharging_state)
        };
        assert_eq!(Alerts::of(&absent_charge, LEVELS), Alerts::default());
    }
}
