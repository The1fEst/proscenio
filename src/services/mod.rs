pub mod files;
pub mod hardware;
pub mod hyprland;
pub mod ipctargets;
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
use crate::services::hyprland::cursorhider::CursorHider;
use crate::services::hyprland::fullscreen::Fullscreen;
use crate::services::hyprland::hyprlockclock::HyprlockClockWriter;
use crate::services::hyprland::hyprstate::HyprState;
use crate::services::hyprland::screencolor::ScreenColor;
use crate::services::hyprland::urgentwindows::UrgentWindows;
use crate::services::hyprland::workspacegroups::WorkspaceGroupWriter;
use crate::services::media::audio::Audio;
use crate::services::media::deviceusage::DeviceUsage;
use crate::services::media::easyeffects::EasyEffects;
use crate::services::media::mpris::Mpris;
use crate::services::media::recording::Recording;
use crate::services::network::bluez::Bluez;
use crate::services::network::bluezagent::BluezAgent;
use crate::services::network::bluezplayer::BluezPlayer;
use crate::services::network::net::Net;
use crate::services::network::warp::Warp;
use crate::services::overlay::notifications::Notifications;
use crate::services::overlay::polkit::Polkit;
use crate::services::overlay::prompter::Prompter;
use crate::services::session::Session;
use crate::services::session::agenda::Agenda;
use crate::services::session::background::BackgroundTasks;
use crate::services::session::darkmode::DarkModeSchedule;
use crate::services::session::location::Location;
use crate::services::session::notes::Notes;
use crate::services::session::states::States;
use crate::services::session::timer::Timer;
use crate::services::session::weather::Weather;
use crate::services::updates::Updates;

pub struct Services {
    pub background: std::rc::Rc<BackgroundTasks>,
    pub resources: std::rc::Rc<Resources>,
    pub events: Events,
    pub hypr: HyprState,
    pub fullscreen: Fullscreen,
    pub urgent_windows: UrgentWindows,
    pub cursor_hider: CursorHider,
    pub session_bus: Option<gio::DBusConnection>,
    pub audio: Option<Audio>,
    pub device_usage: DeviceUsage,
    pub light: Light,
    pub cliphist: Cliphist,
    pub net: Net,
    pub bluez: Bluez,
    pub bluez_agent: std::rc::Rc<BluezAgent>,
    pub power: Power,
    pub session: std::rc::Rc<Session>,
    pub battery: Battery,
    pub updates: Updates,
    pub notifications: Notifications,
    pub easyeffects: EasyEffects,
    pub warp: Warp,
    pub weather: std::rc::Rc<Weather>,
    pub notes: std::rc::Rc<Notes>,
    pub agenda: std::rc::Rc<Agenda>,
    pub timer: Timer,
    pub mpris: Mpris,
    pub xkb: Xkb,
    pub recording: Recording,
    pub states: States,
    pub wallpapers: std::rc::Rc<Wallpapers>,
    pub polkit: std::rc::Rc<Polkit>,
    pub prompter: std::rc::Rc<Prompter>,
    dark_mode_schedule: std::rc::Rc<DarkModeSchedule>,
    _workspace_group_writer: std::rc::Rc<WorkspaceGroupWriter>,
    _hyprlock_clock_writer: HyprlockClockWriter,
    _bluez_player: Option<std::rc::Rc<BluezPlayer>>,
}

impl Services {
    pub fn new(config: &Config) -> Self {
        let system = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok();
        let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok();
        let mpris = Mpris::new(session.clone());
        let session_for_agenda = session.clone();
        let events = Events::default();
        let hypr = HyprState::new(&events);
        let states = States::new();
        let audio = Audio::new();
        let device_usage = DeviceUsage::new(&events, &hypr, audio.clone());
        let notifications =
            Notifications::new(config, session.clone(), system.as_ref(), &device_usage);
        let color = ScreenColor::new(&events);
        let location = Location::new(system.clone());
        let desktop_session = Session::new(config, &color, &location, system.as_ref());
        let bluez = Bluez::new(system.clone());
        let services = Services {
            bluez_agent: BluezAgent::new(system.clone(), &bluez),
            _bluez_player: BluezPlayer::new(system.clone(), &bluez, &mpris),
            dark_mode_schedule: DarkModeSchedule::new(&desktop_session, &location),
            background: BackgroundTasks::start(),
            resources: std::rc::Rc::new(Resources::default()),
            xkb: Xkb::new(&events),
            fullscreen: Fullscreen::new(&hypr),
            urgent_windows: UrgentWindows::new(&events),
            cursor_hider: CursorHider::new(&events, &hypr),
            hypr,
            _workspace_group_writer: WorkspaceGroupWriter::new(&events),
            _hyprlock_clock_writer: HyprlockClockWriter::start(),
            recording: Recording::new(),
            prompter: Prompter::new(session.clone(), &states),
            states,
            wallpapers: Wallpapers::new(),
            events,
            notifications,
            session_bus: session,
            audio,
            device_usage,
            light: Light::new(&color, system.clone()),
            cliphist: Cliphist::new(),
            net: Net::new(),
            bluez,
            power: Power::new(system.clone()),
            polkit: Polkit::new(system.clone()),
            session: desktop_session,
            weather: Weather::new(&location),
            battery: Battery::new(system),
            updates: Updates::new(),
            easyeffects: EasyEffects::new(),
            warp: Warp::new(),
            notes: Notes::new(),
            agenda: Agenda::new(session_for_agenda),
            timer: Timer::new(),
            mpris,
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
        background.add("camera usage", {
            let device_usage = self.device_usage.clone();
            move || {
                device_usage.scan_cameras();
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
        background.add("dark mode schedule", {
            let dark_mode_schedule = self.dark_mode_schedule.clone();
            move || {
                dark_mode_schedule.follow_clock();
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
        if self.light.has_keyboard_backlight() {
            background.add("keyboard backlight", {
                let light = self.light.clone();
                move || {
                    light.follow_keyboard_backlight();
                    Ok(())
                }
            });
        }
        background.add_every("weather", Weather::period, {
            let weather = self.weather.clone();
            move || {
                weather.fetch();
                Ok(())
            }
        });
        background.add_every("updates", Updates::period, {
            let updates = self.updates.clone();
            move || {
                updates.refresh();
                Ok(())
            }
        });
    }
}
