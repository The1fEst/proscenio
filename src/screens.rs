use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;

use crate::core::config::Config;
use crate::core::scope::Scope;
use crate::core::watch;
use crate::panels::notifications::popup;
use crate::panels::overview::{self, launcher};
use crate::panels::{
    background, calendar, cheatsheet, corners, dock, mediacontrols, mixer, sessionscreen, settings,
    sidebar,
};
use crate::platform::hyprland::hypr;
use crate::services::Services;
use crate::ui::theme::SharedTheme;
use crate::ui::unload;

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Panels,
    Overview,
    Sheet,
    Popups,
    Background,
    Dock,
    Bar,
}

const PARTS: [Part; 7] = [
    Part::Panels,
    Part::Overview,
    Part::Sheet,
    Part::Popups,
    Part::Background,
    Part::Dock,
    Part::Bar,
];

impl Part {
    fn pointers(self) -> &'static [&'static str] {
        match self {
            Part::Panels => &[
                "/sidebar/quickToggles/style",
                "/sidebar/quickToggles/android/columns",
                "/sidebar/quickSliders",
                "/bar/bottom",
                "/bar/vertical",
                "/bar/cornerStyle",
            ],
            Part::Overview => &["/overview"],
            Part::Sheet => &["/cheatsheet"],
            Part::Popups => &[],
            Part::Background => &[
                "/background/hideWhenFullscreen",
                "/background/parallax",
                "/bar/workspaces/shown",
            ],
            Part::Dock => &[
                "/dock/enable",
                "/dock/iconSize",
                "/dock/hoverRegionHeight",
                "/dock/hoverToReveal",
                "/dock/monochromeIcons",
                "/dock/ignoredAppRegexes",
            ],
            Part::Bar => &[
                "/bar",
                "/tray/filterPassive",
                "/tray/invertPinnedItems",
                "/tray/monochromeIcons",
                "/tray/showItemId",
                "/battery/low",
                "/interactions/deadPixelWorkaround",
                "/appearance/fakeScreenRounding",
                "/sidebar/cornerOpen",
            ],
        }
    }

    fn ignored(self) -> &'static [&'static str] {
        match self {
            Part::Bar => &[
                "/bar/screenList",
                "/bar/weather/enableGPS",
                "/bar/weather/city",
                "/bar/weather/useUSCS",
                "/bar/weather/fetchInterval",
            ],
            _ => &[],
        }
    }
}

struct Panels {
    sidebar: Rc<sidebar::Sidebar>,
    calendar: Rc<calendar::Calendar>,
    session: Rc<sessionscreen::SessionScreen>,
    media: Rc<mediacontrols::MediaControls>,
    volume_mixer: Option<Rc<mixer::VolumeMixer>>,
}

impl Panels {
    fn windows(&self) -> Vec<gtk4::ApplicationWindow> {
        let mut windows = vec![
            self.sidebar.window.clone(),
            self.calendar.window.clone(),
            self.session.window.clone(),
            self.media.window.clone(),
        ];
        windows.extend(
            self.volume_mixer
                .iter()
                .map(|volume_mixer| volume_mixer.window.clone()),
        );
        windows
    }
}

struct PartInstance {
    part: Part,
    windows: Vec<gtk4::ApplicationWindow>,
    _scope: Scope,
}

struct Screen {
    monitor: gdk::Monitor,
    geometry: gdk::Rectangle,
    part_instances: Vec<PartInstance>,
}

pub struct Surfaces {
    pub connector: String,
    pub sidebar: Rc<sidebar::Sidebar>,
    pub calendar: Rc<calendar::Calendar>,
    pub session: Rc<sessionscreen::SessionScreen>,
    pub media: Rc<mediacontrols::MediaControls>,
    pub volume_mixer: Option<Rc<mixer::VolumeMixer>>,
    pub sheet: Rc<cheatsheet::Cheatsheet>,
    pub overview: Rc<overview::Overview>,
}

pub struct Shared {
    pub app: gtk4::Application,
    pub theme: SharedTheme,
    pub services: Rc<Services>,
    pub launcher: launcher::Launcher,
    pub settings: Rc<settings::Settings>,
    pub wants_background: bool,
}

/// The per-monitor surfaces, rebuilt a part at a time when the config fields that part was
/// built from change.
pub struct Screens {
    shared: Shared,
    config: RefCell<Rc<Config>>,
    screens: RefCell<Vec<Screen>>,
    pub surfaces: Rc<RefCell<Vec<Surfaces>>>,
    dirty_parts: RefCell<Vec<Part>>,
    rounding: Cell<i32>,
    _watches: RefCell<Vec<watch::Watch>>,
}

impl Screens {
    pub fn new(shared: Shared, config: &Rc<Config>) -> Rc<Self> {
        let screens = Rc::new(Screens {
            shared,
            config: RefCell::new(config.clone()),
            screens: RefCell::new(Vec::new()),
            surfaces: Rc::default(),
            dirty_parts: RefCell::new(Vec::new()),
            rounding: Cell::new(config.rounding),
            _watches: RefCell::new(Vec::new()),
        });
        let mut watches = Vec::new();
        for part in PARTS {
            for pointer in part.pointers() {
                let screens = Rc::downgrade(&screens);
                watches.push(watch::config_except(pointer, part.ignored(), move || {
                    mark_dirty(&screens, &[part])
                }));
            }
        }
        watches.push(watch::config("/bar/screenList", {
            let screens = Rc::downgrade(&screens);
            move || {
                if let Some(screens) = screens.upgrade() {
                    screens.refresh_config();
                    screens.sync();
                }
            }
        }));
        screens._watches.replace(watches);
        screens
            .shared
            .services
            .events
            .subscribe({
                let screens = Rc::downgrade(&screens);
                move |event, _| {
                    if event != "configreloaded" {
                        return;
                    }
                    let Some(strong_screens) = screens.upgrade() else {
                        return;
                    };
                    let rounding = hypr::option_int("decoration:rounding").unwrap_or(23);
                    if strong_screens.rounding.replace(rounding) != rounding {
                        mark_dirty(&screens, &[Part::Bar]);
                    }
                }
            })
            .detach();
        screens
    }

    /// Rebuilds every part, for changes that reach all of them, such as the fonts.
    pub fn rebuild_all(self: &Rc<Self>) {
        mark_dirty(&Rc::downgrade(self), &PARTS);
    }

    pub fn rebuild_icon_users(self: &Rc<Self>) {
        let parts: Vec<Part> = PARTS
            .into_iter()
            .filter(|part| *part != Part::Background)
            .collect();
        mark_dirty(&Rc::downgrade(self), &parts);
    }

    fn refresh_config(&self) {
        let new_config = Rc::new(Config::load());
        self.rounding.set(new_config.rounding);
        self.config.replace(new_config);
    }

    pub fn sync(&self) {
        let config = self.config.borrow().clone();
        let services = &self.shared.services;
        let present_monitors: Vec<gdk::Monitor> = gdk::Display::default()
            .map(|display| display.monitors())
            .into_iter()
            .flat_map(|monitors| {
                monitors
                    .iter::<gdk::Monitor>()
                    .flatten()
                    .collect::<Vec<_>>()
            })
            .collect();
        let connectors: Vec<String> = present_monitors
            .iter()
            .map(crate::connector)
            .filter(|connector| !connector.is_empty())
            .collect();
        services.light.set_screens(connectors.clone());
        let has_listed_screen_connected = config
            .screens
            .iter()
            .any(|listed_screen| connectors.contains(listed_screen));
        let is_wanted =
            |connector: &str| !has_listed_screen_connected || config.wants_screen(connector);
        let removed_screens: Vec<Screen> = {
            let mut screens = self.screens.borrow_mut();
            let (remaining_screens, removed_screens) = std::mem::take(&mut *screens)
                .into_iter()
                .partition(|screen: &Screen| {
                    present_monitors.contains(&screen.monitor)
                        && screen.monitor.geometry() == screen.geometry
                        && is_wanted(&crate::connector(&screen.monitor))
                });
            *screens = remaining_screens;
            removed_screens
        };
        let live_connectors: Vec<String> = self
            .screens
            .borrow()
            .iter()
            .map(|screen| crate::connector(&screen.monitor))
            .collect();
        self.surfaces
            .borrow_mut()
            .retain(|screen_surfaces| live_connectors.contains(&screen_surfaces.connector));
        for screen in removed_screens {
            for part_instance in screen.part_instances {
                discard(part_instance);
            }
        }

        for monitor in present_monitors {
            let is_ready = !crate::connector(&monitor).is_empty() && monitor.geometry().width() > 0;
            if !is_ready || !is_wanted(&crate::connector(&monitor)) {
                continue;
            }
            if self
                .screens
                .borrow()
                .iter()
                .any(|screen| screen.monitor == monitor)
            {
                continue;
            }
            let mut part_instances = self.build_surface_parts(&monitor, &config);
            for part in PARTS {
                if matches!(part, Part::Panels | Part::Overview | Part::Sheet) {
                    continue;
                }
                if let Some(part_instance) = self.build(part, &monitor, &config) {
                    part_instances.push(part_instance);
                }
            }
            self.screens.borrow_mut().push(Screen {
                geometry: monitor.geometry(),
                monitor,
                part_instances,
            });
        }
    }

    fn rebuild(&self, parts: &[Part]) {
        self.refresh_config();
        let config = self.config.borrow().clone();
        let monitors: Vec<gdk::Monitor> = self
            .screens
            .borrow()
            .iter()
            .map(|screen| screen.monitor.clone())
            .collect();
        for monitor in monitors {
            for part in PARTS.into_iter().filter(|part| parts.contains(part)) {
                let old_instance = {
                    let mut screens = self.screens.borrow_mut();
                    let Some(screen) = screens.iter_mut().find(|screen| screen.monitor == monitor)
                    else {
                        continue;
                    };
                    screen
                        .part_instances
                        .iter()
                        .position(|part_instance| part_instance.part == part)
                        .map(|index| screen.part_instances.remove(index))
                };
                if let Some(old_instance) = old_instance {
                    discard(old_instance);
                }
                let Some(new_instance) = self.build(part, &monitor, &config) else {
                    continue;
                };
                if let Some(screen) = self
                    .screens
                    .borrow_mut()
                    .iter_mut()
                    .find(|screen| screen.monitor == monitor)
                {
                    screen.part_instances.push(new_instance);
                }
            }
        }
    }

    fn build(
        &self,
        part: Part,
        monitor: &gdk::Monitor,
        config: &Rc<Config>,
    ) -> Option<PartInstance> {
        let shared = &self.shared;
        let (app, theme, services) = (&shared.app, &shared.theme, &shared.services);
        let connector = crate::connector(monitor);
        let scope = Scope::default();
        let windows = match part {
            Part::Panels => {
                let panels = self.panels(monitor, config, &scope);
                let windows = panels.windows();
                self.update_surfaces(&connector, |screen_surfaces| {
                    screen_surfaces.sidebar = panels.sidebar;
                    screen_surfaces.calendar = panels.calendar;
                    screen_surfaces.session = panels.session;
                    screen_surfaces.media = panels.media;
                    screen_surfaces.volume_mixer = panels.volume_mixer;
                });
                windows
            }
            Part::Overview => {
                let overview = self.overview(monitor, config, &scope);
                let window = overview.window.clone();
                self.update_surfaces(&connector, |screen_surfaces| {
                    screen_surfaces.overview = overview
                });
                vec![window]
            }
            Part::Sheet => {
                let sheet = cheatsheet::build(app, config, monitor);
                let window = sheet.window.clone();
                self.update_surfaces(&connector, |screen_surfaces| screen_surfaces.sheet = sheet);
                vec![window]
            }
            Part::Popups => vec![popup::open(
                app,
                &services.notifications,
                &services.events,
                &services.states,
                theme,
                monitor,
                &scope,
            )],
            Part::Background if shared.wants_background => {
                vec![background::open(
                    app, config, theme, services, monitor, &scope,
                )]
            }
            Part::Background => return None,
            Part::Dock => {
                vec![dock::open(app, config, services, theme, monitor, &scope)?]
            }
            Part::Bar => {
                let surfaces = self.surfaces.borrow();
                let screen_surfaces = surfaces
                    .iter()
                    .find(|screen_surfaces| screen_surfaces.connector == connector)?;
                let mut windows = vec![crate::open_bar(
                    app,
                    config,
                    theme,
                    services,
                    &screen_surfaces.sidebar,
                    &screen_surfaces.calendar,
                    &screen_surfaces.media,
                    &screen_surfaces.volume_mixer,
                    &screen_surfaces.overview,
                    monitor,
                    &scope,
                )];
                windows.extend(corners::open(
                    app,
                    config,
                    services,
                    &screen_surfaces.sidebar,
                    monitor,
                    &scope,
                ));
                windows
            }
        };
        Some(self.finish(part, windows, scope))
    }

    /// Builds the parts other parts and the IPC hold on to, and records them for the monitor.
    fn build_surface_parts(
        &self,
        monitor: &gdk::Monitor,
        config: &Rc<Config>,
    ) -> Vec<PartInstance> {
        let (panels_scope, overview_scope) = (Scope::default(), Scope::default());
        let panels = self.panels(monitor, config, &panels_scope);
        let overview = self.overview(monitor, config, &overview_scope);
        let sheet = cheatsheet::build(&self.shared.app, config, monitor);
        let part_instances = vec![
            self.finish(Part::Panels, panels.windows(), panels_scope),
            self.finish(
                Part::Overview,
                vec![overview.window.clone()],
                overview_scope,
            ),
            self.finish(Part::Sheet, vec![sheet.window.clone()], Scope::default()),
        ];
        self.surfaces.borrow_mut().push(Surfaces {
            connector: crate::connector(monitor),
            sidebar: panels.sidebar,
            calendar: panels.calendar,
            session: panels.session,
            media: panels.media,
            volume_mixer: panels.volume_mixer,
            sheet,
            overview,
        });
        part_instances
    }

    fn panels(&self, monitor: &gdk::Monitor, config: &Rc<Config>, scope: &Scope) -> Panels {
        let shared = &self.shared;
        let (app, theme, services) = (&shared.app, &shared.theme, &shared.services);
        let session = sessionscreen::build(app, theme, services, monitor, scope);
        let sidebar = sidebar::build(
            app,
            config,
            theme,
            services,
            &session,
            &shared.settings,
            monitor,
            scope,
        );
        let set_sidebar_open: Rc<dyn Fn(bool)> = {
            let notifications = services.notifications.clone();
            let states = services.states.clone();
            Rc::new(move |is_open| {
                if is_open {
                    states.add_open_sidebar();
                } else {
                    states.remove_open_sidebar();
                }
                notifications.set_inhibited(states.is_sidebar_open.get());
            })
        };
        let is_sidebar_open = Rc::new(Cell::new(false));
        scope.keep(sidebar.watch({
            let set_sidebar_open = set_sidebar_open.clone();
            let is_sidebar_open = is_sidebar_open.clone();
            move |is_open| {
                if is_sidebar_open.replace(is_open) != is_open {
                    set_sidebar_open(is_open);
                }
            }
        }));
        scope.defer(move || {
            if is_sidebar_open.replace(false) {
                set_sidebar_open(false);
            }
        });
        Panels {
            calendar: calendar::build(app, config, theme, services, monitor, scope),
            media: mediacontrols::build(app, config, services, theme, monitor, scope),
            volume_mixer: mixer::build(
                app,
                config,
                services,
                theme,
                &shared.settings,
                monitor,
                scope,
            ),
            sidebar,
            session,
        }
    }

    fn overview(
        &self,
        monitor: &gdk::Monitor,
        config: &Rc<Config>,
        scope: &Scope,
    ) -> Rc<overview::Overview> {
        let shared = &self.shared;
        overview::build(
            &shared.app,
            config,
            &shared.launcher,
            &shared.theme,
            &shared.services.events,
            monitor,
            scope,
        )
    }

    fn finish(
        &self,
        part: Part,
        windows: Vec<gtk4::ApplicationWindow>,
        scope: Scope,
    ) -> PartInstance {
        for window in &windows {
            unload::unrealize_when_hidden(window);
        }
        PartInstance {
            part,
            windows,
            _scope: scope,
        }
    }

    fn update_surfaces(&self, connector: &str, change: impl FnOnce(&mut Surfaces)) {
        if let Some(screen_surfaces) = self
            .surfaces
            .borrow_mut()
            .iter_mut()
            .find(|screen_surfaces| screen_surfaces.connector == connector)
        {
            change(screen_surfaces);
        }
    }
}

fn mark_dirty(screens: &Weak<Screens>, parts: &[Part]) {
    let Some(strong_screens) = screens.upgrade() else {
        return;
    };
    let mut dirty_parts = strong_screens.dirty_parts.borrow_mut();
    let was_clean = dirty_parts.is_empty();
    let affects_bar = parts
        .iter()
        .any(|part| matches!(part, Part::Panels | Part::Overview));
    for part in parts.iter().chain(affects_bar.then_some(&Part::Bar)) {
        if !dirty_parts.contains(part) {
            dirty_parts.push(*part);
        }
    }
    if !was_clean {
        return;
    }
    let screens = screens.clone();
    glib::idle_add_local_once(move || {
        let Some(strong_screens) = screens.upgrade() else {
            return;
        };
        let parts = strong_screens.dirty_parts.take();
        strong_screens.rebuild(&parts);
    });
}

fn discard(part_instance: PartInstance) {
    for window in &part_instance.windows {
        unload::discard(window.upcast_ref());
    }
}
