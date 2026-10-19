pub mod files;
pub mod hardware;
pub mod hyprland;
pub mod media;
pub mod network;
pub mod overlay;
pub mod session;
pub mod updates;

use gtk4::gio;

use crate::core::config::Config;
use crate::platform::hyprland::hypr::Events;
use crate::services::files::cliphist::Cliphist;
use crate::services::files::wallpapers::Wallpapers;
use crate::services::hardware::battery::Battery;
use crate::services::hardware::brightness::Light;
use crate::services::hardware::power::Power;
use crate::services::hardware::sysinfo::Resources;
use crate::services::hardware::xkb::Xkb;
use crate::services::hyprland::fullscreen::Fullscreen;
use crate::services::hyprland::hyprstate::HyprState;
use crate::services::hyprland::screencolor::ScreenColor;
use crate::services::hyprland::workspacegroups::WorkspaceGroupWriter;
use crate::services::media::audio::Audio;
use crate::services::media::easyeffects::EasyEffects;
use crate::services::media::mpris::Mpris;
use crate::services::media::recording::Recording;
use crate::services::network::bluez::Bluez;
use crate::services::network::net::Net;
use crate::services::network::warp::Warp;
use crate::services::overlay::notifications::Notifications;
use crate::services::overlay::polkit::Polkit;
use crate::services::overlay::prompter::Prompter;
use crate::services::session::Session;
use crate::services::session::background::BackgroundTasks;
use crate::services::session::states::States;
use crate::services::session::timer::Timer;
use crate::services::session::todo::Todo;
use crate::services::session::weather::Weather;
use crate::services::updates::Updates;

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
    pub session: std::rc::Rc<Session>,
    pub battery: Battery,
    pub updates: Updates,
    pub notifications: Notifications,
    pub easyeffects: EasyEffects,
    pub warp: Warp,
    pub weather: std::rc::Rc<Weather>,
    pub todo: Todo,
    pub timer: Timer,
    pub mpris: Mpris,
    pub xkb: Xkb,
    pub recording: Recording,
    pub states: States,
    pub wallpapers: std::rc::Rc<Wallpapers>,
    pub polkit: std::rc::Rc<Polkit>,
    pub prompter: std::rc::Rc<Prompter>,
    _workspace_group_writer: std::rc::Rc<WorkspaceGroupWriter>,
}

impl Services {
    pub fn new(config: &Config) -> Self {
        let system = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok();
        let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok();
        let session_for_mpris = session.clone();
        let events = Events::default();
        let hypr = HyprState::new(&events);
        let states = States::new();
        let notifications = Notifications::new(config, session.clone(), &events);
        let color = ScreenColor::new(&events);
        let services = Services {
            background: BackgroundTasks::start(),
            resources: std::rc::Rc::new(Resources::default()),
            xkb: Xkb::new(&events),
            fullscreen: Fullscreen::new(&hypr),
            hypr,
            _workspace_group_writer: WorkspaceGroupWriter::new(&events),
            recording: Recording::new(),
            prompter: Prompter::new(session.clone(), &states),
            states,
            wallpapers: Wallpapers::new(),
            events,
            notifications,
            session_bus: session,
            audio: Audio::new(),
            light: Light::new(&color),
            cliphist: Cliphist::new(),
            net: Net::new(system.clone()),
            bluez: Bluez::new(system.clone()),
            power: Power::new(system.clone()),
            polkit: Polkit::new(system.clone()),
            session: Session::new(config, &color),
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
                if warp.is_available.get() {
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
