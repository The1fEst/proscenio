use gtk4::gio;
use gtk4::glib;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::sun::{self, Date, Daylight, Position};
use crate::core::{config, persistent, watch};
use crate::platform::dbus::geoclue;

const CACHED_POSITION: [&str; 1] = ["location"];

pub enum LocationUpdate {
    Found,
    Failed { had_position: bool },
}

pub struct Location {
    live_position: Cell<Option<Position>>,
    cached_position: Cell<Option<Position>>,
    system: Option<gio::DBusConnection>,
    client: RefCell<Option<geoclue::LocationClientSlot>>,
    listeners: Listeners<LocationUpdate>,
    watches: RefCell<Vec<watch::Watch>>,
}

impl Location {
    pub fn new(system: Option<gio::DBusConnection>) -> Rc<Self> {
        let location = Rc::new(Location {
            live_position: Cell::new(None),
            cached_position: Cell::new(read_cached_position()),
            system,
            client: RefCell::new(None),
            listeners: Listeners::default(),
            watches: RefCell::default(),
        });
        location.follow_demand();
        let watches = [
            "/light/night",
            "/appearance/darkMode",
            "/bar/weather/enable",
            "/bar/weather/enableGPS",
        ]
        .map(|pointer| {
            let location = Rc::downgrade(&location);
            watch::config(pointer, move || {
                if let Some(location) = location.upgrade() {
                    location.follow_demand();
                }
            })
        });
        location.watches.replace(watches.into());
        location
    }

    pub fn live_position(&self) -> Option<Position> {
        self.live_position.get()
    }

    pub fn subscribe(&self, listener: impl Fn(&LocationUpdate) + 'static) -> Subscription {
        self.listeners.add_with(listener)
    }

    pub fn compute_daylight_today(&self) -> Option<Daylight> {
        let position = self.live_position.get().or(self.cached_position.get())?;
        let now = glib::DateTime::now_local().ok()?;
        let date = Date {
            year: now.year(),
            month: now.month() as u32,
            day: now.day_of_month() as u32,
        };
        let utc_offset_minutes = now.utc_offset().as_minutes() as i32;
        Some(sun::compute_daylight(date, utc_offset_minutes, position))
    }

    fn follow_demand(self: &Rc<Self>) {
        let Some(system) = self
            .system
            .clone()
            .filter(|_| config::current().needs_position())
        else {
            self.release_client();
            return;
        };
        if self.client.borrow().is_some() {
            return;
        }
        let slot = geoclue::LocationClientSlot::default();
        self.client.replace(Some(slot.clone()));
        geoclue::follow(
            system,
            slot.clone(),
            {
                let location = Rc::downgrade(self);
                let slot = Rc::downgrade(&slot);
                move |latitude_degrees, longitude_degrees| {
                    let Some(location) = location.upgrade() else {
                        return;
                    };
                    if !location.is_current_client(&slot) {
                        return;
                    }
                    let position = Position {
                        latitude_degrees,
                        longitude_degrees,
                    };
                    location.live_position.set(Some(position));
                    if location.cached_position.replace(Some(position)) != Some(position) {
                        write_cached_position(position);
                    }
                    location.listeners.notify_with(&LocationUpdate::Found);
                }
            },
            {
                let location = Rc::downgrade(self);
                let slot = Rc::downgrade(&slot);
                move || {
                    let Some(location) = location.upgrade() else {
                        return;
                    };
                    if !location.is_current_client(&slot) {
                        return;
                    }
                    location.client.take();
                    let had_position = location.live_position.replace(None).is_some();
                    location
                        .listeners
                        .notify_with(&LocationUpdate::Failed { had_position });
                }
            },
        );
    }

    fn is_current_client(&self, slot: &geoclue::WeakLocationClientSlot) -> bool {
        self.client
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::as_ptr(current) == slot.as_ptr())
    }

    fn release_client(&self) {
        self.live_position.set(None);
        let Some(slot) = self.client.take() else {
            return;
        };
        if let Some(client) = slot.take()
            && let Some(system) = self.system.clone()
        {
            geoclue::release(system, client.client_path);
        }
    }
}

impl Drop for Location {
    fn drop(&mut self) {
        self.release_client();
    }
}

fn read_cached_position() -> Option<Position> {
    let cached = persistent::read(&CACHED_POSITION)?;
    Some(Position {
        latitude_degrees: cached.get("latitude")?.as_f64()?,
        longitude_degrees: cached.get("longitude")?.as_f64()?,
    })
}

fn write_cached_position(position: Position) {
    let cached: Value = json!({
        "latitude": position.latitude_degrees,
        "longitude": position.longitude_degrees,
    });
    persistent::write(&CACHED_POSITION, cached);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_newest_location_request_counts() {
        let location = Location::new(None);
        let older_slot = geoclue::LocationClientSlot::default();
        let newer_slot = geoclue::LocationClientSlot::default();
        location.client.replace(Some(newer_slot.clone()));
        assert!(location.is_current_client(&Rc::downgrade(&newer_slot)));
        assert!(!location.is_current_client(&Rc::downgrade(&older_slot)));
        location.release_client();
        assert!(!location.is_current_client(&Rc::downgrade(&newer_slot)));
    }
}
