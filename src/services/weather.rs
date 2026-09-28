use gtk4::gio;
use gtk4::glib;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::{config, process, watch};
use crate::platform::geoclue;
use crate::platform::notify::{self, Notification};

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
    pub refreshed: String,
}

#[derive(Clone)]
pub struct Weather {
    pub data: Rc<RefCell<Report>>,
    position: Rc<Cell<Option<(f64, f64)>>>,
    location: geoclue::Held,
    system: Option<gio::DBusConnection>,
    listeners: Rc<Listeners>,
    following: Rc<RefCell<Vec<watch::Watch>>>,
}

impl Weather {
    pub fn new(system: Option<gio::DBusConnection>) -> Self {
        let weather = Weather {
            data: Rc::new(RefCell::new(Report::default())),
            position: Rc::default(),
            location: Rc::default(),
            system,
            listeners: Rc::default(),
            following: Rc::default(),
        };
        weather.locate();
        let follows = [
            ("/bar/weather/enable", true),
            ("/bar/weather/enableGPS", true),
            ("/bar/weather/city", false),
            ("/bar/weather/useUSCS", false),
        ]
        .map(|(pointer, relocate)| {
            let weather = weather.clone();
            watch::config(pointer, move || {
                if relocate {
                    weather.locate();
                }
                if config::current().weather_enable {
                    weather.fetch();
                }
            })
        });
        weather.following.replace(follows.into());
        weather
    }

    fn locate(&self) {
        let config = config::current();
        let Some(system) = self
            .system
            .clone()
            .filter(|_| config.weather_enable && config.weather_gps)
        else {
            self.location.take();
            self.position.set(None);
            return;
        };
        if self.location.borrow().is_some() {
            return;
        }
        let found = self.clone();
        geoclue::follow(
            system,
            self.location.clone(),
            move |latitude, longitude| {
                found.position.set(Some((latitude, longitude)));
                found.fetch();
            },
            || {
                notify::send(&Notification {
                    app: "Shell",
                    summary: "Weather Service",
                    body: "Cannot find a GPS service. Using the fallback method instead.",
                    ..Default::default()
                });
            },
        );
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

    pub fn fetch(&self) {
        let weather = self.clone();
        glib::spawn_future_local(async move {
            let place = match weather.position.get() {
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
            let Ok(parsed) = serde_json::from_str::<Value>(&body) else {
                return;
            };
            weather.data.replace(weather.refine(&parsed));
            weather.listeners.notify();
        });
    }

    fn refine(&self, parsed: &Value) -> Report {
        let current = parsed.pointer("/current_condition/0");
        let area = parsed.pointer("/nearest_area/0");
        let sky = parsed.pointer("/weather/0/astronomy/0");
        let read = |node: Option<&Value>, key: &str, fallback: &str| {
            node.and_then(|node| node.get(key))
                .and_then(Value::as_str)
                .filter(|text| !text.is_empty())
                .unwrap_or(fallback)
                .to_owned()
        };

        let uscs = config::current().weather_uscs;
        let (wind, precipitation, visibility, pressure, temperature, feels_like) = if uscs {
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
                .unwrap_or("City")
                .to_owned(),
            wind,
            precipitation,
            visibility,
            pressure,
            temperature,
            feels_like,
            refreshed: glib::DateTime::now_local()
                .ok()
                .and_then(|now| now.format(&stamp_format()).ok())
                .map(Into::into)
                .unwrap_or_default(),
        }
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
