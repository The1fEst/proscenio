use gtk4::gdk;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, Layer, LayerShell};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::config::{self, Config};
use crate::core::scope::Scope;
use crate::core::watch;
use crate::platform::hypr;
use crate::services::Services;
use crate::services::fullscreen::Fullscreen;
use crate::services::net::Net;
use crate::ui::anim;
use crate::ui::theme::{SharedTheme, mix};
use crate::ui::widgets::paint::Paint;

mod clock;
mod cookie;
mod weather;

const NAMESPACE: &str = "proscenio:background";
const SLIDE: f64 = 600.0;
const ENTRY: f64 = 1000.0;
const MIDDLE: f64 = 0.5;
const VEIL: f32 = 0.75;
const LOCK_ZOOM: f64 = 1.1;
const LOCK_ZOOM_MILLIS: f64 = 400.0;
const LOCK_BLUR_RADIUS: f64 = 100.0;
const LOCK_WASH: f32 = 0.3;
const VIDEOS: [&str; 5] = [".mp4", ".webm", ".mkv", ".avi", ".mov"];
const EVENTS: [&str; 11] = [
    "workspace",
    "workspacev2",
    "focusedmon",
    "focusedmonv2",
    "moveworkspace",
    "moveworkspacev2",
    "openwindow",
    "closewindow",
    "movewindow",
    "movewindowv2",
    "configreloaded",
];

struct Image {
    texture: gdk::Texture,
    width: f64,
    height: f64,
    vertical: bool,
}

struct Background {
    window: gtk4::ApplicationWindow,
    paint: Paint,
    overlay: gtk4::Overlay,
    clock: Rc<clock::Clock>,
    weather: Rc<weather::WeatherWidget>,
    clock_area: RefCell<Option<gdk::Rectangle>>,
    weather_area: RefCell<Option<gdk::Rectangle>>,
    config: Rc<Config>,
    monitor: String,
    screen: (f64, f64),
    source: RefCell<String>,
    image: RefCell<Option<Image>>,
    blurred: RefCell<Option<gdk::Texture>>,
    x: Rc<anim::Motion>,
    y: Rc<anim::Motion>,
    margins: Cell<[i32; 4]>,
    placed: Cell<bool>,
    sidebar_open: Cell<bool>,
    locked: Cell<bool>,
    zoom: Rc<anim::Motion>,
    hidden: Cell<bool>,
    net: Net,
    fullscreen: Fullscreen,
    theme: SharedTheme,
}

pub fn open(
    app: &gtk4::Application,
    config: &Rc<Config>,
    theme: &SharedTheme,
    services: &Services,
    monitor: &gdk::Monitor,
    scope: &Scope,
) -> gtk4::ApplicationWindow {
    let area = monitor.geometry();
    let screen = (area.width() as f64, area.height() as f64);
    let paint = Paint::new(|_, _, _| {});
    let clock = clock::Clock::new(theme, screen);
    let weather = weather::WeatherWidget::new(theme, &services.weather, screen);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&paint));
    overlay.add_overlay(&weather.root);
    overlay.add_overlay(&clock.root);
    let window = gtk4::ApplicationWindow::builder()
        .application(app)
        .child(&overlay)
        .build();
    window.init_layer_shell();
    window.set_namespace(Some(NAMESPACE));
    window.set_monitor(Some(monitor));
    window.set_layer(Layer::Bottom);
    for edge in [Edge::Left, Edge::Right, Edge::Top, Edge::Bottom] {
        window.set_anchor(edge, true);
    }
    window.set_exclusive_zone(-1);
    window.connect_realize(|window| {
        if let Some(surface) = window.surface() {
            surface.set_input_region(Some(&gtk4::cairo::Region::create()));
        }
    });

    let background = Rc::new(Background {
        window: window.clone(),
        x: anim::Motion::new(&clock.root, 0.0, SLIDE, anim::Ease::OutCubic),
        y: anim::Motion::new(&clock.root, 0.0, SLIDE, anim::Ease::OutCubic),
        margins: Cell::new([0; 4]),
        paint: paint.clone(),
        overlay: overlay.clone(),
        clock: clock.clone(),
        weather: weather.clone(),
        clock_area: RefCell::new(None),
        weather_area: RefCell::new(None),
        config: config.clone(),
        monitor: monitor.connector().map(Into::into).unwrap_or_default(),
        screen,
        source: RefCell::new(String::new()),
        image: RefCell::new(None),
        blurred: RefCell::new(None),
        placed: Cell::new(false),
        sidebar_open: Cell::new(false),
        locked: Cell::new(false),
        zoom: anim::Motion::new(&paint, 1.0, LOCK_ZOOM_MILLIS, anim::EXPRESSIVE_DEFAULT),
        hidden: Cell::new(false),
        net: services.net.clone(),
        fullscreen: services.fullscreen.clone(),
        theme: theme.clone(),
    });

    overlay.connect_get_child_position({
        let background = Rc::downgrade(&background);
        move |_, child| {
            let background = background.upgrade()?;
            let width = child.measure(gtk4::Orientation::Horizontal, -1).1;
            let height = child.measure(gtk4::Orientation::Vertical, -1).1;
            let (x, y) = if *child == background.clock.root {
                background.clock.position(width as f64, height as f64)
            } else if *child == background.weather.root {
                background.weather.position(width as f64, height as f64)
            } else {
                return None;
            };
            let (shift_x, shift_y) = background.canvas_shift();
            let [left, top, ..] = background.margins.get();
            let area = gtk4::gdk::Rectangle::new(
                (x + shift_x).round() as i32 - left,
                (y + shift_y).round() as i32 - top,
                width,
                height,
            );
            background.catch_input(child, &area);
            Some(area)
        }
    });
    let reposition = {
        let overlay = overlay.downgrade();
        move || {
            if let Some(overlay) = overlay.upgrade() {
                overlay.queue_allocate();
            }
        }
    };
    clock.connect_moved(reposition.clone());
    weather.connect_moved(reposition);

    paint.set_draw({
        let background = Rc::downgrade(&background);
        move |snapshot, width, height| {
            let Some(background) = background.upgrade() else {
                return;
            };
            if background.hidden.get() {
                let theme = background.theme.borrow();
                let veil = mix(theme.colors.col_layer0, theme.colors.col_primary, VEIL);
                snapshot.append_color(&veil, &graphene::Rect::new(0.0, 0.0, width, height));
                return;
            }
            let image = background.image.borrow();
            let Some(image) = image.as_ref() else {
                return;
            };
            let bounds = graphene::Rect::new(0.0, 0.0, image.width as f32, image.height as f32);
            let locked = background.locked.get();
            let blurred = config::value_bool("/lock/blur/enable", true)
                && (locked || background.zoom.running());
            if !blurred {
                let nearest = gsk::ScalingFilter::Nearest;
                snapshot.append_scaled_texture(&image.texture, nearest, &bounds);
                return;
            }
            let centre = graphene::Point::new(
                bounds.x() + bounds.width() / 2.0,
                bounds.y() + bounds.height() / 2.0,
            );
            let scale = background.zoom.get() as f32;
            snapshot.save();
            snapshot.translate(&centre);
            snapshot.scale(scale, scale);
            snapshot.translate(&graphene::Point::new(-centre.x(), -centre.y()));
            let blurred = background.blurred.borrow();
            let texture = blurred
                .as_ref()
                .filter(|_| locked)
                .unwrap_or(&image.texture);
            snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Linear, &bounds);
            if locked {
                let mut wash = background.theme.borrow().colors.col_layer0;
                wash.set_alpha(wash.alpha() * LOCK_WASH);
                snapshot.append_color(&wash, &bounds);
            }
            snapshot.restore();
        }
    });

    background.reload();
    scope.hold(background.clone());
    scope.keep(services.events.subscribe({
        let background = Rc::downgrade(&background);
        move |event, _| {
            if let Some(background) = background.upgrade()
                && EVENTS.contains(&event)
            {
                background.place();
            }
        }
    }));
    scope.keep(services.fullscreen.subscribe({
        let background = Rc::downgrade(&background);
        move || {
            if let Some(background) = background.upgrade() {
                background.place();
            }
        }
    }));
    scope.keep(services.states.subscribe({
        let background = Rc::downgrade(&background);
        let states = services.states.clone();
        move || {
            let Some(background) = background.upgrade() else {
                return;
            };
            let open = states.sidebar_open.get();
            if background.sidebar_open.replace(open) != open {
                background.place();
            }
            let locked = states.screen_locked.get();
            if background.locked.replace(locked) != locked {
                background.follow_lock();
            }
        }
    }));
    scope.hold(watch::config("/background/wallpaperPath", {
        let background = Rc::downgrade(&background);
        move || {
            if let Some(background) = background.upgrade() {
                background.reload();
                background.check_safety();
            }
        }
    }));
    scope.hold(watch::config("/lock/blur", {
        let background = Rc::downgrade(&background);
        move || {
            if let Some(background) = background.upgrade() {
                background.reblur();
            }
        }
    }));
    scope.hold(watch::config("/workSafety", {
        let background = Rc::downgrade(&background);
        move || {
            if let Some(background) = background.upgrade() {
                background.check_safety();
            }
        }
    }));
    scope.keep(services.net.subscribe({
        let background = Rc::downgrade(&background);
        move || {
            if let Some(background) = background.upgrade() {
                background.check_safety();
            }
        }
    }));
    background.check_safety();

    window.present();
    window
}

impl Background {
    fn canvas_shift(&self) -> (f64, f64) {
        let factor = self.clock.parallax();
        let image = self.image.borrow();
        let Some(image) = image.as_ref() else {
            return (0.0, 0.0);
        };
        let (screen_width, screen_height) = self.screen;
        let base_x = (screen_width - image.width) / 2.0;
        let base_y = (screen_height - image.height) / 2.0;
        (
            (self.x.get() - base_x) * factor,
            (self.y.get() - base_y) * factor,
        )
    }

    fn catch_input(&self, child: &gtk4::Widget, area: &gdk::Rectangle) {
        let held = if *child == self.clock.root {
            &self.clock_area
        } else {
            &self.weather_area
        };
        held.replace(Some(*area));
        let Some(surface) = self.window.surface() else {
            return;
        };
        let region = gtk4::cairo::Region::create();
        let spots = [
            (self.clock.draggable(), &self.clock_area),
            (self.weather.draggable(), &self.weather_area),
        ];
        for (draggable, spot) in spots {
            if let Some(area) = spot.borrow().as_ref().filter(|_| draggable) {
                let _ = region.union_rectangle(&gtk4::cairo::RectangleInt::new(
                    area.x(),
                    area.y(),
                    area.width(),
                    area.height(),
                ));
            }
        }
        surface.set_input_region(Some(&region));
    }

    fn follow_lock(self: &Rc<Self>) {
        let locked = self.locked.get();
        self.clock.set_locked(locked);
        self.weather.set_locked(locked);
        let zoom = if locked {
            config::value_f64("/lock/blur/extraZoom", LOCK_ZOOM)
        } else {
            1.0
        };
        self.zoom.to(zoom);
        self.window.set_layer(Layer::Bottom);
        self.place();
        let background = self.clone();
        self.paint.add_tick_callback(move |paint, _| {
            paint.queue_draw();
            if background.zoom.running() {
                return glib::ControlFlow::Continue;
            }
            if background.locked.get() {
                background.window.set_layer(Layer::Overlay);
            }
            glib::ControlFlow::Break
        });
    }

    fn check_safety(&self) {
        let config = config::current();
        let wallpaper = config_text("/background/wallpaperPath").to_lowercase();
        let network = self.net.name.borrow().to_lowercase();
        let video = VIDEOS.iter().any(|suffix| wallpaper.ends_with(suffix));
        let hidden = config.safety_wallpaper
            && !video
            && contains_any(&wallpaper, &config.safety_files)
            && contains_any(&network, &config.safety_networks);
        if self.hidden.replace(hidden) != hidden {
            self.follow_margins();
            self.paint.queue_draw();
        }
        self.clock.set_safety(hidden);
    }

    fn reload(self: &Rc<Self>) {
        let wallpaper = config_text("/background/wallpaperPath");
        let video = VIDEOS.iter().any(|suffix| wallpaper.ends_with(suffix));
        if self.source.replace(wallpaper.clone()) == wallpaper && self.image.borrow().is_some() {
            return;
        }
        self.clock.wallpaper_changed();
        self.weather.wallpaper_changed();
        if video || wallpaper.is_empty() {
            self.image.replace(None);
            self.blurred.replace(None);
            self.follow_margins();
            self.paint.queue_draw();
            return;
        }
        let Some((_, source_width, source_height)) = Pixbuf::file_info(&wallpaper) else {
            return;
        };
        if source_width <= 0 || source_height <= 0 {
            return;
        }
        let (screen_width, screen_height) = self.screen;
        let cover = (screen_width / source_width as f64).max(screen_height / source_height as f64);
        let scale = cover * self.config.workspace_zoom;
        let width = (source_width as f64 * scale).round() as i32;
        let height = (source_height as f64 * scale).round() as i32;
        let vertical = self.config.parallax_vertical
            || (self.config.parallax_auto_vertical && source_height > source_width);

        let background = self.clone();
        glib::spawn_future_local(async move {
            let path = PathBuf::from(&wallpaper);
            let Some(texture) = crate::ui::image::texture(path, (width, height)).await else {
                return;
            };
            if *background.source.borrow() != wallpaper {
                return;
            }
            background.image.replace(Some(Image {
                width: texture.width() as f64,
                height: texture.height() as f64,
                texture,
                vertical,
            }));
            background.placed.set(false);
            background.place();
            background.paint.queue_draw();
            background.reblur();
        });
    }

    fn reblur(self: &Rc<Self>) {
        self.blurred.replace(None);
        let wallpaper = self.source.borrow().clone();
        let Some(size) = self
            .image
            .borrow()
            .as_ref()
            .map(|image| (image.width as i32, image.height as i32))
        else {
            return;
        };
        let radius = config::value_f64("/lock/blur/radius", LOCK_BLUR_RADIUS);
        if !config::value_bool("/lock/blur/enable", true) || radius <= 0.0 {
            return;
        }
        let background = self.clone();
        glib::spawn_future_local(async move {
            let path = PathBuf::from(&wallpaper);
            let sigma = gsk_blur(radius) / 2.0;
            let Some(texture) = crate::ui::image::blurred_texture(path, size, sigma).await else {
                return;
            };
            let current = config::value_f64("/lock/blur/radius", LOCK_BLUR_RADIUS);
            if *background.source.borrow() != wallpaper || current != radius {
                return;
            }
            background.blurred.replace(Some(texture));
            if background.locked.get() {
                background.paint.queue_draw();
            }
        });
    }

    fn follow_margins(&self) {
        let (left, top, right, bottom) = match (self.image.borrow().as_ref(), self.hidden.get()) {
            (Some(image), false) => {
                let (screen_width, screen_height) = self.screen;
                let left = self.x.get().round();
                let top = self.y.get().round();
                (
                    left,
                    top,
                    screen_width - left - image.width,
                    screen_height - top - image.height,
                )
            }
            _ => (0.0, 0.0, 0.0, 0.0),
        };
        let margins = [left, top, right, bottom].map(|margin| margin as i32);
        if self.margins.replace(margins) == margins {
            return;
        }
        for (edge, margin) in [Edge::Left, Edge::Top, Edge::Right, Edge::Bottom]
            .into_iter()
            .zip(margins)
        {
            self.window.set_margin(edge, margin);
        }
    }

    fn target(&self, workspace_fraction: f64) -> Option<(f64, f64)> {
        let fraction = if self.config.parallax_workspace {
            workspace_fraction
        } else {
            MIDDLE
        };
        self.position(fraction)
    }

    fn position(&self, fraction: f64) -> Option<(f64, f64)> {
        let image = self.image.borrow();
        let image = image.as_ref()?;
        let mut part_x = if image.vertical { MIDDLE } else { fraction };
        let part_y = if image.vertical { fraction } else { MIDDLE };
        if self.config.parallax_sidebar && self.sidebar_open.get() {
            part_x += self.config.workspace_zoom / self.config.workspaces_shown as f64 / 2.0;
        }
        let (screen_width, screen_height) = self.screen;
        let x = if screen_width > image.width {
            (screen_width - image.width) / 2.0
        } else {
            -(image.width - screen_width) * part_x.clamp(0.0, 1.0)
        };
        let y = if screen_height > image.height {
            (screen_height - image.height) / 2.0
        } else {
            -(image.height - screen_height) * part_y.clamp(0.0, 1.0)
        };
        Some((x, y))
    }

    fn place(self: &Rc<Self>) {
        let view = look(&self.monitor, self.config.workspaces_shown);
        let covered = self.fullscreen.covers(&self.monitor);
        self.window
            .set_visible(self.locked.get() || !(self.config.hide_when_fullscreen && covered));

        let Some((x, y)) = self.target(view.fraction) else {
            return;
        };
        if self.placed.replace(true) {
            self.x.to(x);
            self.y.to(y);
        } else {
            let (from_x, from_y) = self.position(glib::random_double()).unwrap_or((x, y));
            self.x.jump(from_x);
            self.y.jump(from_y);
            self.x.to_over(x, ENTRY);
            self.y.to_over(y, ENTRY);
        }
        self.follow_margins();
        let background = self.clone();
        self.overlay.add_tick_callback(move |overlay, _| {
            background.follow_margins();
            overlay.queue_allocate();
            if background.x.running() || background.y.running() {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }
}

fn gsk_blur(radius: f64) -> f64 {
    3.0 * (radius + 1.0) / 3.3333
}

fn config_text(pointer: &str) -> String {
    config::value(pointer)
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn contains_any(text: &str, keywords: &[String]) -> bool {
    keywords
        .iter()
        .any(|keyword| text.contains(keyword.as_str()))
}

struct View {
    fraction: f64,
}

fn look(monitor: &str, chunk: i32) -> View {
    let mut active = 1;
    let mut id = None;
    if let Some(monitors) = hypr::json("monitors").and_then(|value| value.as_array().cloned())
        && let Some(mine) = monitors
            .iter()
            .find(|entry| entry.get("name").and_then(Value::as_str) == Some(monitor))
    {
        active = mine
            .pointer("/activeWorkspace/id")
            .and_then(Value::as_i64)
            .unwrap_or(1) as i32;
        id = mine.get("id").and_then(Value::as_i64);
    }

    let mut last = active;
    if let Some(clients) = hypr::json("clients").and_then(|value| value.as_array().cloned()) {
        for client in clients
            .iter()
            .filter(|client| client.get("monitor").and_then(Value::as_i64) == id)
        {
            let Some(workspace) = client.pointer("/workspace/id").and_then(Value::as_i64) else {
                continue;
            };
            if workspace < 0 {
                continue;
            }
            last = last.max(workspace as i32);
        }
    }

    let chunk = chunk.max(1);
    let total = (last as f64 / chunk as f64).ceil() as i32 * chunk;
    View {
        fraction: if total <= 1 {
            MIDDLE
        } else {
            ((active - 1) as f64 / (total - 1) as f64).clamp(0.0, 1.0)
        },
    }
}
