mod barwindow;
mod core;
mod panels;
mod platform;
mod screens;
mod services;
mod theming;
mod ui;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::core::actions;
use crate::core::config::Config;
use crate::core::paths;
use crate::panels::overview::launcher;
use crate::panels::{
    bluetoothagent, conflicts, displayconfirm, keyringprompt, lock, osd, osk, polkit,
    regionselector, renderercheck, settings, wallpaperselector, welcome,
};
use crate::platform::dbus::ipc;
use crate::platform::hyprland::hypr;
use crate::platform::system::{locale, readiness};
use crate::platform::wayland::shortcuts;
use crate::screens::{Screens, Surfaces};
use crate::services::Services;
use crate::services::hardware::devicenotifications::DeviceNotifications;
use crate::services::ipctargets;
use crate::services::media::{mpris, recording};
use crate::services::overlay::notifications;
use crate::services::overlay::statusnotifierwatcher::{self, StatusNotifierWatcher};
use crate::services::session::{deadlineipc, states};
use crate::theming::{colors, switchwall};
use crate::ui::theme::{self, Theme};
use crate::ui::{unload, widgets};

const APP_ID: &str = "dev.fEst.Proscenio";

fn main() -> glib::ExitCode {
    unload::single_arena();
    let arguments: Vec<String> = std::env::args().collect();
    let command = arguments.get(1).map(String::as_str);
    if command == Some(locale::COMMAND) {
        return locale::run(&arguments[2..]);
    }
    if command == Some(platform::network::firewall::COMMAND) {
        return platform::network::firewall::run(&arguments[2..]);
    }
    if command == Some(platform::dbus::powersettings::COMMAND) {
        return platform::dbus::powersettings::run(&arguments[2..]);
    }
    if std::env::var_os("PKEXEC_UID").is_some() {
        return glib::ExitCode::FAILURE;
    }
    if command == Some("ipc") {
        return ipc::client(APP_ID, &arguments[2..]);
    }
    paths::prepare();
    match command {
        Some("colors") => return colors::run(&arguments[2..]),
        Some("switchwall") => return switchwall::run(&arguments[2..]),
        Some("record") => return recording::run(&arguments[2..]),
        Some("screenshot-notify") => return regionselector::notification::run(&arguments[2..]),
        Some(renderercheck::COMMAND) => return renderercheck::run(APP_ID),
        _ => {}
    }
    let sets_renderer_variable = crate::core::config::choose_renderer();
    widgets::display::text::disable_symbol_hinting();
    let app = gtk4::Application::builder().application_id(APP_ID).build();
    let ipc = Rc::new(ipc::Ipc::default());
    let is_activated = Cell::new(false);
    app.connect_activate(move |app| {
        if is_activated.replace(true) {
            return;
        }
        if let Some(connection) = app.dbus_connection() {
            ipc.export(&connection);
        }
        if sets_renderer_variable {
            renderercheck::start_if_on_trial();
        }
        build(app, &ipc);
        if sets_renderer_variable {
            unsafe { std::env::remove_var("GSK_RENDERER") };
        }
    });
    app.run_with_args(&arguments[..1])
}

fn build(app: &gtk4::Application, ipc: &Rc<ipc::Ipc>) {
    std::mem::forget(app.hold());
    let config = Rc::new(Config::load());
    widgets::display::text::init(&config);
    let theme = Rc::new(RefCell::new(Theme::load(&config)));

    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&theme.borrow().css());
    let display = gdk::Display::default().expect("no Wayland display");
    let gtk_settings = gtk4::Settings::for_display(&display);
    gtk_settings.set_gtk_icon_theme_name(Some(&widgets::display::text::icon_theme()));
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_USER + 1,
    );
    let theme_watch = theme::watch(&theme, &provider, app);

    let _ = settings::pages::migrate_settings_files();
    let services = Rc::new(Services::new(&config));
    std::mem::forget(StatusNotifierWatcher::start(services.session_bus.as_ref()));
    readiness::report_when_ready(
        services.session_bus.as_ref(),
        &[
            notifications::NAME,
            statusnotifierwatcher::NAME,
            &statusnotifierwatcher::host_name(),
        ],
    );
    std::mem::forget(DeviceNotifications::start());
    let launcher = launcher::Launcher::new(&services.cliphist, &services.todo, &services.net);
    let osd = osd::Osd::new(app, &services, &theme);
    let region = regionselector::RegionSelector::new(
        app,
        &theme,
        &services.recording,
        &services.cursor_hider,
    );
    let wallpaper_selector =
        wallpaperselector::WallpaperSelector::new(app, &theme, &services.wallpapers);
    std::mem::forget(polkit::PolkitWindows::new(app, &theme, &services.polkit));
    std::mem::forget(keyringprompt::KeyringPromptWindows::new(
        app,
        &theme,
        &services.prompter,
    ));
    std::mem::forget(bluetoothagent::BluetoothAgentWindow::new(
        app,
        &theme,
        &services.bluez_agent,
    ));
    std::mem::forget(displayconfirm::DisplayConfirmWindows::new(app, &theme));
    conflicts::check(app, &theme);
    std::mem::forget(panels::bellflash::watch(app, &services));
    platform::network::proxy::apply_at_start();
    platform::system::autostart::start_once();
    let settings_window = settings::Settings::new(app, &theme, &services);
    let settings_page = std::env::var(core::process::OPEN_SETTINGS).ok();
    unsafe { std::env::remove_var(core::process::OPEN_SETTINGS) };
    let should_reopen_welcome = std::env::var_os(core::process::OPEN_WELCOME).is_some();
    unsafe { std::env::remove_var(core::process::OPEN_WELCOME) };
    let welcome_window = welcome::Welcome::new(app, &theme, &services);
    let welcome_actions: [(&str, fn(&Rc<welcome::Welcome>)); 3] = [
        ("open", |welcome| welcome.open()),
        ("close", |welcome| welcome.close()),
        ("toggle", |welcome| welcome.toggle()),
    ];
    for (name, act) in welcome_actions {
        let welcome = welcome_window.clone();
        ipc.add("welcome", name, move || act(&welcome));
    }

    let wants_background = !std::env::args().any(|argument| argument == "--no-background");

    let screens = Screens::new(
        screens::Shared {
            app: app.clone(),
            theme: theme.clone(),
            services: services.clone(),
            launcher: launcher.clone(),
            settings: settings_window.clone(),
            wants_background,
        },
        &config,
    );
    let fonts_watch = core::watch::config("/appearance/fonts", {
        let screens = screens.clone();
        let settings_window = settings_window.clone();
        move || {
            widgets::display::text::init(&core::config::current());
            screens.rebuild_all();
            settings_window.refont();
        }
    });
    let icon_theme_monitor = follow_icon_theme(&gtk_settings, &screens);
    let monitors = display.monitors();
    let sync: Rc<dyn Fn()> = Rc::new({
        let screens = screens.clone();
        move || screens.sync()
    });

    sync();
    register_ipc(ipc, &screens.surfaces, &services, &osd, &region);
    ipc.add("settings", "open", {
        let settings_window = settings_window.clone();
        move || settings_window.open(None)
    });
    ipc.add_with("settings", "openPage", &["page: string"], {
        let settings_window = settings_window.clone();
        move |arguments| settings_window.open(Some(&arguments[0]))
    });
    ipc.add("settings", "close", {
        let settings_window = settings_window.clone();
        move || settings_window.close()
    });
    ipc.add("settings", "toggle", {
        let settings_window = settings_window.clone();
        move || settings_window.toggle()
    });
    ipc.add("renderer", "keep", || core::config::settle_renderer(true));
    ipc.add("renderer", "revert", || {
        core::config::settle_renderer(false);
        restart_after_reply();
    });
    ipc.add("renderer", "reset", || {
        core::config::reset_renderer();
        restart_after_reply();
    });
    ipc.add("wallpaperSelector", "toggle", {
        let selector = wallpaper_selector.clone();
        move || selector.toggle()
    });
    ipc.add("wallpaperSelector", "random", {
        let selector = wallpaper_selector.clone();
        move || selector.random()
    });
    let keyboard = osk::Osk::new(app, &theme, &services.states);
    let keyboard_actions: [(&str, &str, &str, fn(&osk::Osk)); 3] = [
        (
            "toggle",
            "oskToggle",
            "Toggles on screen keyboard on press",
            |osk| osk.toggle(),
        ),
        (
            "open",
            "oskOpen",
            "Opens on screen keyboard on press",
            |osk| osk.open(),
        ),
        (
            "close",
            "oskClose",
            "Closes on screen keyboard on press",
            |osk| osk.close(),
        ),
    ];
    for (name, shortcut, description, act) in keyboard_actions {
        ipc.add("osk", name, {
            let keyboard = keyboard.clone();
            move || act(&keyboard)
        });
        actions::add(shortcut, description, {
            let keyboard = keyboard.clone();
            move || act(&keyboard)
        });
    }
    let screen_lock = lock::Lock::new(&theme, &services);
    ipc.add("lock", "activate", {
        let screen_lock = screen_lock.clone();
        move || screen_lock.lock()
    });
    ipc.add("lock", "focus", {
        let screen_lock = screen_lock.clone();
        move || screen_lock.focus()
    });
    ipc.add_with("wallpapers", "apply", &["path: string"], {
        let wallpapers = services.wallpapers.clone();
        let theme = theme.clone();
        move |arguments| {
            let is_dark_mode = theme.borrow().m3.is_dark_mode;
            wallpapers.apply(std::path::Path::new(&arguments[0]), is_dark_mode);
        }
    });
    actions::add("lock", "Locks the screen", {
        let screen_lock = screen_lock.clone();
        move || screen_lock.lock()
    });
    actions::add(
        "lockFocus",
        "Re-focuses the lock screen. This is because Hyprland after waking up for whatever reason decides to keyboard-unfocus the lock screen",
        {
            let screen_lock = screen_lock.clone();
            move || screen_lock.focus()
        },
    );
    actions::add("wallpaperSelectorToggle", "Toggle wallpaper selector", {
        let selector = wallpaper_selector.clone();
        move || selector.toggle()
    });
    actions::add(
        "wallpaperSelectorRandom",
        "Select random wallpaper in current folder",
        {
            let selector = wallpaper_selector.clone();
            move || selector.random()
        },
    );
    if let Some(audio) = services.audio.clone() {
        actions::add("micMuteToggle", "Toggles the microphone", move || {
            audio.toggle_source_mute()
        });
    }
    bind(&screens.surfaces);
    bind_states(&services.states);
    actions::add(
        "toggleLightDark",
        "Switches light and dark mode on press",
        {
            let session = services.session.clone();
            move || session.toggle_dark_mode()
        },
    );
    actions::add("osdVolumeTrigger", "Triggers volume OSD on press", {
        let osd = osd.clone();
        move || osd.trigger()
    });
    actions::add("osdVolumeHide", "Hides volume OSD on press", move || {
        osd.hide()
    });
    let region_actions: [(&str, &str, fn(&Rc<regionselector::RegionSelector>)); 4] = [
        (
            "regionScreenshot",
            "Takes a screenshot of the selected region",
            |region| region.screenshot(),
        ),
        ("regionRecord", "Records the selected region", |region| {
            region.record()
        }),
        (
            "regionRecordWithSound",
            "Records the selected region with sound",
            |region| region.record_with_sound(),
        ),
        ("recordStop", "Stops the running recording", |region| {
            region.stop_recording()
        }),
    ];
    for (name, description, act) in region_actions {
        let region = region.clone();
        actions::add(name, description, move || act(&region));
    }
    actions::add(
        "xkbLayoutNext",
        "Switches every keyboard to the next layout",
        {
            let xkb = services.xkb.clone();
            move || xkb.cycle_layout()
        },
    );
    shortcuts::Shortcuts::publish(shortcuts::APP_ID);
    let watch_monitor = {
        let weak_sync = Rc::downgrade(&sync);
        move |monitor: &gdk::Monitor| {
            let weak_sync = weak_sync.clone();
            let resync = move |_: &gdk::Monitor| {
                if let Some(sync) = weak_sync.upgrade() {
                    sync();
                }
            };
            monitor.connect_connector_notify(resync.clone());
            monitor.connect_geometry_notify(resync);
        }
    };
    for monitor in monitors.iter::<gdk::Monitor>().flatten() {
        watch_monitor(&monitor);
    }
    monitors.connect_items_changed(move |list, position, _, added_count| {
        let _keep_alive = (&theme_watch, &fonts_watch, &icon_theme_monitor);
        for index in position..position + added_count {
            if let Some(monitor) = list.item(index).and_downcast::<gdk::Monitor>() {
                watch_monitor(&monitor);
            }
        }
        sync();
    });
    services.events.start();
    screen_lock.start();
    services.background.add("trim", unload::trimmer());
    welcome::greet_if_first_run(&welcome_window);
    if should_reopen_welcome {
        welcome_window.open();
    }
    if let Some(page) = settings_page {
        settings_window.open(Some(&page));
    }
}

fn restart_after_reply() {
    glib::timeout_add_local_once(std::time::Duration::from_millis(200), || {
        core::process::restart_shell()
    });
}

fn follow_icon_theme(
    gtk_settings: &gtk4::Settings,
    screens: &Rc<Screens>,
) -> Option<gtk4::gio::FileMonitor> {
    let monitor = gtk4::gio::File::for_path(glib::user_config_dir().join("kdeglobals"))
        .monitor_file(
            gtk4::gio::FileMonitorFlags::NONE,
            gtk4::gio::Cancellable::NONE,
        )
        .ok()?;
    let gtk_settings = gtk_settings.clone();
    let screens = Rc::downgrade(screens);
    monitor.connect_changed(move |_, _, _, event| {
        if event != gtk4::gio::FileMonitorEvent::ChangesDoneHint {
            return;
        }
        let name = widgets::display::text::icon_theme();
        if gtk_settings.gtk_icon_theme_name().as_deref() == Some(name.as_str()) {
            return;
        }
        gtk_settings.set_gtk_icon_theme_name(Some(&name));
        if let Some(screens) = screens.upgrade() {
            screens.rebuild_icon_users();
        }
    });
    Some(monitor)
}

type FocusedRunner = Rc<dyn Fn(&dyn Fn(&Surfaces))>;

fn focused_runner(surfaces: &Rc<RefCell<Vec<Surfaces>>>) -> FocusedRunner {
    let surfaces = surfaces.clone();
    Rc::new(move |act: &dyn Fn(&Surfaces)| {
        let focused_connector = hypr::focused_monitor();
        let all_surfaces = surfaces.borrow();
        let target_surfaces = all_surfaces
            .iter()
            .find(|screen_surfaces| {
                Some(screen_surfaces.connector.as_str()) == focused_connector.as_deref()
            })
            .or_else(|| all_surfaces.first());
        if let Some(screen_surfaces) = target_surfaces {
            act(screen_surfaces);
        }
    })
}

fn register_ipc(
    ipc: &ipc::Ipc,
    surfaces: &Rc<RefCell<Vec<Surfaces>>>,
    services: &Rc<Services>,
    osd: &Rc<osd::Osd>,
    region: &Rc<regionselector::RegionSelector>,
) {
    let run_on_focused = focused_runner(surfaces);
    let panel_actions: [(&'static str, &'static str, fn(&Surfaces)); 25] = [
        ("sidebarRight", "toggle", |screen_surfaces| {
            screen_surfaces.sidebar.toggle()
        }),
        ("sidebarRight", "close", |screen_surfaces| {
            screen_surfaces.sidebar.close()
        }),
        ("sidebarRight", "open", |screen_surfaces| {
            screen_surfaces.sidebar.open()
        }),
        ("session", "toggle", |screen_surfaces| {
            screen_surfaces.session.toggle()
        }),
        ("session", "close", |screen_surfaces| {
            screen_surfaces.session.close()
        }),
        ("session", "open", |screen_surfaces| {
            screen_surfaces.session.open()
        }),
        ("calendar", "toggle", |screen_surfaces| {
            screen_surfaces.calendar.toggle()
        }),
        ("calendar", "close", |screen_surfaces| {
            screen_surfaces.calendar.close()
        }),
        ("calendar", "open", |screen_surfaces| {
            screen_surfaces.calendar.open()
        }),
        ("cheatsheet", "toggle", |screen_surfaces| {
            screen_surfaces.sheet.toggle()
        }),
        ("cheatsheet", "close", |screen_surfaces| {
            screen_surfaces.sheet.hide()
        }),
        ("cheatsheet", "open", |screen_surfaces| {
            screen_surfaces.sheet.open()
        }),
        ("mediaControls", "toggle", |screen_surfaces| {
            screen_surfaces.media.toggle()
        }),
        ("mediaControls", "close", |screen_surfaces| {
            screen_surfaces.media.close()
        }),
        ("mediaControls", "open", |screen_surfaces| {
            screen_surfaces.media.open()
        }),
        ("volumeMixer", "toggle", |screen_surfaces| {
            if let Some(volume_mixer) = &screen_surfaces.volume_mixer {
                volume_mixer.toggle()
            }
        }),
        ("volumeMixer", "close", |screen_surfaces| {
            if let Some(volume_mixer) = &screen_surfaces.volume_mixer {
                volume_mixer.close()
            }
        }),
        ("volumeMixer", "open", |screen_surfaces| {
            if let Some(volume_mixer) = &screen_surfaces.volume_mixer {
                volume_mixer.open()
            }
        }),
        ("search", "toggle", |screen_surfaces| {
            screen_surfaces.overview.toggle()
        }),
        ("search", "workspacesToggle", |screen_surfaces| {
            screen_surfaces.overview.toggle()
        }),
        ("search", "close", |screen_surfaces| {
            screen_surfaces.overview.hide()
        }),
        ("search", "open", |screen_surfaces| {
            screen_surfaces.overview.open()
        }),
        ("search", "clipboardToggle", |screen_surfaces| {
            screen_surfaces.overview.toggle_clipboard()
        }),
        ("search", "emojiToggle", |screen_surfaces| {
            screen_surfaces.overview.toggle_emojis()
        }),
        ("search", "workspacesClose", |screen_surfaces| {
            screen_surfaces.overview.hide()
        }),
    ];
    for (target, name, act) in panel_actions {
        let run_on_focused = run_on_focused.clone();
        ipc.add(target, name, move || run_on_focused(&act));
    }

    let bar_actions: [(&'static str, fn(&states::States)); 3] = [
        ("toggle", |states| {
            states.set_bar_open(!states.is_bar_open.get())
        }),
        ("close", |states| states.set_bar_open(false)),
        ("open", |states| states.set_bar_open(true)),
    ];
    for (name, act) in bar_actions {
        let states = services.states.clone();
        ipc.add("bar", name, move || act(&states));
    }

    let osd_actions: [(&'static str, fn(&Rc<osd::Osd>)); 3] = [
        ("trigger", |osd| osd.trigger()),
        ("hide", |osd| osd.hide()),
        ("toggle", |osd| osd.toggle()),
    ];
    for (name, act) in osd_actions {
        let osd = osd.clone();
        ipc.add("osdVolume", name, move || act(&osd));
    }

    let region_actions: [(&'static str, fn(&Rc<regionselector::RegionSelector>)); 4] = [
        ("screenshot", |region| region.screenshot()),
        ("record", |region| region.record()),
        ("recordWithSound", |region| region.record_with_sound()),
        ("stopRecording", |region| region.stop_recording()),
    ];
    for (name, act) in region_actions {
        let region = region.clone();
        ipc.add("region", name, move || act(&region));
    }

    let player_actions: [(&'static str, fn(&mpris::Mpris, &mpris::Track)); 3] = [
        ("playPause", |mpris, track| mpris.toggle_playing(track)),
        ("previous", |mpris, track| mpris.previous(track)),
        ("next", |mpris, track| mpris.skip_or_end(track)),
    ];
    ipc.add("mpris", "pauseAll", {
        let mpris = services.mpris.clone();
        move || mpris.pause_all()
    });
    for (name, act) in player_actions {
        let mpris = services.mpris.clone();
        ipc.add("mpris", name, move || {
            if let Some(track) = mpris.active_track() {
                act(&mpris, &track);
            }
        });
    }

    ipc.add("brightness", "increment", {
        let light = services.light.clone();
        move || light.raise()
    });
    ipc.add("brightness", "decrement", {
        let light = services.light.clone();
        move || light.lower()
    });
    ipc.add("theme", "toggleLightDark", {
        let session = services.session.clone();
        move || session.toggle_dark_mode()
    });
    ipc.add("audio", "cycleOutput", {
        let audio = services.audio.clone();
        move || {
            if let Some(audio) = &audio {
                audio.cycle_output();
            }
        }
    });
    ipc.add("cliphistService", "update", {
        let cliphist = services.cliphist.clone();
        move || cliphist.refresh()
    });
    deadlineipc::register(ipc, services);
    ipctargets::register(ipc, services);
}

fn bind(surfaces: &Rc<RefCell<Vec<Surfaces>>>) {
    let run_on_focused = focused_runner(surfaces);

    let shortcut_actions: [(&str, &str, Box<dyn Fn(&Surfaces)>); 21] = [
        (
            "searchToggle",
            "Toggles search on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.overview.toggle()),
        ),
        (
            "overviewWorkspacesToggle",
            "Toggles overview on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.overview.toggle()),
        ),
        (
            "overviewWorkspacesClose",
            "Closes overview on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.overview.hide()),
        ),
        (
            "overviewClipboardToggle",
            "Toggle clipboard query on overview widget",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.overview.toggle_clipboard()),
        ),
        (
            "overviewEmojiToggle",
            "Toggle emoji query on overview widget",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.overview.toggle_emojis()),
        ),
        (
            "cheatsheetToggle",
            "Toggles cheatsheet on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.sheet.toggle()),
        ),
        (
            "cheatsheetOpen",
            "Opens cheatsheet on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.sheet.open()),
        ),
        (
            "cheatsheetClose",
            "Closes cheatsheet on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.sheet.hide()),
        ),
        (
            "sidebarRightToggle",
            "Toggles right sidebar on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.sidebar.toggle()),
        ),
        (
            "sidebarRightOpen",
            "Opens right sidebar on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.sidebar.open()),
        ),
        (
            "sidebarRightClose",
            "Closes right sidebar on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.sidebar.close()),
        ),
        (
            "calendarToggle",
            "Toggles the calendar on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.calendar.toggle()),
        ),
        (
            "sessionToggle",
            "Toggle session screen on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.session.toggle()),
        ),
        (
            "sessionOpen",
            "Opens session screen on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.session.open()),
        ),
        (
            "sessionClose",
            "Closes session screen on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.session.close()),
        ),
        (
            "mediaControlsToggle",
            "Toggles media controls on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.media.toggle()),
        ),
        (
            "mediaControlsOpen",
            "Opens media controls on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.media.open()),
        ),
        (
            "mediaControlsClose",
            "Closes media controls on press",
            Box::new(|screen_surfaces: &Surfaces| screen_surfaces.media.close()),
        ),
        (
            "volumeMixerToggle",
            "Toggles the volume mixer on press",
            Box::new(|screen_surfaces: &Surfaces| {
                if let Some(volume_mixer) = &screen_surfaces.volume_mixer {
                    volume_mixer.toggle()
                }
            }),
        ),
        (
            "volumeMixerOpen",
            "Opens the volume mixer on press",
            Box::new(|screen_surfaces: &Surfaces| {
                if let Some(volume_mixer) = &screen_surfaces.volume_mixer {
                    volume_mixer.open()
                }
            }),
        ),
        (
            "volumeMixerClose",
            "Closes the volume mixer on press",
            Box::new(|screen_surfaces: &Surfaces| {
                if let Some(volume_mixer) = &screen_surfaces.volume_mixer {
                    volume_mixer.close()
                }
            }),
        ),
    ];

    for (name, description, act) in shortcut_actions {
        let run_on_focused = run_on_focused.clone();
        actions::add(name, description, move || run_on_focused(act.as_ref()));
    }
}

fn bind_states(states: &states::States) {
    let bar_shortcut_actions: [(&str, &str, fn(&states::States)); 3] = [
        ("barToggle", "Toggles bar on press", |states| {
            states.set_bar_open(!states.is_bar_open.get())
        }),
        ("barOpen", "Opens bar on press", |states| {
            states.set_bar_open(true)
        }),
        ("barClose", "Closes bar on press", |states| {
            states.set_bar_open(false)
        }),
    ];
    for (name, description, act) in bar_shortcut_actions {
        let states = states.clone();
        actions::add(name, description, move || act(&states));
    }
    actions::add(
        "workspaceNumber",
        "Hold to show workspace numbers, release to show icons",
        {
            let states = states.clone();
            move || states.set_super_down(true)
        },
    );
    actions::set_release_handler("workspaceNumber", {
        let states = states.clone();
        move || states.set_super_down(false)
    });
}

fn connector(monitor: &gdk::Monitor) -> String {
    monitor.connector().map(Into::into).unwrap_or_default()
}
