use gtk4::gio;
use gtk4::glib;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::config::{self, ScheduleMode};
use crate::core::sun::{self, Date, Daylight, Position};
use crate::core::{persistent, watch};
use crate::platform::dbus::geoclue;

const CACHED_POSITION: [&str; 1] = ["location"];

pub struct Location {
    position: Cell<Option<Position>>,
    system: Option<gio::DBusConnection>,
    client: RefCell<Option<geoclue::LocationClientSlot>>,
    watches: RefCell<Vec<watch::Watch>>,
}

impl Location {
    pub fn new(system: Option<gio::DBusConnection>) -> Rc<Self> {
        let location = Rc::new(Location {
            position: Cell::new(read_cached_position()),
            system,
            client: RefCell::new(None),
            watches: RefCell::default(),
        });
        location.follow_demand();
        let watches = ["/light/night", "/appearance/darkMode"].map(|pointer| {
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

    pub fn compute_daylight_today(&self) -> Option<Daylight> {
        let position = self.position.get()?;
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
        let config = config::current();
        let is_needed = (config.night_automatic && config.night_schedule == ScheduleMode::Sun)
            || (config.is_dark_mode_automatic && config.dark_mode_schedule == ScheduleMode::Sun);
        let Some(system) = self.system.clone().filter(|_| is_needed) else {
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
                    if location.position.replace(Some(position)) != Some(position) {
                        write_cached_position(position);
                    }
                }
            },
            {
                let location = Rc::downgrade(self);
                let slot = Rc::downgrade(&slot);
                move || {
                    let Some(location) = location.upgrade() else {
                        return;
                    };
                    if location.is_current_client(&slot) {
                        location.client.take();
                    }
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
