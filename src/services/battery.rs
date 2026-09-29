use gtk4::gio;
use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::i18n::{tr, trf};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::detach;
use crate::platform::dbus;
use crate::platform::notify::{self, Notification, Urgency};
use crate::services::audio;

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
    pub available: bool,
    pub state: u32,
    pub percentage: f64,
    pub rate: f64,
    pub time_to_empty: i64,
    pub time_to_full: i64,
    pub health: f64,
}

impl Charge {
    pub fn charging(&self) -> bool {
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
    low: bool,
    critical: bool,
    suspending: bool,
    full: bool,
    plugged: Option<bool>,
}

impl Alerts {
    fn of(charge: &Charge, levels: Levels) -> Self {
        if !charge.available {
            return Alerts::default();
        }
        let level = charge.percentage;
        let charging = charge.charging();
        Alerts {
            low: level <= levels.low && !charging,
            critical: level <= levels.critical && !charging,
            suspending: levels.suspend.is_some_and(|suspend| level <= suspend) && !charging,
            full: level >= levels.full && charging,
            plugged: Some(charging || charge.state == PENDING_CHARGE),
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
            let again = battery.clone();
            let subscription = dbus::on_properties_changed(system, BUS, move || again.refresh());
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
            let present = dbus::bool_property(&system, BUS, DISPLAY, DEVICE, "IsPresent")
                .await
                .unwrap_or(false);
            let charge = Charge {
                available: kind == BATTERY && present,
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
        let now = Alerts::of(charge, Levels::of(&config));
        let before = self.alerts.replace(now);
        let sound = |name: &str| {
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
                transient: true,
                ..Default::default()
            });
        };
        if now.low && !before.low {
            notify(
                &tr("Low battery"),
                &tr("Consider plugging in your device"),
                Urgency::Critical,
            );
            sound("dialog-warning");
        }
        if now.critical && !before.critical {
            let body = trf(
                "Please charge!\nAutomatic suspend triggers at %1%",
                &[&(config.battery_suspend * 100.0).round().to_string()],
            );
            notify(&tr("Critically low battery"), &body, Urgency::Critical);
            sound("suspend-error");
        }
        if now.suspending && !before.suspending {
            detach(&["bash", "-c", "systemctl suspend || loginctl suspend"]);
        }
        if now.full && !before.full {
            notify(
                &tr("Battery full"),
                &tr("Please unplug the charger"),
                Urgency::Normal,
            );
            sound("complete");
        }
        if let (Some(was), Some(plugged)) = (before.plugged, now.plugged) {
            if was != plugged {
                sound(if plugged {
                    "power-plug"
                } else {
                    "power-unplug"
                });
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
            available: true,
            state,
            percentage,
            ..Charge::default()
        }
    }

    #[test]
    fn alerts_follow_the_level_and_the_charger() {
        let discharging = 2;
        assert_eq!(
            Alerts::of(&charge(0.04, discharging), LEVELS),
            Alerts {
                low: true,
                critical: true,
                suspending: false,
                full: false,
                plugged: Some(false),
            }
        );
        assert_eq!(
            Alerts::of(&charge(0.02, CHARGING), LEVELS),
            Alerts {
                plugged: Some(true),
                ..Alerts::default()
            }
        );
        assert_eq!(
            Alerts::of(&charge(0.97, CHARGING), LEVELS),
            Alerts {
                full: true,
                plugged: Some(true),
                ..Alerts::default()
            }
        );
        assert!(Alerts::of(&charge(0.02, discharging), LEVELS).suspending);
        let manual = Levels {
            suspend: None,
            ..LEVELS
        };
        assert!(!Alerts::of(&charge(0.02, discharging), manual).suspending);
        let absent = Charge {
            available: false,
            ..charge(0.02, discharging)
        };
        assert_eq!(Alerts::of(&absent, LEVELS), Alerts::default());
    }
}
