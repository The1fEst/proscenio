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
use crate::services::Services;
use crate::services::hyprland::fullscreen::Fullscreen;
use crate::services::hyprland::hyprstate::{HyprState, Snapshot};
use crate::services::network::net::Net;
use crate::ui::anim;
use crate::ui::theme::{SharedTheme, mix};
use crate::ui::widgets::display::paint::Paint;

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

struct Image {
    texture: gdk::Texture,
    width: f64,
    height: f64,
    is_vertical: bool,
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
    blur_texture: RefCell<Option<gdk::Texture>>,
    is_blur_enabled: Cell<bool>,
    x: Rc<anim::Motion>,
    y: Rc<anim::Motion>,
    margins: Cell<[i32; 4]>,
    is_sliding: Cell<bool>,
    is_placed: Cell<bool>,
    is_sidebar_open: Cell<bool>,
    is_locked: Cell<bool>,
    zoom: Rc<anim::Motion>,
    is_safety_enforced: Cell<bool>,
    net: Net,
    fullscreen: Fullscreen,
    hypr: HyprState,
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
        is_sliding: Cell::new(false),
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
        blur_texture: RefCell::new(None),
        is_blur_enabled: Cell::new(config::value_bool("/lock/blur/enable", true)),
        is_placed: Cell::new(false),
        is_sidebar_open: Cell::new(false),
        is_locked: Cell::new(false),
        zoom: anim::Motion::new(&paint, 1.0, LOCK_ZOOM_MILLIS, anim::EXPRESSIVE_DEFAULT),
        is_safety_enforced: Cell::new(false),
        net: services.net.clone(),
        fullscreen: services.fullscreen.clone(),
        hypr: services.hypr.clone(),
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
            if background.is_safety_enforced.get() {
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
            let is_locked = background.is_locked.get();
            let is_blurred =
                background.is_blur_enabled.get() && (is_locked || background.zoom.is_running());
            if !is_blurred {
                let nearest = gsk::ScalingFilter::Nearest;
                snapshot.append_scaled_texture(&image.texture, nearest, &bounds);
                return;
            }
            let center = graphene::Point::new(
                bounds.x() + bounds.width() / 2.0,
                bounds.y() + bounds.height() / 2.0,
            );
            let scale = background.zoom.get() as f32;
            snapshot.save();
            snapshot.translate(&center);
            snapshot.scale(scale, scale);
            snapshot.translate(&graphene::Point::new(-center.x(), -center.y()));
            let blur_texture = background.blur_texture.borrow();
            let texture = blur_texture
                .as_ref()
                .filter(|_| is_locked)
                .unwrap_or(&image.texture);
            snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Linear, &bounds);
            if is_locked {
                let mut wash = background.theme.borrow().colors.col_layer0;
                wash.set_alpha(wash.alpha() * LOCK_WASH);
                snapshot.append_color(&wash, &bounds);
            }
            snapshot.restore();
        }
    });

    background.reload();
    scope.hold(background.clone());
    scope.keep(services.hypr.subscribe({
        let background = Rc::downgrade(&background);
        move || {
            if let Some(background) = background.upgrade() {
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
            let is_sidebar_open = states.is_sidebar_open.get();
            if background.is_sidebar_open.replace(is_sidebar_open) != is_sidebar_open {
                background.place();
            }
            let is_locked = states.is_screen_locked.get();
            if background.is_locked.replace(is_locked) != is_locked {
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
        let child_area = if *child == self.clock.root {
            &self.clock_area
        } else {
            &self.weather_area
        };
        child_area.replace(Some(*area));
        let Some(surface) = self.window.surface() else {
            return;
        };
        let region = gtk4::cairo::Region::create();
        let spots = [
            (self.clock.is_draggable(), &self.clock_area),
            (self.weather.is_draggable(), &self.weather_area),
        ];
        for (is_draggable, spot) in spots {
            if let Some(area) = spot.borrow().as_ref().filter(|_| is_draggable) {
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
        let is_locked = self.is_locked.get();
        self.clock.set_locked(is_locked);
        self.weather.set_locked(is_locked);
        let zoom = if is_locked {
            config::value_f64("/lock/blur/extraZoom", LOCK_ZOOM)
        } else {
            1.0
        };
        self.zoom.to(zoom);
        self.window.set_layer(Layer::Bottom);
        self.place();
        let background = Rc::downgrade(self);
        self.paint.add_tick_callback(move |paint, _| {
            let Some(background) = background.upgrade() else {
                return glib::ControlFlow::Break;
            };
            paint.queue_draw();
            if background.zoom.is_running() {
                return glib::ControlFlow::Continue;
            }
            if background.is_locked.get() {
                background.window.set_layer(Layer::Overlay);
            }
            glib::ControlFlow::Break
        });
    }

    fn check_safety(&self) {
        let config = config::current();
        let wallpaper = config_text("/background/wallpaperPath").to_lowercase();
        let network = self.net.name.borrow().to_lowercase();
        let is_video = VIDEOS.iter().any(|suffix| wallpaper.ends_with(suffix));
        let is_safety_enforced = config.safety_wallpaper
            && !is_video
            && contains_any(&wallpaper, &config.safety_files)
            && contains_any(&network, &config.safety_networks);
        if self.is_safety_enforced.replace(is_safety_enforced) != is_safety_enforced {
            self.follow_margins();
            self.paint.queue_draw();
        }
        self.clock.set_safety_enforced(is_safety_enforced);
    }

    fn reload(self: &Rc<Self>) {
        let wallpaper = config_text("/background/wallpaperPath");
        let is_video = VIDEOS.iter().any(|suffix| wallpaper.ends_with(suffix));
        if self.source.replace(wallpaper.clone()) == wallpaper && self.image.borrow().is_some() {
            return;
        }
        self.clock.on_wallpaper_changed();
        self.weather.on_wallpaper_changed();
        if is_video || wallpaper.is_empty() {
            self.image.replace(None);
            self.blur_texture.replace(None);
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
        let is_vertical = self.config.parallax_vertical
            || (self.config.parallax_auto_vertical && source_height > source_width);

        let background = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let path = PathBuf::from(&wallpaper);
            let Some(texture) = crate::ui::image::texture(path, (width, height)).await else {
                return;
            };
            let Some(background) = background.upgrade() else {
                return;
            };
            if *background.source.borrow() != wallpaper {
                return;
            }
            background.image.replace(Some(Image {
                width: texture.width() as f64,
                height: texture.height() as f64,
                texture,
                is_vertical,
            }));
            background.is_placed.set(false);
            background.place();
            background.paint.queue_draw();
            background.reblur();
        });
    }

    fn reblur(self: &Rc<Self>) {
        self.blur_texture.replace(None);
        self.is_blur_enabled
            .set(config::value_bool("/lock/blur/enable", true));
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
        if !self.is_blur_enabled.get() || radius <= 0.0 {
            return;
        }
        let background = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let path = PathBuf::from(&wallpaper);
            let sigma = gsk_blur(radius) / 2.0;
            let Some(texture) = crate::ui::image::blurred_texture(path, size, sigma).await else {
                return;
            };
            let Some(background) = background.upgrade() else {
                return;
            };
            let current = config::value_f64("/lock/blur/radius", LOCK_BLUR_RADIUS);
            if *background.source.borrow() != wallpaper || current != radius {
                return;
            }
            background.blur_texture.replace(Some(texture));
            if background.is_locked.get() {
                background.paint.queue_draw();
            }
        });
    }

    fn follow_margins(&self) {
        let (left, top, right, bottom) =
            match (self.image.borrow().as_ref(), self.is_safety_enforced.get()) {
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
        let mut part_x = if image.is_vertical { MIDDLE } else { fraction };
        let part_y = if image.is_vertical { fraction } else { MIDDLE };
        if self.config.parallax_sidebar && self.is_sidebar_open.get() {
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
        let view = look(
            &self.hypr.snapshot(),
            &self.monitor,
            self.config.workspaces_shown,
        );
        let is_covered = self.fullscreen.covers(&self.monitor);
        self.window
            .set_visible(self.is_locked.get() || !(self.config.hide_when_fullscreen && is_covered));

        let Some((x, y)) = self.target(view.fraction) else {
            return;
        };
        if self.is_placed.replace(true) {
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
        if self.is_sliding.replace(true) {
            return;
        }
        let background = Rc::downgrade(self);
        self.overlay.add_tick_callback(move |overlay, _| {
            let Some(background) = background.upgrade() else {
                return glib::ControlFlow::Break;
            };
            background.follow_margins();
            overlay.queue_allocate();
            if background.x.is_running() || background.y.is_running() {
                return glib::ControlFlow::Continue;
            }
            background.is_sliding.set(false);
            glib::ControlFlow::Break
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

fn look(snapshot: &Snapshot, monitor: &str, chunk: i32) -> View {
    let mut active = 1;
    let mut id = None;
    if let Some(monitors) = &snapshot.monitors
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
    if let Some(clients) = &snapshot.clients {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_view_comes_from_the_shared_snapshot() {
        let snapshot = Snapshot {
            monitors: Some(vec![
                serde_json::json!({"name": "DP-1", "id": 0, "activeWorkspace": {"id": 3}}),
                serde_json::json!({"name": "DP-2", "id": 1, "activeWorkspace": {"id": 12}}),
            ]),
            clients: Some(vec![
                serde_json::json!({"monitor": 0, "workspace": {"id": 4}}),
                serde_json::json!({"monitor": 1, "workspace": {"id": 25}}),
            ]),
            ..Default::default()
        };
        assert_eq!(look(&snapshot, "DP-1", 5).fraction, 0.5);
        assert_eq!(look(&snapshot, "DP-2", 10).fraction, 11.0 / 29.0);
    }
}
