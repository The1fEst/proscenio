#[cfg(feature = "compat")]
mod compat;
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
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

use crate::core::actions;
use crate::core::config::Config;
use crate::core::paths;
use crate::core::scope::Scope;
use crate::panels::overview::{self, launcher};
use crate::panels::{
    bar, calendar, conflicts, lock, mediacontrols, osd, osk, polkit, regionselector, renderercheck,
    settings, sidebar, wallpaperselector, welcome,
};
use crate::platform::{hypr, ipc, locale, shortcuts};
use crate::screens::{Screens, Surfaces};
use crate::services::{Services, mpris, recording, states};
use crate::theming::{colors, switchwall};
use crate::ui::theme::{self, Theme};
use crate::ui::{anim, reserve, unload, widgets};

const APP_ID: &str = "dev.fEst.Proscenio";
const NAMESPACE: &str = "proscenio:bar";
const VERTICAL_NAMESPACE: &str = "proscenio:verticalBar";
const RESERVE_NAMESPACE: &str = "proscenio:barReserve";

fn main() -> glib::ExitCode {
    unload::single_arena();
    let arguments: Vec<String> = std::env::args().collect();
    let command = arguments.get(1).map(String::as_str);
    if command == Some("ipc") {
        return ipc::client(APP_ID, &arguments[2..]);
    }
    if command == Some(locale::COMMAND) {
        return locale::run(&arguments[2..]);
    }
    #[cfg(feature = "compat")]
    compat::prepare();
    paths::prepare();
    match command {
        Some("colors") => return colors::run(&arguments[2..]),
        Some("switchwall") => return switchwall::run(&arguments[2..]),
        Some("record") => return recording::run(&arguments[2..]),
        Some(renderercheck::COMMAND) => return renderercheck::run(APP_ID),
        _ => {}
    }
    let chose_renderer = crate::core::config::choose_renderer();
    let app = gtk4::Application::builder().application_id(APP_ID).build();
    let ipc = Rc::new(ipc::Ipc::default());
    let built = Cell::new(false);
    app.connect_activate(move |app| {
        if built.replace(true) {
            return;
        }
        if let Some(connection) = app.dbus_connection() {
            ipc.export(&connection);
        }
        if chose_renderer {
            renderercheck::start_if_on_trial();
        }
        build(app, &ipc);
        if chose_renderer {
            unsafe { std::env::remove_var("GSK_RENDERER") };
        }
    });
    app.run_with_args(&arguments[..1])
}

fn build(app: &gtk4::Application, ipc: &Rc<ipc::Ipc>) {
    let config = Rc::new(Config::load());
    widgets::text::init(&config);
    let theme = Rc::new(RefCell::new(Theme::load(&config)));

    let provider = gtk4::CssProvider::new();
    provider.load_from_string(&theme.borrow().css());
    let display = gdk::Display::default().expect("no Wayland display");
    let settings = gtk4::Settings::for_display(&display);
    settings.set_gtk_icon_theme_name(Some(&widgets::text::icon_theme()));
    gtk4::style_context_add_provider_for_display(
        &display,
        &provider,
        gtk4::STYLE_PROVIDER_PRIORITY_USER + 1,
    );
    let watch = theme::watch(&theme, &provider, app);

    let services = Rc::new(Services::new(&config));
    let launcher = launcher::Launcher::new(&services.cliphist, &services.todo, &services.net);
    let osd = osd::Osd::new(app, &services, &theme);
    let region = regionselector::RegionSelector::new(app, &theme, &services.recording);
    let wallpaper_selector =
        wallpaperselector::WallpaperSelector::new(app, &theme, &services.wallpapers);
    std::mem::forget(polkit::PolkitWindows::new(app, &theme, &services.polkit));
    conflicts::check(app, &theme);
    platform::proxy::apply_at_start();
    let settings_window = settings::Settings::new(app, &theme, &services);
    let settings_page = std::env::var(core::process::OPEN_SETTINGS).ok();
    unsafe { std::env::remove_var(core::process::OPEN_SETTINGS) };
    let reopen_welcome = std::env::var_os(core::process::OPEN_WELCOME).is_some();
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
    let fonts = core::watch::config("/appearance/fonts", {
        let screens = screens.clone();
        let settings = settings_window.clone();
        move || {
            widgets::text::init(&core::config::current());
            screens.rebuild_all();
            settings.refont();
        }
    });
    let icons = follow_icon_theme(&settings, &screens);
    let monitors = display.monitors();
    let sync: Rc<dyn Fn()> = Rc::new({
        let screens = screens.clone();
        move || screens.sync()
    });

    sync();
    register_ipc(ipc, &screens.surfaces, &services, &osd, &region);
    ipc.add("settings", "open", {
        let settings = settings_window.clone();
        move || settings.open(None)
    });
    ipc.add_with("settings", "openPage", &["page: string"], {
        let settings = settings_window.clone();
        move |arguments| settings.open(Some(&arguments[0]))
    });
    ipc.add("settings", "close", {
        let settings = settings_window.clone();
        move || settings.close()
    });
    ipc.add("settings", "toggle", {
        let settings = settings_window.clone();
        move || settings.toggle()
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
            let dark = theme.borrow().m3.darkmode;
            wallpapers.apply(std::path::Path::new(&arguments[0]), dark);
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
    if let Some(shortcuts) = shortcuts::Shortcuts::publish(shortcuts::APP_ID) {
        std::mem::forget(shortcuts);
    }
    #[cfg(feature = "compat")]
    compat::install(ipc, &settings_window);
    let follow = {
        let later = Rc::downgrade(&sync);
        move |monitor: &gdk::Monitor| {
            let later = later.clone();
            let again = move |_: &gdk::Monitor| {
                if let Some(sync) = later.upgrade() {
                    sync();
                }
            };
            monitor.connect_connector_notify(again.clone());
            monitor.connect_geometry_notify(again);
        }
    };
    for monitor in monitors.iter::<gdk::Monitor>().flatten() {
        follow(&monitor);
    }
    monitors.connect_items_changed(move |list, position, _, added| {
        let _keep_alive = (&watch, &fonts, &icons);
        for index in position..position + added {
            if let Some(monitor) = list.item(index).and_downcast::<gdk::Monitor>() {
                follow(&monitor);
            }
        }
        sync();
    });
    services.events.start();
    screen_lock.start();
    services.background.add("trim", unload::trimming());
    welcome::greet_if_first_run(&welcome_window);
    if reopen_welcome {
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
    settings: &gtk4::Settings,
    screens: &Rc<Screens>,
) -> Option<gtk4::gio::FileMonitor> {
    let monitor = gtk4::gio::File::for_path(glib::user_config_dir().join("kdeglobals"))
        .monitor_file(
            gtk4::gio::FileMonitorFlags::NONE,
            gtk4::gio::Cancellable::NONE,
        )
        .ok()?;
    let settings = settings.clone();
    let screens = Rc::downgrade(screens);
    monitor.connect_changed(move |_, _, _, event| {
        if event != gtk4::gio::FileMonitorEvent::ChangesDoneHint {
            return;
        }
        let name = widgets::text::icon_theme();
        if settings.gtk_icon_theme_name().as_deref() == Some(name.as_str()) {
            return;
        }
        settings.set_gtk_icon_theme_name(Some(&name));
        if let Some(screens) = screens.upgrade() {
            screens.rebuild_icon_users();
        }
    });
    Some(monitor)
}

type Focused = Rc<dyn Fn(&dyn Fn(&Surfaces))>;

fn focused(surfaces: &Rc<RefCell<Vec<Surfaces>>>) -> Focused {
    let surfaces = surfaces.clone();
    Rc::new(move |act: &dyn Fn(&Surfaces)| {
        let wanted = hypr::focused_monitor();
        let held = surfaces.borrow();
        let found = held
            .iter()
            .find(|entry| Some(entry.connector.as_str()) == wanted.as_deref())
            .or_else(|| held.first());
        if let Some(entry) = found {
            act(entry);
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
    let focused = focused(surfaces);
    let panels: [(&'static str, &'static str, fn(&Surfaces)); 20] = [
        ("sidebarRight", "toggle", |held| held.sidebar.toggle()),
        ("sidebarRight", "close", |held| held.sidebar.close()),
        ("sidebarRight", "open", |held| held.sidebar.open()),
        ("session", "toggle", |held| held.session.toggle()),
        ("session", "close", |held| held.session.close()),
        ("session", "open", |held| held.session.open()),
        ("calendar", "toggle", |held| held.calendar.toggle_centred()),
        ("calendar", "close", |held| held.calendar.close()),
        ("calendar", "open", |held| held.calendar.open()),
        ("cheatsheet", "toggle", |held| held.sheet.toggle()),
        ("cheatsheet", "close", |held| held.sheet.hide()),
        ("cheatsheet", "open", |held| held.sheet.open()),
        ("mediaControls", "close", |held| held.media.close()),
        ("search", "toggle", |held| held.overview.toggle()),
        ("search", "workspacesToggle", |held| held.overview.toggle()),
        ("search", "close", |held| held.overview.hide()),
        ("search", "open", |held| held.overview.open()),
        ("search", "clipboardToggle", |held| {
            held.overview.toggle_clipboard()
        }),
        ("search", "emojiToggle", |held| {
            held.overview.toggle_emojis()
        }),
        ("search", "workspacesClose", |held| held.overview.hide()),
    ];
    for (target, name, act) in panels {
        let focused = focused.clone();
        ipc.add(target, name, move || focused(&act));
    }
    for (name, toggle) in [("toggle", true), ("open", false)] {
        let focused = focused.clone();
        let notifications = services.notifications.clone();
        ipc.add("mediaControls", name, move || {
            focused(&|held: &Surfaces| {
                if toggle {
                    held.media.toggle();
                } else {
                    held.media.open();
                }
                if held.media.is_open() {
                    notifications.timeout_all();
                }
            })
        });
    }

    let states: [(&'static str, fn(&states::States)); 3] = [
        ("toggle", |states| {
            states.set_bar_open(!states.bar_open.get())
        }),
        ("close", |states| states.set_bar_open(false)),
        ("open", |states| states.set_bar_open(true)),
    ];
    for (name, act) in states {
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

    let region_actions: [(&'static str, fn(&Rc<regionselector::RegionSelector>)); 3] = [
        ("screenshot", |region| region.screenshot()),
        ("record", |region| region.record()),
        ("recordWithSound", |region| region.record_with_sound()),
    ];
    for (name, act) in region_actions {
        let region = region.clone();
        ipc.add("region", name, move || act(&region));
    }

    let player: [(&'static str, fn(&mpris::Mpris, &mpris::Track)); 3] = [
        ("playPause", |mpris, track| mpris.toggle_playing(track)),
        ("previous", |mpris, track| mpris.previous(track)),
        ("next", |mpris, track| mpris.skip_or_end(track)),
    ];
    ipc.add("mpris", "pauseAll", {
        let mpris = services.mpris.clone();
        move || mpris.pause_all()
    });
    for (name, act) in player {
        let mpris = services.mpris.clone();
        ipc.add("mpris", name, move || {
            if let Some(track) = mpris.active() {
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
        move || session.toggle_dark()
    });
    ipc.add("cliphistService", "update", {
        let cliphist = services.cliphist.clone();
        move || cliphist.refresh()
    });
}

fn bind(surfaces: &Rc<RefCell<Vec<Surfaces>>>) {
    let focused = focused(surfaces);

    let acts: [(&str, &str, Box<dyn Fn(&Surfaces)>); 18] = [
        (
            "searchToggle",
            "Toggles search on press",
            Box::new(|held: &Surfaces| held.overview.toggle()),
        ),
        (
            "overviewWorkspacesToggle",
            "Toggles overview on press",
            Box::new(|held: &Surfaces| held.overview.toggle()),
        ),
        (
            "overviewWorkspacesClose",
            "Closes overview on press",
            Box::new(|held: &Surfaces| held.overview.hide()),
        ),
        (
            "overviewClipboardToggle",
            "Toggle clipboard query on overview widget",
            Box::new(|held: &Surfaces| held.overview.toggle_clipboard()),
        ),
        (
            "overviewEmojiToggle",
            "Toggle emoji query on overview widget",
            Box::new(|held: &Surfaces| held.overview.toggle_emojis()),
        ),
        (
            "cheatsheetToggle",
            "Toggles cheatsheet on press",
            Box::new(|held: &Surfaces| held.sheet.toggle()),
        ),
        (
            "cheatsheetOpen",
            "Opens cheatsheet on press",
            Box::new(|held: &Surfaces| held.sheet.open()),
        ),
        (
            "cheatsheetClose",
            "Closes cheatsheet on press",
            Box::new(|held: &Surfaces| held.sheet.hide()),
        ),
        (
            "sidebarRightToggle",
            "Toggles right sidebar on press",
            Box::new(|held: &Surfaces| held.sidebar.toggle()),
        ),
        (
            "sidebarRightOpen",
            "Opens right sidebar on press",
            Box::new(|held: &Surfaces| held.sidebar.open()),
        ),
        (
            "sidebarRightClose",
            "Closes right sidebar on press",
            Box::new(|held: &Surfaces| held.sidebar.close()),
        ),
        (
            "calendarToggle",
            "Toggles the calendar on press",
            Box::new(|held: &Surfaces| held.calendar.toggle_centred()),
        ),
        (
            "sessionToggle",
            "Toggle session screen on press",
            Box::new(|held: &Surfaces| held.session.toggle()),
        ),
        (
            "sessionOpen",
            "Opens session screen on press",
            Box::new(|held: &Surfaces| held.session.open()),
        ),
        (
            "sessionClose",
            "Closes session screen on press",
            Box::new(|held: &Surfaces| held.session.close()),
        ),
        (
            "mediaControlsToggle",
            "Toggles media controls on press",
            Box::new(|held: &Surfaces| held.media.toggle()),
        ),
        (
            "mediaControlsOpen",
            "Opens media controls on press",
            Box::new(|held: &Surfaces| held.media.open()),
        ),
        (
            "mediaControlsClose",
            "Closes media controls on press",
            Box::new(|held: &Surfaces| held.media.close()),
        ),
    ];

    for (name, description, act) in acts {
        let focused = focused.clone();
        actions::add(name, description, move || focused(act.as_ref()));
    }
}

fn bind_states(states: &states::States) {
    let toggles: [(&str, &str, fn(&states::States)); 3] = [
        ("barToggle", "Toggles bar on press", |states| {
            states.set_bar_open(!states.bar_open.get())
        }),
        ("barOpen", "Opens bar on press", |states| {
            states.set_bar_open(true)
        }),
        ("barClose", "Closes bar on press", |states| {
            states.set_bar_open(false)
        }),
    ];
    for (name, description, act) in toggles {
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
    actions::on_release("workspaceNumber", {
        let states = states.clone();
        move || states.set_super_down(false)
    });
}

fn open_bar(
    app: &gtk4::Application,
    config: &Rc<Config>,
    theme: &theme::SharedTheme,
    services: &Services,
    sidebar: &Rc<sidebar::Sidebar>,
    calendar: &Rc<calendar::Calendar>,
    media: &Rc<mediacontrols::MediaControls>,
    overview: &Rc<overview::Overview>,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> gtk4::ApplicationWindow {
    let screen = bar::Screen {
        connector: &connector(monitor),
        width: monitor.geometry().width(),
    };
    let body = if config.vertical {
        bar::vertical::build(
            config, theme, services, sidebar, calendar, media, overview, screen, scope,
        )
    } else {
        bar::build(
            config, theme, services, sidebar, calendar, media, overview, screen, scope,
        )
    };
    let content = widgets::text::Shift::new(&body);
    let far = if config.bottom {
        gtk4::Align::End
    } else {
        gtk4::Align::Start
    };
    let builder = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&content);
    let window = if config.vertical {
        content.set_sideways(true);
        content.set_halign(far);
        builder.default_width(config.surface_thickness()).build()
    } else {
        content.set_valign(far);
        builder.default_height(config.surface_thickness()).build()
    };

    let edge = match (config.vertical, config.bottom) {
        (false, false) => Edge::Top,
        (false, true) => Edge::Bottom,
        (true, false) => Edge::Left,
        (true, true) => Edge::Right,
    };
    window.init_layer_shell();
    window.set_namespace(Some(if config.vertical {
        VERTICAL_NAMESPACE
    } else {
        NAMESPACE
    }));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Top);
    if config.vertical {
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Bottom, true);
    } else {
        window.set_anchor(Edge::Left, true);
        window.set_anchor(Edge::Right, true);
    }
    window.set_anchor(edge, true);
    if config.dead_pixel && !config.vertical {
        window.set_margin(Edge::Right, -1);
        if config.bottom {
            window.set_margin(Edge::Bottom, -1);
        }
    }
    window.set_exclusive_zone(config.exclusive_zone());
    focus_on_demand_once_shown(&window);

    let offset = anim::Motion::new(&content, 0.0, 200.0, anim::EXPRESSIVE_EFFECTS);
    let hovered = Rc::new(Cell::new(false));
    let super_show = Rc::new(Cell::new(false));
    let place = {
        let config = config.clone();
        let window = window.downgrade();
        let content = content.downgrade();
        let offset = offset.clone();
        move || {
            let (Some(content), Some(window)) = (content.upgrade(), window.upgrade()) else {
                return;
            };
            let shift = offset.get() as i32;
            content.set_offset(shift as f32);
            let reach = config.hover_region_width;
            let near = if config.bottom {
                config.hug_rounding() + shift - reach
            } else {
                shift - reach
            };
            let thickness = config.bar_thickness() + reach * 2;
            let strip = if config.vertical {
                gtk4::cairo::RectangleInt::new(near, 0, thickness, i32::MAX / 2)
            } else {
                gtk4::cairo::RectangleInt::new(0, near, i32::MAX / 2, thickness)
            };
            if let Some(surface) = window.surface() {
                surface.set_input_region(Some(&gtk4::cairo::Region::create_rectangle(&strip)));
            }
        }
    };
    let place = Rc::new(place);
    window.connect_realize({
        let place = place.clone();
        move |_| place()
    });

    let update = {
        let config = config.clone();
        let window = window.downgrade();
        let content = content.downgrade();
        let offset = offset.clone();
        let hovered = hovered.clone();
        let super_show = super_show.clone();
        let place = place.clone();
        move || {
            if !config.auto_hide {
                return;
            }
            let (Some(content), Some(window)) = (content.upgrade(), window.upgrade()) else {
                return;
            };
            let must_show = hovered.get() || super_show.get();
            window.set_exclusive_zone(if !must_show || !config.auto_hide_push_windows {
                0
            } else {
                config.exclusive_zone()
            });
            let hidden = if config.bottom {
                config.bar_thickness()
            } else {
                -config.bar_thickness()
            };
            offset.to(if must_show { 0.0 } else { hidden as f64 });
            let place = place.clone();
            let offset = offset.clone();
            content.add_tick_callback(move |_, _| {
                place();
                if offset.running() {
                    gtk4::glib::ControlFlow::Continue
                } else {
                    gtk4::glib::ControlFlow::Break
                }
            });
        }
    };
    let update = Rc::new(update);
    if config.auto_hide {
        let hidden = if config.bottom {
            config.bar_thickness()
        } else {
            -config.bar_thickness()
        };
        offset.jump(hidden as f64);
        window.set_exclusive_zone(0);
    }

    let hover = gtk4::EventControllerMotion::new();
    hover.connect_enter({
        let hovered = hovered.clone();
        let update = update.clone();
        move |_, _, _| {
            hovered.set(true);
            update();
        }
    });
    hover.connect_leave({
        let hovered = hovered.clone();
        let update = update.clone();
        move |_| {
            hovered.set(false);
            update();
        }
    });
    window.add_controller(hover);

    let reserve = reserve::Reserve::new(app, monitor, edge, RESERVE_NAMESPACE);
    let show: Rc<dyn Fn()> = Rc::new({
        let states = services.states.clone();
        let fullscreen = services.fullscreen.clone();
        let connector = connector(monitor);
        let window = window.clone();
        move || {
            let wanted = states.bar_open.get() && !states.screen_locked.get();
            if wanted && fullscreen.covers(&connector) {
                reserve.hold(window.exclusive_zone());
                window.set_visible(false);
                return;
            }
            window.set_visible(wanted);
            reserve.release();
        }
    });
    scope.keep(services.fullscreen.subscribe({
        let show = show.clone();
        move || show()
    }));

    let pending: Rc<RefCell<Option<gtk4::glib::SourceId>>> = Rc::new(RefCell::new(None));
    scope.keep(services.states.subscribe({
        let states = services.states.clone();
        let config = config.clone();
        let show = show.clone();
        move || {
            show();
            if !config.super_show {
                return;
            }
            if let Some(timer) = pending.borrow_mut().take() {
                timer.remove();
            }
            if states.super_down.get() {
                let super_show = super_show.clone();
                let update = update.clone();
                let slot = pending.clone();
                let delay = std::time::Duration::from_millis(config.super_show_delay as u64);
                pending.replace(Some(gtk4::glib::timeout_add_local_once(delay, move || {
                    slot.replace(None);
                    super_show.set(true);
                    update();
                })));
            } else if super_show.replace(false) {
                update();
            }
        }
    }));

    if services.fullscreen.covers(&connector(monitor)) {
        show();
    } else {
        window.present();
    }
    window
}

fn focus_on_demand_once_shown(window: &gtk4::ApplicationWindow) {
    window.set_keyboard_mode(KeyboardMode::None);
    window.connect_realize(|window| {
        let Some(surface) = window.surface() else {
            return;
        };
        let window = window.downgrade();
        surface.connect_enter_monitor(move |_, _| {
            if let Some(window) = window.upgrade() {
                window.set_keyboard_mode(KeyboardMode::OnDemand);
            }
        });
    });
    window.connect_unmap(|window| window.set_keyboard_mode(KeyboardMode::None));
}

fn connector(monitor: &gdk::Monitor) -> String {
    monitor.connector().map(Into::into).unwrap_or_default()
}
