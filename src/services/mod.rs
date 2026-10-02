pub mod accounts;
pub mod appearance;
pub mod audio;
pub mod background;
pub mod battery;
pub mod bluez;
pub mod brightness;
pub mod cliphist;
pub mod deviceoptions;
pub mod displays;
pub mod easyeffects;
pub mod fullscreen;
pub mod hyproptions;
pub mod hyprstate;
pub mod idleoptions;
pub mod mpris;
pub mod net;
pub mod nmsettings;
pub mod notifications;
pub mod polkit;
pub mod power;
pub mod privacy;
pub mod recording;
pub mod session;
pub mod shellusage;
pub mod states;
pub mod sysinfo;
pub mod thumbnails;
pub mod timedate;
pub mod timer;
pub mod todo;
pub mod updates;
pub mod wallpapers;
pub mod warp;
pub mod weather;
pub mod wifi;
pub mod xkb;

use gtk4::gio;

use crate::core::config::Config;
use crate::platform::hypr::Events;
use crate::services::audio::Audio;
use crate::services::background::BackgroundTasks;
use crate::services::battery::Battery;
use crate::services::bluez::Bluez;
use crate::services::brightness::Light;
use crate::services::cliphist::Cliphist;
use crate::services::easyeffects::EasyEffects;
use crate::services::fullscreen::Fullscreen;
use crate::services::hyprstate::HyprState;
use crate::services::mpris::Mpris;
use crate::services::net::Net;
use crate::services::notifications::Notifications;
use crate::services::polkit::Polkit;
use crate::services::power::Power;
use crate::services::recording::Recording;
use crate::services::session::Session;
use crate::services::states::States;
use crate::services::sysinfo::Resources;
use crate::services::timer::Timer;
use crate::services::todo::Todo;
use crate::services::updates::Updates;
use crate::services::wallpapers::Wallpapers;
use crate::services::warp::Warp;
use crate::services::weather::Weather;
use crate::services::xkb::Xkb;

pub struct Services {
    pub background: std::rc::Rc<BackgroundTasks>,
    pub resources: std::rc::Rc<Resources>,
    pub events: Events,
    pub hypr: HyprState,
    pub fullscreen: Fullscreen,
    pub session_bus: Option<gio::DBusConnection>,
    pub audio: Option<Audio>,
    pub light: Light,
    pub cliphist: Cliphist,
    pub net: Net,
    pub bluez: Bluez,
    pub power: Power,
    pub session: Session,
    pub battery: Battery,
    pub updates: Updates,
    pub notifications: Notifications,
    pub easyeffects: EasyEffects,
    pub warp: Warp,
    pub weather: Weather,
    pub todo: Todo,
    pub timer: Timer,
    pub mpris: Mpris,
    pub xkb: Xkb,
    pub recording: Recording,
    pub states: States,
    pub wallpapers: std::rc::Rc<Wallpapers>,
    pub polkit: std::rc::Rc<Polkit>,
}

impl Services {
    pub fn new(config: &Config) -> Self {
        let system = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok();
        let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok();
        let session_for_mpris = session.clone();
        let events = Events::default();
        let hypr = HyprState::new(&events);
        let services = Services {
            background: BackgroundTasks::start(),
            resources: std::rc::Rc::new(Resources::default()),
            xkb: Xkb::new(&events),
            fullscreen: Fullscreen::new(&hypr),
            hypr,
            recording: Recording::new(),
            states: States::new(),
            wallpapers: Wallpapers::new(),
            events,
            notifications: Notifications::new(config, session.clone()),
            session_bus: session,
            audio: Audio::new(),
            light: Light::new(),
            cliphist: Cliphist::new(),
            net: Net::new(system.clone()),
            bluez: Bluez::new(system.clone()),
            power: Power::new(system.clone()),
            polkit: Polkit::new(system.clone()),
            session: Session::new(config),
            weather: Weather::new(system.clone()),
            battery: Battery::new(system),
            updates: Updates::new(),
            easyeffects: EasyEffects::new(),
            warp: Warp::new(),
            todo: Todo::new(),
            timer: Timer::new(),
            mpris: Mpris::new(session_for_mpris),
        };
        services.register_tasks();
        services
    }

    fn register_tasks(&self) {
        let background = &self.background;
        background.add("resources", {
            let resources = self.resources.clone();
            move || resources.sample()
        });
        background.add("media position", {
            let mpris = self.mpris.clone();
            move || {
                mpris.follow_position();
                Ok(())
            }
        });
        background.add("recording", {
            let recording = self.recording.clone();
            move || {
                recording.check_idle();
                Ok(())
            }
        });
        background.add("night light schedule", {
            let session = self.session.clone();
            move || {
                session.follow_clock();
                Ok(())
            }
        });
        background.add("warp", {
            let warp = self.warp.clone();
            move || {
                if warp.available.get() {
                    warp.refresh();
                }
                Ok(())
            }
        });
        background.add_every("weather", Weather::period, {
            let weather = self.weather.clone();
            move || {
                if crate::core::config::current().weather_enable {
                    weather.fetch();
                }
                Ok(())
            }
        });
        background.add_every("updates", Updates::period, {
            let updates = self.updates.clone();
            move || {
                if crate::core::config::current().updates_enable_check {
                    updates.refresh();
                }
                Ok(())
            }
        });
    }
}
