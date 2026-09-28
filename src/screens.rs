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
    background, calendar, cheatsheet, corners, dock, mediacontrols, sessionscreen, settings,
    sidebar,
};
use crate::platform::hypr;
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
                "/dock/height",
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
}

impl Panels {
    fn windows(&self) -> Vec<gtk4::ApplicationWindow> {
        vec![
            self.sidebar.window.clone(),
            self.calendar.window.clone(),
            self.session.window.clone(),
            self.media.window.clone(),
        ]
    }
}

struct Built {
    part: Part,
    windows: Vec<gtk4::ApplicationWindow>,
    _scope: Scope,
}

struct Screen {
    monitor: gdk::Monitor,
    geometry: gdk::Rectangle,
    built: Vec<Built>,
}

pub struct Surfaces {
    pub connector: String,
    pub sidebar: Rc<sidebar::Sidebar>,
    pub calendar: Rc<calendar::Calendar>,
    pub session: Rc<sessionscreen::SessionScreen>,
    pub media: Rc<mediacontrols::MediaControls>,
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
    dirty: RefCell<Vec<Part>>,
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
            dirty: RefCell::new(Vec::new()),
            rounding: Cell::new(config.rounding),
            _watches: RefCell::new(Vec::new()),
        });
        let mut watches = Vec::new();
        for part in PARTS {
            for pointer in part.pointers() {
                let screens = Rc::downgrade(&screens);
                watches.push(watch::config_except(pointer, part.ignored(), move || {
                    mark(&screens, &[part])
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
                    let Some(held) = screens.upgrade() else {
                        return;
                    };
                    let rounding = hypr::option_int("decoration:rounding").unwrap_or(23);
                    if held.rounding.replace(rounding) != rounding {
                        mark(&screens, &[Part::Bar]);
                    }
                }
            })
            .forever();
        screens
    }

    /// Rebuilds every part, for changes that reach all of them, such as the fonts.
    pub fn rebuild_all(self: &Rc<Self>) {
        mark(&Rc::downgrade(self), &PARTS);
    }

    pub fn rebuild_icon_users(self: &Rc<Self>) {
        let parts: Vec<Part> = PARTS
            .into_iter()
            .filter(|part| *part != Part::Background)
            .collect();
        mark(&Rc::downgrade(self), &parts);
    }

    fn refresh_config(&self) {
        let fresh = Rc::new(Config::load());
        self.rounding.set(fresh.rounding);
        self.config.replace(fresh);
    }

    pub fn sync(&self) {
        let config = self.config.borrow().clone();
        let services = &self.shared.services;
        let present: Vec<gdk::Monitor> = gdk::Display::default()
            .map(|display| display.monitors())
            .into_iter()
            .flat_map(|monitors| {
                monitors
                    .iter::<gdk::Monitor>()
                    .flatten()
                    .collect::<Vec<_>>()
            })
            .collect();
        let names: Vec<String> = present
            .iter()
            .map(crate::connector)
            .filter(|name| !name.is_empty())
            .collect();
        services.light.set_screens(names.clone());
        let listed_present = config.screens.iter().any(|name| names.contains(name));
        let wanted = |name: &str| !listed_present || config.wants_screen(name);
        let gone: Vec<Screen> = {
            let mut screens = self.screens.borrow_mut();
            let (kept, gone) =
                std::mem::take(&mut *screens)
                    .into_iter()
                    .partition(|screen: &Screen| {
                        present.contains(&screen.monitor)
                            && screen.monitor.geometry() == screen.geometry
                            && wanted(&crate::connector(&screen.monitor))
                    });
            *screens = kept;
            gone
        };
        let live: Vec<String> = self
            .screens
            .borrow()
            .iter()
            .map(|screen| crate::connector(&screen.monitor))
            .collect();
        self.surfaces
            .borrow_mut()
            .retain(|held| live.contains(&held.connector));
        for screen in gone {
            for built in screen.built {
                discard(built);
            }
        }

        for item in present {
            let ready = !crate::connector(&item).is_empty() && item.geometry().width() > 0;
            if !ready || !wanted(&crate::connector(&item)) {
                continue;
            }
            if self
                .screens
                .borrow()
                .iter()
                .any(|screen| screen.monitor == item)
            {
                continue;
            }
            let mut built = self.first(&item, &config);
            for part in PARTS {
                if matches!(part, Part::Panels | Part::Overview | Part::Sheet) {
                    continue;
                }
                if let Some(part) = self.build(part, &item, &config) {
                    built.push(part);
                }
            }
            self.screens.borrow_mut().push(Screen {
                geometry: item.geometry(),
                monitor: item,
                built,
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
                let old = {
                    let mut screens = self.screens.borrow_mut();
                    let Some(screen) = screens.iter_mut().find(|screen| screen.monitor == monitor)
                    else {
                        continue;
                    };
                    screen
                        .built
                        .iter()
                        .position(|built| built.part == part)
                        .map(|index| screen.built.remove(index))
                };
                if let Some(old) = old {
                    discard(old);
                }
                let Some(fresh) = self.build(part, &monitor, &config) else {
                    continue;
                };
                if let Some(screen) = self
                    .screens
                    .borrow_mut()
                    .iter_mut()
                    .find(|screen| screen.monitor == monitor)
                {
                    screen.built.push(fresh);
                }
            }
        }
    }

    fn build(&self, part: Part, monitor: &gdk::Monitor, config: &Rc<Config>) -> Option<Built> {
        let shared = &self.shared;
        let (app, theme, services) = (&shared.app, &shared.theme, &shared.services);
        let connector = crate::connector(monitor);
        let scope = Scope::default();
        let windows = match part {
            Part::Panels => {
                let panels = self.panels(monitor, config, &scope);
                let windows = panels.windows();
                self.hold(&connector, |held| {
                    held.sidebar = panels.sidebar;
                    held.calendar = panels.calendar;
                    held.session = panels.session;
                    held.media = panels.media;
                });
                windows
            }
            Part::Overview => {
                let overview = self.overview(monitor, config, &scope);
                let window = overview.window.clone();
                self.hold(&connector, |held| held.overview = overview);
                vec![window]
            }
            Part::Sheet => {
                let sheet = cheatsheet::build(app, config, monitor);
                let window = sheet.window.clone();
                self.hold(&connector, |held| held.sheet = sheet);
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
                let held = surfaces.iter().find(|held| held.connector == connector)?;
                let mut windows = vec![crate::open_bar(
                    app,
                    config,
                    theme,
                    services,
                    &held.sidebar,
                    &held.calendar,
                    &held.media,
                    &held.overview,
                    monitor,
                    &scope,
                )];
                windows.extend(corners::open(
                    app,
                    config,
                    services,
                    &held.sidebar,
                    monitor,
                    &scope,
                ));
                windows
            }
        };
        Some(self.finish(part, windows, scope))
    }

    /// Builds the parts other parts and the IPC hold on to, and records them for the monitor.
    fn first(&self, monitor: &gdk::Monitor, config: &Rc<Config>) -> Vec<Built> {
        let (panels_scope, overview_scope) = (Scope::default(), Scope::default());
        let panels = self.panels(monitor, config, &panels_scope);
        let overview = self.overview(monitor, config, &overview_scope);
        let sheet = cheatsheet::build(&self.shared.app, config, monitor);
        let built = vec![
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
            sheet,
            overview,
        });
        built
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
        scope.keep(sidebar.watch({
            let notifications = services.notifications.clone();
            let states = services.states.clone();
            move |open| {
                notifications.set_inhibited(open);
                states.set_sidebar_open(open);
            }
        }));
        Panels {
            calendar: calendar::build(app, config, theme, services, monitor, scope),
            media: mediacontrols::build(app, config, services, theme, monitor, scope),
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

    fn finish(&self, part: Part, windows: Vec<gtk4::ApplicationWindow>, scope: Scope) -> Built {
        for window in &windows {
            unload::when_hidden(window);
        }
        Built {
            part,
            windows,
            _scope: scope,
        }
    }

    fn hold(&self, connector: &str, change: impl FnOnce(&mut Surfaces)) {
        if let Some(held) = self
            .surfaces
            .borrow_mut()
            .iter_mut()
            .find(|held| held.connector == connector)
        {
            change(held);
        }
    }
}

fn mark(screens: &Weak<Screens>, parts: &[Part]) {
    let Some(held) = screens.upgrade() else {
        return;
    };
    let mut dirty = held.dirty.borrow_mut();
    let idle = dirty.is_empty();
    let holders = parts
        .iter()
        .any(|part| matches!(part, Part::Panels | Part::Overview));
    for part in parts.iter().chain(holders.then_some(&Part::Bar)) {
        if !dirty.contains(part) {
            dirty.push(*part);
        }
    }
    if !idle {
        return;
    }
    let screens = screens.clone();
    glib::idle_add_local_once(move || {
        let Some(held) = screens.upgrade() else {
            return;
        };
        let parts = held.dirty.take();
        held.rebuild(&parts);
    });
}

fn discard(built: Built) {
    for window in &built.windows {
        unload::discard(window.upcast_ref());
    }
}
