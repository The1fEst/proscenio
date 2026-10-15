use gtk4::gio;
use gtk4::glib;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::{config, process, watch};
use crate::platform::dbus::geoclue;
use crate::platform::dbus::notify::{self, Notification};

#[derive(Clone, Default)]
pub struct Report {
    pub uv: String,
    pub humidity: String,
    pub sunrise: String,
    pub sunset: String,
    pub wind_direction: String,
    pub code: String,
    pub city: String,
    pub wind: String,
    pub precipitation: String,
    pub visibility: String,
    pub pressure: String,
    pub temperature: String,
    pub feels_like: String,
    pub refresh_time: String,
}

pub struct Weather {
    pub report: RefCell<Report>,
    position: Cell<Option<(f64, f64)>>,
    location: RefCell<Option<geoclue::LocationClientSlot>>,
    system: Option<gio::DBusConnection>,
    listeners: Listeners,
    config_watches: RefCell<Vec<watch::Watch>>,
    fetch_generation: Cell<u64>,
}

impl Weather {
    pub fn new(system: Option<gio::DBusConnection>) -> Rc<Self> {
        let weather = Rc::new(Weather {
            report: RefCell::default(),
            position: Cell::default(),
            location: RefCell::new(None),
            system,
            listeners: Listeners::default(),
            config_watches: RefCell::default(),
            fetch_generation: Cell::new(0),
        });
        weather.locate();
        let watches = [
            ("/bar/weather/enable", true),
            ("/bar/weather/enableGPS", true),
            ("/bar/weather/city", false),
            ("/bar/weather/useUSCS", false),
        ]
        .map(|(pointer, should_relocate)| {
            let weather = Rc::downgrade(&weather);
            watch::config(pointer, move || {
                let Some(weather) = weather.upgrade() else {
                    return;
                };
                if should_relocate {
                    weather.locate();
                }
                if config::current().weather_enable {
                    weather.fetch();
                }
            })
        });
        weather.config_watches.replace(watches.into());
        weather
    }

    fn locate(self: &Rc<Self>) {
        let config = config::current();
        let Some(system) = self
            .system
            .clone()
            .filter(|_| config.weather_enable && config.weather_gps)
        else {
            self.stop_locating();
            self.position.set(None);
            return;
        };
        if self.location.borrow().is_some() {
            return;
        }
        let slot = geoclue::LocationClientSlot::default();
        self.location.replace(Some(slot.clone()));
        geoclue::follow(
            system,
            slot.clone(),
            {
                let weather = Rc::downgrade(self);
                let slot = Rc::downgrade(&slot);
                move |latitude, longitude| {
                    let Some(weather) = weather.upgrade() else {
                        return;
                    };
                    if !weather.is_current_location(&slot) {
                        return;
                    }
                    weather.position.set(Some((latitude, longitude)));
                    weather.fetch();
                }
            },
            {
                let weather = Rc::downgrade(self);
                let slot = Rc::downgrade(&slot);
                move || {
                    let Some(weather) = weather.upgrade() else {
                        return;
                    };
                    if !weather.is_current_location(&slot) {
                        return;
                    }
                    weather.location.take();
                    let had_position = weather.position.replace(None).is_some();
                    notify::send(&Notification {
                        app: "Shell",
                        summary: &tr("Weather Service"),
                        body: &tr("Cannot find a GPS service. Using the fallback method instead."),
                        ..Default::default()
                    });
                    if had_position {
                        weather.fetch();
                    }
                }
            },
        );
    }

    fn is_current_location(&self, slot: &geoclue::WeakLocationClientSlot) -> bool {
        self.location
            .borrow()
            .as_ref()
            .is_some_and(|current| Rc::as_ptr(current) == slot.as_ptr())
    }

    fn stop_locating(&self) {
        let Some(slot) = self.location.take() else {
            return;
        };
        if let Some(client) = slot.take()
            && let Some(system) = self.system.clone()
        {
            geoclue::release(system, client.client_path);
        }
    }

    pub fn period() -> Duration {
        let config = config::current();
        if !config.weather_enable {
            return Duration::ZERO;
        }
        Duration::from_secs((config.weather_interval as u64).max(1) * 60)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn fetch(self: &Rc<Self>) {
        let weather = Rc::downgrade(self);
        let position = self.position.get();
        let generation = self.start_fetch();
        glib::spawn_future_local(async move {
            if weather.upgrade().is_none() {
                return;
            }
            let place = match position {
                Some((latitude, longitude)) => format!("{latitude},{longitude}"),
                None => config::current()
                    .weather_city
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join("+"),
            };
            let url = format!("https://wttr.in/{place}?format=j1");
            let Some(body) = run(&["curl", "-s", &url]).await else {
                return;
            };
            let Some(weather) = weather.upgrade() else {
                return;
            };
            let Ok(response) = serde_json::from_str::<Value>(&body) else {
                return;
            };
            weather.apply_report(generation, &response);
        });
    }

    fn start_fetch(&self) -> u64 {
        let generation = self.fetch_generation.get() + 1;
        self.fetch_generation.set(generation);
        generation
    }

    fn apply_report(&self, generation: u64, response: &Value) {
        if generation != self.fetch_generation.get() {
            return;
        }
        self.report.replace(self.refine(response));
        self.listeners.notify();
    }

    fn refine(&self, response: &Value) -> Report {
        let current = response.pointer("/current_condition/0");
        let area = response.pointer("/nearest_area/0");
        let sky = response.pointer("/weather/0/astronomy/0");
        let read = |node: Option<&Value>, key: &str, fallback: &str| {
            node.and_then(|node| node.get(key))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .unwrap_or(fallback)
                .to_owned()
        };

        let uses_uscs = config::current().weather_uscs;
        let (wind, precipitation, visibility, pressure, temperature, feels_like) = if uses_uscs {
            (
                format!("{} mph", read(current, "windspeedMiles", "0")),
                format!("{} in", read(current, "precipInches", "0")),
                format!("{} mi", read(current, "visibilityMiles", "0")),
                format!("{} inHg", read(current, "pressureInches", "0")),
                format!("{}°F", read(current, "temp_F", "0")),
                format!("{}°F", read(current, "FeelsLikeF", "0")),
            )
        } else {
            (
                format!("{} km/h", read(current, "windspeedKmph", "0")),
                format!("{} mm", read(current, "precipMM", "0")),
                format!("{} km", read(current, "visibility", "0")),
                format!("{} hPa", read(current, "pressure", "0")),
                format!("{}°C", read(current, "temp_C", "0")),
                format!("{}°C", read(current, "FeelsLikeC", "0")),
            )
        };

        Report {
            uv: read(current, "uvIndex", "0"),
            humidity: format!("{}%", read(current, "humidity", "0")),
            sunrise: read(sky, "sunrise", "0.0"),
            sunset: read(sky, "sunset", "0.0"),
            wind_direction: read(current, "winddir16Point", "N"),
            code: read(current, "weatherCode", "113"),
            city: area
                .and_then(|node| node.pointer("/areaName/0/value"))
                .and_then(Value::as_str)
                .map(str::to_owned)
                .unwrap_or_else(|| tr("City")),
            wind,
            precipitation,
            visibility,
            pressure,
            temperature,
            feels_like,
            refresh_time: glib::DateTime::now_local()
                .ok()
                .and_then(|now| now.format(&stamp_format()).ok())
                .map(Into::into)
                .unwrap_or_default(),
        }
    }
}

impl Drop for Weather {
    fn drop(&mut self) {
        self.stop_locating();
    }
}

pub fn symbol(code: &str) -> &'static str {
    match code {
        "113" => "clear_day",
        "116" => "partly_cloudy_day",
        "119" | "122" => "cloud",
        "143" | "248" | "260" => "foggy",
        "200" | "386" | "389" | "392" => "thunderstorm",
        "227" | "320" | "323" | "326" | "368" => "cloudy_snowing",
        "230" | "329" | "332" | "338" => "snowing_heavy",
        "335" | "371" | "395" => "snowing",
        "302" | "308" | "359" => "weather_hail",
        "176" | "179" | "182" | "185" | "263" | "266" | "281" | "284" | "293" | "296" | "299"
        | "305" | "311" | "314" | "317" | "350" | "353" | "356" | "362" | "365" | "374" | "377" => {
            "rainy"
        }
        _ => "cloud",
    }
}

fn stamp_format() -> String {
    let config = config::current();
    format!(
        "{} • {}",
        crate::panels::bar::clock::strftime_from_qt(&config.time_format),
        crate::panels::bar::clock::strftime_from_qt(&config.date_with_year_format),
    )
}

async fn run(line: &[&str]) -> Option<String> {
    process::capture_text(process::command(line))
        .await
        .filter(|text| !text.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dropped_weather_service_is_freed() {
        let weather = Weather::new(None);
        let weak_weather = Rc::downgrade(&weather);
        drop(weather);
        assert!(weak_weather.upgrade().is_none());
    }

    #[test]
    fn only_the_newest_location_request_counts() {
        let weather = Weather::new(None);
        let older_slot = geoclue::LocationClientSlot::default();
        let newer_slot = geoclue::LocationClientSlot::default();
        weather.location.replace(Some(newer_slot.clone()));
        assert!(weather.is_current_location(&Rc::downgrade(&newer_slot)));
        assert!(!weather.is_current_location(&Rc::downgrade(&older_slot)));
        weather.stop_locating();
        assert!(!weather.is_current_location(&Rc::downgrade(&newer_slot)));
    }

    #[test]
    fn only_the_report_of_the_newest_fetch_is_shown() {
        let weather = Weather::new(None);
        let response =
            |city: &str| serde_json::json!({"nearest_area": [{"areaName": [{"value": city}]}]});
        let older = weather.start_fetch();
        let newer = weather.start_fetch();
        weather.apply_report(newer, &response("Coordinates"));
        weather.apply_report(older, &response("City"));
        assert_eq!(weather.report.borrow().city, "Coordinates");

        weather.start_fetch();
        weather.apply_report(newer, &response("Before the change"));
        assert_eq!(weather.report.borrow().city, "Coordinates");
    }
}
