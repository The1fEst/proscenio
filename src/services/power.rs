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
            let again = power.clone();
            let subscription = dbus::on_properties_changed(system, BUS, move || again.refresh());
            std::mem::forget(subscription);
        }
        power
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn cycle(&self) {
        let next = match (self.profile.borrow().as_str(), self.has_performance.get()) {
            ("power-saver", _) => "balanced",
            ("balanced", true) => "performance",
            ("balanced", false) => "power-saver",
            _ => "power-saver",
        };
        if let Some(system) = &self.system {
            set_active_profile(system, next);
        }
    }

    fn refresh(&self) {
        let Some(system) = self.system.clone() else {
            return;
        };
        let power = self.clone();
        glib::spawn_future_local(async move {
            if let Some(active) =
                dbus::string_property(&system, BUS, PATH, BUS, "ActiveProfile").await
            {
                power.profile.replace(active);
            }
            if let Some(list) = dbus::property(&system, BUS, PATH, BUS, "Profiles").await {
                power.has_performance.set(list.iter().any(|entry| {
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
    let request = set_profile(profile);
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

fn set_profile(profile: &str) -> glib::Variant {
    (BUS, "ActiveProfile", profile.to_variant()).to_variant()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_profile_travels_as_a_plain_string_inside_the_value() {
        let request = set_profile("performance");
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
