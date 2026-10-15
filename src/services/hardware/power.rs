use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::dbus;

const BUS: &str = "net.hadess.PowerProfiles";
const PATH: &str = "/net/hadess/PowerProfiles";

#[derive(Clone)]
pub struct Power {
    pub profile: Rc<RefCell<String>>,
    pub has_performance: Rc<Cell<bool>>,
    system: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
}

impl Power {
    pub fn new(system: Option<gio::DBusConnection>) -> Self {
        let power = Power {
            profile: Rc::new(RefCell::new("balanced".to_owned())),
            has_performance: Rc::new(Cell::new(false)),
            system,
            listeners: Rc::default(),
        };
        power.refresh();
        if let Some(system) = &power.system {
            let subscription = dbus::subscribe_properties_changed(system, BUS, {
                let power = power.clone();
                move || power.refresh()
            });
            std::mem::forget(subscription);
        }
        power
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn cycle(&self) {
        let next_profile = match (self.profile.borrow().as_str(), self.has_performance.get()) {
            ("power-saver", _) => "balanced",
            ("balanced", true) => "performance",
            ("balanced", false) => "power-saver",
            _ => "power-saver",
        };
        if let Some(system) = &self.system {
            set_active_profile(system, next_profile);
        }
    }

    fn refresh(&self) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let power = self.clone();
        glib::spawn_future_local(async move {
            if let Some(active_profile) =
                dbus::string_property(&system, BUS, PATH, BUS, "ActiveProfile").await
            {
                power.profile.replace(active_profile);
            }
            if let Some(profiles) = dbus::property(&system, BUS, PATH, BUS, "Profiles").await {
                power.has_performance.set(profiles.iter().any(|entry| {
                    entry.iter().any(|pair| {
                        pair.child_value(0).str() == Some("Profile")
                            && pair
                                .child_value(1)
                                .as_variant()
                                .and_then(|value| value.str().map(|name| name == "performance"))
                                == Some(true)
                    })
                }));
            }
            power.listeners.notify();
        });
    }
}

pub fn set_active_profile(system: &gio::DBusConnection, profile: &str) {
    let system = system.clone();
    let request = build_profile_request(profile);
    glib::spawn_future_local(async move {
        let _ = system
            .call_future(
                Some(BUS),
                PATH,
                "org.freedesktop.DBus.Properties",
                "Set",
                Some(&request),
                None,
                gio::DBusCallFlags::NONE,
                2000,
            )
            .await;
    });
}

fn build_profile_request(profile: &str) -> glib::Variant {
    (BUS, "ActiveProfile", profile.to_variant()).to_variant()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_travels_as_a_plain_string_inside_the_value() {
        let request = build_profile_request("performance");
        assert_eq!(request.type_().as_str(), "(ssv)");
        assert_eq!(
            request
                .child_value(2)
                .as_variant()
                .and_then(|value| value.str().map(str::to_owned)),
            Some("performance".to_owned())
        );
    }
}
