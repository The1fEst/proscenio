pub mod screenshot;

use gtk4::gdk::{self, RGBA};
use gtk4::glib;
use gtk4::graphene;
use gtk4::gsk;
use gtk4::prelude::*;
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::config::{self, Config};
use crate::core::{persistent, process};
use crate::platform::hypr;
use crate::services::recording::Recording;
use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Tween};
use crate::ui::theme::{SharedTheme, mix, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::{self, Centred};
use crate::ui::widgets::controls::ConfigSwitch;
use crate::ui::widgets::group::{GroupButton, Look as GroupLook};
use crate::ui::widgets::paint::Paint;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::text::{self, Family};
use crate::ui::widgets::toolbar::{self, TabBar};
use crate::ui::widgets::tooltip::{self, Tooltip};
use screenshot::{Action, Crop};

const NAMESPACE: &str = "proscenio:regionSelector";
const OVERLAY_ALPHA: f32 = 0.6;
const DASH: f64 = 8.0;
const GAP: f64 = 4.0;
const LABEL_MARGIN: f64 = 8.0;
const AIM_OPACITY: f32 = 0.2;
const REGION_RADIUS: f64 = rounding::WINDOW_ROUNDING as f64;
const REGION_BORDER: f32 = 2.0;
const REGION_BORDER_TARGETED: f32 = 4.0;
const REGION_TEXT_PADDING: f64 = 10.0;
const TAG_PADDING: (f64, f64) = (10.0, 5.0);
const TAG_RADIUS: f64 = 10.0;
const TAG_SPACING: f64 = 4.0;
const FAST_MILLIS: f64 = 200.0;
const MOVE_MILLIS: f64 = 500.0;
const BREATH_MILLIS: f64 = 1200.0;
const BREATH_HIGH: f64 = 0.9;
const BREATH_LOW: f64 = 0.3;
const GUIDE_MARGIN: i32 = 8;
const GUIDE_HEIGHT: i32 = 38;
const GUIDE_PADDING: i32 = 8;
const GUIDE_CORNER: f64 = 6.0;
const GUIDE_SPACING: i32 = 12;
const GUIDE_TEXT_INSET: i32 = 6;
const GUIDE_ICON: f64 = 22.0;
const DESCRIPTION_TIMEOUT: Duration = Duration::from_secs(1);
const TOOLBAR_ICON: f64 = 22.0;
const ARROW_ICON: f64 = 20.0;
const OPTIONS_SPACING: i32 = 6;
const CONTROLS_SPACING: i32 = 6;
const CONTROLS_MARGIN: i32 = 8;
const MENU_WIDTH: i32 = 340;
const MENU_SPACING: i32 = 2;
const LABEL_INSET: i32 = 2;
const CHOICE_PADDING: (i32, i32) = (12, 8);
const CHOICE_SQUARE: f64 = rounding::UNSHARPENMORE as f64;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Screen,
    Window,
    Region,
    Shape,
    RecordScreen,
    RecordRegion,
}

impl Mode {
    fn whole_screen(self) -> bool {
        matches!(self, Mode::Screen | Mode::RecordScreen)
    }

    fn recording(self) -> bool {
        matches!(self, Mode::RecordScreen | Mode::RecordRegion)
    }

    fn circle(self) -> bool {
        self == Mode::Shape
    }

    fn picks_region(self) -> bool {
        matches!(self, Mode::Window | Mode::Region)
    }
}

const MODES: [(Mode, &str, &str); 6] = [
    (Mode::Screen, "fullscreen", "Whole screen"),
    (
        Mode::Window,
        "select_window",
        "With rounded corners and a shadow",
    ),
    (Mode::Region, "activity_zone", "As it is on screen"),
    (Mode::Shape, "gesture", "A drawn shape"),
    (Mode::RecordScreen, "screen_record", "Record the screen"),
    (Mode::RecordRegion, "screenshot_region", "Record a region"),
];

const COUNTDOWNS: [(i64, &str); 3] = [(0, "None"), (5, "5s"), (10, "10s")];

fn mode_index(mode: Mode) -> usize {
    MODES
        .iter()
        .position(|(candidate, _, _)| *candidate == mode)
        .unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    Select,
    Post,
}

#[derive(Clone)]
struct Target {
    bounds: [f64; 4],
    label: String,
    radius: f64,
}

#[derive(Default)]
struct Drag {
    start: (f64, f64),
    at: (f64, f64),
    diff: (f64, f64),
    dragging: bool,
    points: Vec<(f64, f64)>,
}

struct Guide {
    widget: gtk4::Overlay,
    background: Paint,
    symbol: Rc<dyn Fn(&str)>,
    description: gtk4::Label,
    width: Cell<Tween>,
    fade: Cell<Tween>,
    shown: Cell<bool>,
    timeout: RefCell<Option<glib::SourceId>>,
}

struct Controls {
    holder: text::Shift,
    tabs: Rc<TabBar>,
    capture: RippleButton,
    capture_symbol: gtk4::Label,
    capture_tip: Rc<Tooltip>,
    options: RippleButton,
    arrow: gtk4::Label,
    menu: gtk4::Box,
    open: Cell<bool>,
    rise: Cell<Tween>,
    appear: Cell<Tween>,
    _switches: Vec<Rc<ConfigSwitch>>,
    _countdown: Rc<dyn Fn(bool)>,
}

struct Selection {
    selector: Weak<RegionSelector>,
    controls: Controls,
    config: Rc<Config>,
    theme: SharedTheme,
    window: gtk4::ApplicationWindow,
    paint: Paint,
    layer: gtk4::Fixed,
    guide: Guide,
    screen: String,
    size: (f64, f64),
    scale: f64,
    offset: (f64, f64),
    path: PathBuf,
    mode: Cell<Mode>,
    phase: Cell<Phase>,
    frozen: RefCell<Option<gdk::Texture>>,
    drag: RefCell<Drag>,
    region: Cell<[f64; 4]>,
    targeted: Cell<Option<[f64; 4]>>,
    windows: Vec<Target>,
    layers: Vec<Target>,
    fills: RefCell<Vec<Tween>>,
    fade: Cell<Tween>,
    button: Cell<u32>,
    pointer: Cell<(f64, f64)>,
    action: Cell<Action>,
    breathing: Cell<i64>,
    ticking: Cell<bool>,
    snipped: Cell<bool>,
}

impl Selection {
    fn now(&self) -> i64 {
        self.paint
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    fn action(&self) -> Action {
        if self.mode.get().recording() {
            return if self.config.region.record_sound.get() {
                Action::RecordWithSound
            } else {
                Action::Record
            };
        }
        if self.button.get() == gdk::BUTTON_SECONDARY {
            Action::Edit
        } else {
            Action::Copy
        }
    }

    fn action_changed(self: &Rc<Self>) {
        let action = self.action();
        if self.action.replace(action) == action {
            return;
        }
        (self.guide.symbol)(guide_symbol(action));
        self.guide.description.set_text(guide_description(action));
        self.show_description();
    }

    fn show_description(self: &Rc<Self>) {
        self.set_description(true);
        if let Some(source) = self.guide.timeout.borrow_mut().take() {
            source.remove();
        }
        let selection = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(DESCRIPTION_TIMEOUT, move || {
            if let Some(selection) = selection.upgrade() {
                selection.guide.timeout.borrow_mut().take();
                selection.set_description(false);
            }
        });
        self.guide.timeout.replace(Some(source));
    }

    fn set_description(self: &Rc<Self>, shown: bool) {
        self.guide.shown.set(shown);
        let now = self.now();
        let mut width = self.guide.width.get();
        width.retarget(self.guide_width(shown), now);
        self.guide.width.set(width);
        let mut fade = self.guide.fade.get();
        fade.retarget(if shown { 1.0 } else { 0.0 }, now);
        self.guide.fade.set(fade);
        self.animate();
    }

    fn guide_width(&self, shown: bool) -> f64 {
        if !shown {
            return GUIDE_HEIGHT as f64;
        }
        let row = self.guide.widget.last_child();
        let natural = row
            .map(|row| row.measure(gtk4::Orientation::Horizontal, -1).1)
            .unwrap_or(0);
        (natural + GUIDE_PADDING + GUIDE_TEXT_INSET) as f64
    }

    fn set_mode(self: &Rc<Self>, mode: Mode) {
        self.mode.set(mode);
        self.controls.tabs.set_current(mode_index(mode));
        self.show_mode_region();
        self.action_changed();
        self.sync_capture();
        self.animate();
    }

    fn sync_capture(&self) {
        let mode = self.mode.get();
        let controls = &self.controls;
        controls.capture.set_visible(mode.whole_screen());
        let (icon, tip) = if mode.recording() {
            ("screen_record", "Record")
        } else {
            ("photo_camera", "Capture")
        };
        controls.capture_symbol.set_text(icon);
        controls.capture_tip.set_text(tip);
    }

    fn toggle_options(&self) {
        let controls = &self.controls;
        let open = !controls.open.get();
        controls.open.set(open);
        controls.options.set_toggled(open);
        controls.menu.set_visible(open);
        controls.arrow.set_text(if open {
            "keyboard_arrow_down"
        } else {
            "keyboard_arrow_up"
        });
    }

    fn rise(self: &Rc<Self>) {
        let now = self.now();
        let height = self
            .controls
            .holder
            .measure(gtk4::Orientation::Vertical, -1)
            .1;
        let mut rise = self.controls.rise.get();
        rise.jump((height + CONTROLS_MARGIN) as f64);
        rise.retarget(0.0, now);
        self.controls.rise.set(rise);
        let mut appear = self.controls.appear.get();
        appear.retarget(1.0, now);
        self.controls.appear.set(appear);
        self.animate();
    }

    fn show_mode_region(&self) {
        let mut drag = self.drag.borrow_mut();
        if self.mode.get().whole_screen() {
            drag.start = (0.0, 0.0);
            drag.at = self.size;
        } else {
            drag.start = (0.0, 0.0);
            drag.at = (0.0, 0.0);
            if let Some((start, end)) = self.remembered() {
                drag.start = start;
                drag.at = end;
            }
        }
        let region = region_of(&drag);
        drop(drag);
        self.region.set(region);
    }

    fn remembered(&self) -> Option<((f64, f64), (f64, f64))> {
        if !self.config.region.remember_region.get() {
            return None;
        }
        let saved = persistent::read(&["regionSelector"])?;
        let number = |key: &str| saved.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        if saved.get("screen").and_then(Value::as_str) != Some(self.screen.as_str())
            || number("width") <= 0.0
            || number("height") <= 0.0
        {
            return None;
        }
        let (x, y) = (number("x"), number("y"));
        Some(((x, y), (x + number("width"), y + number("height"))))
    }

    fn remember(&self, [x, y, width, height]: [f64; 4]) {
        persistent::write(
            &["regionSelector"],
            serde_json::json!({
                "screen": self.screen,
                "x": x,
                "y": y,
                "width": width,
                "height": height,
            }),
        );
    }

    fn update_targeted(&self, x: f64, y: f64) {
        let hit = |target: &&Target| {
            let [left, top, width, height] = target.bounds;
            left <= x && x <= left + width && top <= y && y <= top + height
        };
        let found = self
            .layers
            .iter()
            .find(hit)
            .or_else(|| self.windows.iter().find(hit))
            .map(|target| target.bounds);
        if self.targeted.replace(found) == found {
            return;
        }
        let now = self.now();
        let mut fills = self.fills.borrow_mut();
        for (tween, target) in fills
            .iter_mut()
            .zip(self.windows.iter().chain(self.layers.iter()))
        {
            let on = found == Some(target.bounds);
            tween.retarget(if on { 1.0 } else { 0.0 }, now);
        }
    }

    fn press(self: &Rc<Self>, button: u32, x: f64, y: f64) {
        self.button.set(button);
        self.action_changed();
        if self.mode.get().whole_screen() {
            return;
        }
        let mut drag = self.drag.borrow_mut();
        drag.start = (x, y);
        drag.at = (x, y);
        drag.dragging = true;
        let region = region_of(&drag);
        drop(drag);
        self.region.set(region);
        self.animate();
    }

    fn motion(self: &Rc<Self>, x: f64, y: f64) {
        self.pointer.set((x, y));
        self.place_guide();
        if self.mode.get().whole_screen() {
            self.animate();
            return;
        }
        self.update_targeted(x, y);
        let mut drag = self.drag.borrow_mut();
        if drag.dragging {
            let was_away = drag.diff != (0.0, 0.0);
            drag.at = (x, y);
            drag.diff = (x - drag.start.0, y - drag.start.1);
            drag.points.push((x, y));
            let region = region_of(&drag);
            let away = drag.diff != (0.0, 0.0);
            drop(drag);
            self.region.set(region);
            self.place_guide();
            if away && !was_away {
                let mut fade = self.fade.get();
                fade.retarget(0.0, self.now());
                self.fade.set(fade);
            }
        } else {
            drop(drag);
        }
        self.animate();
    }

    fn release(self: &Rc<Self>) {
        if self.mode.get().whole_screen() {
            self.capture_whole_screen();
            return;
        }
        let drag = std::mem::take(&mut *self.drag.borrow_mut());
        if drag.points.iter().all(|point| *point == drag.start) {
            if let Some([x, y, width, height]) = self.targeted.get() {
                let padding = if self.mode.get() == Mode::Window {
                    0.0
                } else {
                    self.config.region.selection_padding
                };
                self.region.set([
                    x - padding,
                    y - padding,
                    width + padding * 2.0,
                    height + padding * 2.0,
                ]);
            }
        } else if self.mode.get().circle() {
            let padding =
                self.config.region.circle_padding + self.config.region.circle_stroke / 2.0;
            let points = if drag.points.is_empty() {
                vec![self.pointer.get()]
            } else {
                drag.points.clone()
            };
            let min_x = points.iter().map(|point| point.0).fold(f64::MAX, f64::min);
            let max_x = points.iter().map(|point| point.0).fold(f64::MIN, f64::max);
            let min_y = points.iter().map(|point| point.1).fold(f64::MAX, f64::min);
            let max_y = points.iter().map(|point| point.1).fold(f64::MIN, f64::max);
            self.region.set([
                min_x - padding,
                min_y - padding,
                max_x - min_x + padding * 2.0,
                max_y - min_y + padding * 2.0,
            ]);
        }
        self.drag.replace(Drag {
            points: drag.points,
            ..Drag::default()
        });
        self.snip();
    }

    fn capture_whole_screen(self: &Rc<Self>) {
        self.region.set([0.0, 0.0, self.size.0, self.size.1]);
        self.snip();
    }

    fn snip(self: &Rc<Self>) {
        let [x, y, width, height] = self.region.get();
        if width <= 0.0 || height <= 0.0 {
            self.dismiss();
            return;
        }
        let (screen_width, screen_height) = self.size;
        let x = x.min(screen_width - width).max(0.0);
        let y = y.min(screen_height - height).max(0.0);
        let width = width.min(screen_width - x).max(0.0);
        let height = height.min(screen_height - y).max(0.0);
        let region = [x, y, width, height];
        self.region.set(region);
        self.remember(region);

        let options = &self.config.region;
        let save_dir = if options.save.get() {
            options.save_path.clone()
        } else {
            String::new()
        };
        let shadowed = self.mode.get() == Mode::Window;
        let crop = Crop {
            x: x * self.scale,
            y: y * self.scale,
            width: width * self.scale,
            height: height * self.scale,
            shadow: shadowed,
            radius: if shadowed {
                self.targeted
                    .get()
                    .and_then(|bounds| self.windows.iter().find(|window| window.bounds == bounds))
                    .map_or(REGION_RADIUS, |window| window.radius)
                    * self.scale
            } else {
                0.0
            },
        };
        let record_region = format!(
            "{},{} {}x{}",
            (self.offset.0 + x).round(),
            (self.offset.1 + y).round(),
            width.round(),
            height.round()
        );
        let recording = self.mode.get().recording();
        let command = screenshot::command(
            &crop,
            &self.path,
            self.action(),
            &save_dir,
            &record_region,
            &crate::core::process::executable(),
        );
        let script = screenshot::delayed(
            command,
            options.countdown.get(),
            &self.screen,
            (!recording).then_some(self.path.as_path()),
            options.show_pointer.get(),
        );
        self.snipped.set(true);
        crate::platform::desktop::shell(&script);
        if !recording {
            self.dismiss();
            return;
        }
        self.phase.set(Phase::Post);
        self.window.set_keyboard_mode(KeyboardMode::None);
        if let Some(surface) = self.window.surface() {
            surface.set_input_region(Some(&gtk4::cairo::Region::create()));
        }
        self.guide.widget.set_visible(false);
        self.controls.holder.set_visible(false);
        self.breathing.set(self.now());
        self.animate();
        if let Some(selector) = self.selector.upgrade() {
            selector.recording_started(&self.screen);
        }
    }

    fn dismiss(&self) {
        if let Some(selector) = self.selector.upgrade() {
            selector.dismiss();
        }
    }

    fn place_guide(&self) {
        let drag = self.drag.borrow();
        let (x, y) = if drag.dragging {
            let [left, top, width, height] = self.region.get();
            (left + width, top + height)
        } else {
            self.pointer.get()
        };
        drop(drag);
        self.layer.move_(
            &self.guide.widget,
            x + GUIDE_MARGIN as f64,
            y + GUIDE_MARGIN as f64,
        );
    }

    fn animate(self: &Rc<Self>) {
        self.paint.queue_draw();
        if self.ticking.replace(true) {
            return;
        }
        let selection = Rc::downgrade(self);
        self.paint.add_tick_callback(move |paint, clock| {
            let Some(selection) = selection.upgrade() else {
                return glib::ControlFlow::Break;
            };
            let now = clock.frame_time();
            paint.queue_draw();
            selection.apply_guide(now);
            if selection.running(now) {
                return glib::ControlFlow::Continue;
            }
            selection.ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    fn apply_guide(&self, now: i64) {
        self.controls
            .holder
            .set_offset(self.controls.rise.get().value(now) as f32);
        self.controls
            .holder
            .set_opacity(self.controls.appear.get().value(now));
        let width = self.guide.width.get().value(now).round() as i32;
        self.guide.widget.set_size_request(width, GUIDE_HEIGHT);
        self.guide.background.queue_draw();
        self.guide
            .description
            .set_opacity(self.guide.fade.get().value(now));
    }

    fn running(&self, now: i64) -> bool {
        self.phase.get() == Phase::Post
            || self.fade.get().running(now)
            || self.controls.rise.get().running(now)
            || self.controls.appear.get().running(now)
            || self.guide.width.get().running(now)
            || self.guide.fade.get().running(now)
            || self.fills.borrow().iter().any(|tween| tween.running(now))
    }

    fn colours(&self) -> Colours {
        let theme = self.theme.borrow();
        let colours = &theme.colors;
        let dark = theme.m3.darkmode;
        let bright_text = if dark {
            colours.col_on_layer0
        } else {
            colours.col_layer0
        };
        let bright_secondary = if dark {
            colours.col_secondary
        } else {
            colours.col_on_secondary
        };
        Colours {
            overlay: RGBA::new(0.0, 0.0, 0.0, OVERLAY_ALPHA),
            selection: mix(bright_text, bright_secondary, 0.5),
            window_border: bright_secondary,
            window_fill: transparentize(bright_secondary, 0.85),
            outline_variant: theme.m3.outline_variant,
        }
    }

    fn draw(&self, snapshot: &gtk4::Snapshot) {
        let now = self.now();
        let colours = self.colours();
        let (width, height) = self.size;
        let select = self.phase.get() == Phase::Select;
        if select && let Some(texture) = self.frozen.borrow().as_ref() {
            snapshot.append_scaled_texture(
                texture,
                gsk::ScalingFilter::Linear,
                &rect(0.0, 0.0, width, height),
            );
        }
        if self.mode.get().circle() {
            self.draw_circle(snapshot, &colours);
        } else {
            self.draw_rect(snapshot, &colours);
        }
        if !select {
            return;
        }
        let opacity = self.fade.get().value(now);
        if opacity <= 0.0 {
            return;
        }
        let mode = self.mode.get();
        let fills = self.fills.borrow();
        let mut index = 0;
        for (enabled, targets, icons) in [
            (
                self.config.region.target_windows && mode.picks_region(),
                &self.windows,
                true,
            ),
            (
                self.config.region.target_layers && mode.picks_region(),
                &self.layers,
                false,
            ),
        ] {
            for (position, target) in targets.iter().enumerate() {
                let fill = fills
                    .get(index)
                    .map(|tween| tween.value(now))
                    .unwrap_or(0.0);
                index += 1;
                if !enabled {
                    continue;
                }
                for part in uncovered(&target.bounds, &targets[..position]) {
                    snapshot.push_clip(&part);
                    self.draw_target(snapshot, &colours, target, fill, opacity, icons);
                    snapshot.pop();
                }
            }
        }
    }

    fn draw_rect(&self, snapshot: &gtk4::Snapshot, colours: &Colours) {
        let (width, height) = self.size;
        let [x, y, region_width, region_height] = self.region.get();
        let breathing = self.phase.get() == Phase::Post;
        if !breathing {
            for bounds in [
                rect(0.0, 0.0, width, y.max(0.0)),
                rect(0.0, y + region_height, width, height - y - region_height),
                rect(0.0, y, x.max(0.0), region_height),
                rect(x + region_width, y, width - x - region_width, region_height),
            ] {
                if bounds.width() > 0.0 && bounds.height() > 0.0 {
                    snapshot.append_color(&colours.overlay, &bounds);
                }
            }
            let text = format!("{} x {}", region_width.round(), region_height.round());
            let layout = self.paint.create_pango_layout(Some(&text));
            layout.set_font_description(Some(&text::font(
                Family::Main,
                pixel_size::SMALL as f64,
                "wght=450",
            )));
            let (_, logical) = layout.pixel_extents();
            let border_right = x.round() + region_width.round() + 1.0;
            let border_bottom = y.round() + region_height.round() + 1.0;
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                (border_right - LABEL_MARGIN - logical.width() as f64) as f32,
                (border_bottom + LABEL_MARGIN) as f32,
            ));
            snapshot.append_layout(&layout, &colours.selection);
            snapshot.restore();
            if self.config.region.aim_lines {
                let (pointer_x, pointer_y) = self.pointer.get();
                let aim = transparentize(colours.selection, 1.0 - AIM_OPACITY);
                snapshot.append_color(&aim, &rect(pointer_x, 0.0, 1.0, height));
                snapshot.append_color(&aim, &rect(0.0, pointer_y, width, 1.0));
            }
        }
        let opacity = if breathing {
            breath((self.now() - self.breathing.get()) as f64 / 1000.0)
        } else {
            BREATH_HIGH
        };
        let left = x.round() - 1.0;
        let top = y.round() - 1.0;
        let border_width = region_width.round() + 2.0;
        let border_height = region_height.round() + 2.0;
        let bounds = rect(left, top, border_width.max(1.0), border_height.max(1.0));
        let cr = snapshot.append_cairo(&bounds);
        cr.set_source_rgba(
            colours.selection.red() as f64,
            colours.selection.green() as f64,
            colours.selection.blue() as f64,
            colours.selection.alpha() as f64 * opacity,
        );
        cr.set_line_width(1.0);
        cr.set_dash(&[DASH, GAP], 0.0);
        cr.rectangle(
            left + 0.5,
            top + 0.5,
            border_width - 1.0,
            border_height - 1.0,
        );
        let _ = cr.stroke();
    }

    fn draw_circle(&self, snapshot: &gtk4::Snapshot, colours: &Colours) {
        let (width, height) = self.size;
        let bounds = rect(0.0, 0.0, width, height);
        snapshot.append_color(&colours.overlay, &bounds);
        let drag = self.drag.borrow();
        if drag.points.is_empty() {
            return;
        }
        let cr = snapshot.append_cairo(&bounds);
        cr.set_source_rgba(
            colours.selection.red() as f64,
            colours.selection.green() as f64,
            colours.selection.blue() as f64,
            colours.selection.alpha() as f64,
        );
        cr.set_line_width(self.config.region.circle_stroke);
        cr.set_line_cap(gtk4::cairo::LineCap::Round);
        cr.set_line_join(gtk4::cairo::LineJoin::Round);
        let (first_x, first_y) = drag.points[0];
        cr.move_to(first_x, first_y);
        for (x, y) in drag.points.iter().skip(1) {
            cr.line_to(*x, *y);
        }
        let _ = cr.stroke();
    }

    fn draw_target(
        &self,
        snapshot: &gtk4::Snapshot,
        colours: &Colours,
        target: &Target,
        fill: f64,
        opacity: f64,
        icon: bool,
    ) {
        let [x, y, width, height] = target.bounds;
        let bounds = rect(x, y, width, height);
        let outline = gsk::RoundedRect::from_rect(bounds, target.radius as f32);
        let targeted = self.targeted.get() == Some(target.bounds);
        snapshot.push_opacity(opacity);
        if fill > 0.0 {
            let colour = mix(
                colours.window_fill,
                RGBA::new(0.0, 0.0, 0.0, 0.0),
                fill as f32,
            );
            snapshot.push_rounded_clip(&outline);
            snapshot.append_color(&colour, &bounds);
            snapshot.pop();
        }
        let border = if targeted {
            REGION_BORDER_TARGETED
        } else {
            REGION_BORDER
        };
        snapshot.append_border(&outline, &[border; 4], &[colours.window_border; 4]);
        if self.config.region.show_label {
            self.draw_tag(snapshot, colours, target, icon);
        }
        snapshot.pop();
    }

    fn draw_tag(&self, snapshot: &gtk4::Snapshot, colours: &Colours, target: &Target, icon: bool) {
        let layout = self.paint.create_pango_layout(Some(&target.label));
        layout.set_font_description(Some(&text::font(
            Family::Main,
            pixel_size::SMALL as f64,
            "wght=450",
        )));
        let (_, logical) = layout.pixel_extents();
        let text_height = centred::layout_qt_metrics(&layout)
            .map(|(height, _)| height as f64)
            .unwrap_or(logical.height() as f64);
        let icon_size = pixel_size::LARGER as f64;
        let icon_width = if icon { icon_size + TAG_SPACING } else { 0.0 };
        let row_height = text_height.max(if icon { icon_size } else { 0.0 });
        let width = icon_width + logical.width() as f64 + TAG_PADDING.0 * 2.0;
        let height = row_height + TAG_PADDING.1 * 2.0;
        let left = target.bounds[0] + REGION_TEXT_PADDING;
        let top = target.bounds[1] + REGION_TEXT_PADDING;
        let bounds = rect(left, top, width, height);
        let outline = gsk::RoundedRect::from_rect(bounds, TAG_RADIUS as f32);
        snapshot.push_rounded_clip(&outline);
        snapshot.append_color(
            &RGBA::new(
                0x11 as f32 / 255.0,
                0x11 as f32 / 255.0,
                0x11 as f32 / 255.0,
                0.9,
            ),
            &bounds,
        );
        snapshot.pop();
        snapshot.append_border(&outline, &[1.0; 4], &[colours.outline_variant; 4]);
        let mut cursor = left + TAG_PADDING.0;
        if icon && let Some(display) = gdk::Display::default() {
            let theme = gtk4::IconTheme::for_display(&display);
            let name = crate::platform::appicon::guess(&theme, &target.label);
            let paintable = crate::platform::appicon::themed(
                &theme,
                &name,
                "image-missing",
                icon_size as i32,
                self.paint.scale_factor(),
            );
            snapshot.save();
            snapshot.translate(&graphene::Point::new(
                cursor as f32,
                (top + (height - icon_size) / 2.0) as f32,
            ));
            paintable.snapshot(snapshot, icon_size, icon_size);
            snapshot.restore();
            cursor += icon_width;
        }
        snapshot.save();
        snapshot.translate(&graphene::Point::new(
            cursor as f32,
            (top + (height - logical.height() as f64) / 2.0) as f32,
        ));
        snapshot.append_layout(&layout, &RGBA::new(1.0, 1.0, 1.0, 0xdd as f32 / 255.0));
        snapshot.restore();
    }
}

struct Colours {
    overlay: RGBA,
    selection: RGBA,
    window_border: RGBA,
    window_fill: RGBA,
    outline_variant: RGBA,
}

fn region_of(drag: &Drag) -> [f64; 4] {
    [
        drag.start.0.min(drag.at.0),
        drag.start.1.min(drag.at.1),
        (drag.at.0 - drag.start.0).abs(),
        (drag.at.1 - drag.start.1).abs(),
    ]
}

fn breath(seconds: f64) -> f64 {
    let half = BREATH_MILLIS / 1000.0;
    let phase = seconds % (half * 2.0);
    let (from, to, part) = if phase < half {
        (BREATH_HIGH, BREATH_LOW, phase / half)
    } else {
        (BREATH_LOW, BREATH_HIGH, (phase - half) / half)
    };
    let eased = if part < 0.5 {
        2.0 * part * part
    } else {
        1.0 - (-2.0 * part + 2.0).powi(2) / 2.0
    };
    from + (to - from) * eased
}

fn guide_symbol(action: Action) -> &'static str {
    match action {
        Action::Copy | Action::Edit => "content_cut",
        Action::Record | Action::RecordWithSound => "videocam",
    }
}

fn guide_description(action: Action) -> &'static str {
    match action {
        Action::Copy | Action::Edit => "Copy region (LMB) or annotate (RMB)",
        Action::Record | Action::RecordWithSound => "Record region",
    }
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> graphene::Rect {
    graphene::Rect::new(x as f32, y as f32, width as f32, height as f32)
}

fn uncovered(bounds: &[f64; 4], above: &[Target]) -> Vec<graphene::Rect> {
    let pixels = |[x, y, width, height]: [f64; 4]| {
        let (left, top) = (x.floor() as i32, y.floor() as i32);
        let (right, bottom) = ((x + width).ceil() as i32, (y + height).ceil() as i32);
        gtk4::cairo::RectangleInt::new(left, top, right - left, bottom - top)
    };
    let region = gtk4::cairo::Region::create_rectangle(&pixels(*bounds));
    for other in above {
        let _ = region.subtract_rectangle(&pixels(other.bounds));
    }
    (0..region.num_rectangles())
        .map(|index| {
            let part = region.rectangle(index);
            rect(
                part.x() as f64,
                part.y() as f64,
                part.width() as f64,
                part.height() as f64,
            )
        })
        .collect()
}

fn intersection_over_union(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    let inter = (right - left).max(0.0) * (bottom - top).max(0.0);
    let union = a[2] * a[3] + b[2] * b[3] - inter;
    if union > 0.0 { inter / union } else { 0.0 }
}

fn targets(name: &str, offset: (f64, f64)) -> (Vec<Target>, Vec<Target>) {
    let monitor = hypr::json("monitors")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(name));
    let active = monitor
        .as_ref()
        .and_then(|entry| entry.pointer("/activeWorkspace/id"))
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let bounds =
        |at: (f64, f64), size: (f64, f64)| [at.0 - offset.0, at.1 - offset.1, size.0, size.1];
    let number = |value: &Value, key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let layers: Vec<Target> = hypr::json("layers")
        .and_then(|value| value.get(name)?.pointer("/levels/2")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|layer| {
            let namespace = layer.get("namespace")?.as_str()?.to_owned();
            if [":bar", ":verticalBar", ":dock"]
                .iter()
                .any(|excluded| namespace.contains(excluded))
            {
                return None;
            }
            Some(Target {
                bounds: bounds(
                    (number(layer, "x"), number(layer, "y")),
                    (number(layer, "w"), number(layer, "h")),
                ),
                label: namespace,
                radius: REGION_RADIUS,
            })
        })
        .collect();
    let pair = |value: &Value, key: &str| {
        let list = value.get(key).and_then(Value::as_array);
        let at = |index: usize| {
            list.and_then(|list| list.get(index))
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
        };
        (at(0), at(1))
    };
    let mut clients: Vec<Value> = hypr::json("clients")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|client| {
            client.pointer("/workspace/id").and_then(Value::as_i64) == Some(active as i64)
        })
        .collect();
    clients.sort_by_key(|client| {
        let floating = client.get("floating").and_then(Value::as_bool) == Some(true);
        let focused = client
            .get("focusHistoryID")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX);
        (!floating, if floating { focused } else { 0 })
    });
    let rounding = hypr::option_int("decoration:rounding").unwrap_or(0) as f64;
    let window_rounding = |client: &Value| {
        if client
            .get("fullscreen")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= 2
        {
            return 0.0;
        }
        client
            .get("address")
            .and_then(Value::as_str)
            .and_then(|address| hypr::request(&format!("getprop address:{address} rounding")))
            .and_then(|reply| reply.trim().parse::<f64>().ok())
            .unwrap_or(rounding)
    };
    let windows = clients
        .iter()
        .map(|client| Target {
            bounds: bounds(pair(client, "at"), pair(client, "size")),
            label: client
                .get("class")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            radius: window_rounding(client),
        })
        .filter(|window| {
            layers
                .iter()
                .all(|layer| intersection_over_union(&window.bounds, &layer.bounds) <= 0.0)
        })
        .collect();
    (windows, layers)
}

pub struct RegionSelector {
    app: gtk4::Application,
    theme: SharedTheme,
    recording: Recording,
    mode: Cell<Mode>,
    recording_screen: RefCell<String>,
    selections: RefCell<Vec<Rc<Selection>>>,
}

impl RegionSelector {
    pub fn new(app: &gtk4::Application, theme: &SharedTheme, recording: &Recording) -> Rc<Self> {
        let selector = Rc::new(RegionSelector {
            app: app.clone(),
            theme: theme.clone(),
            recording: recording.clone(),
            mode: Cell::new(Mode::Region),
            recording_screen: RefCell::new(String::new()),
            selections: RefCell::new(Vec::new()),
        });
        recording
            .subscribe({
                let selector = Rc::downgrade(&selector);
                move || {
                    let Some(selector) = selector.upgrade() else {
                        return;
                    };
                    if !selector.recording.active()
                        && !selector.recording_screen.borrow().is_empty()
                    {
                        selector.dismiss();
                    }
                }
            })
            .forever();
        selector
    }

    fn config(&self) -> Rc<Config> {
        config::current()
    }

    pub fn screenshot(self: &Rc<Self>) {
        self.mode.set(Mode::Region);
        self.reopen();
    }

    pub fn record(self: &Rc<Self>) {
        self.config().region.set_record_sound(false);
        self.mode.set(Mode::RecordRegion);
        self.reopen();
    }

    pub fn record_with_sound(self: &Rc<Self>) {
        self.config().region.set_record_sound(true);
        self.mode.set(Mode::RecordRegion);
        self.reopen();
    }

    pub fn stop_recording(&self) {
        self.recording.stop();
    }

    fn reopen(self: &Rc<Self>) {
        self.recording_screen.replace(String::new());
        self.close_all();
        let Some(display) = gdk::Display::default() else {
            return;
        };
        let monitors: Vec<gdk::Monitor> = display
            .monitors()
            .iter::<gdk::Monitor>()
            .flatten()
            .collect();
        for monitor in monitors {
            let selection = self.build(&monitor);
            self.selections.borrow_mut().push(selection.clone());
            self.prepare(&selection);
        }
    }

    pub fn dismiss(&self) {
        self.recording_screen.replace(String::new());
        self.close_all();
    }

    fn close_all(&self) {
        let selections: Vec<Rc<Selection>> = self.selections.borrow_mut().drain(..).collect();
        for selection in selections {
            close(&selection);
        }
    }

    fn recording_started(&self, screen: &str) {
        self.recording_screen.replace(screen.to_owned());
        let others: Vec<Rc<Selection>> = {
            let mut selections = self.selections.borrow_mut();
            let (kept, others) = selections
                .drain(..)
                .partition(|selection| selection.screen == screen);
            *selections = kept;
            others
        };
        for selection in others {
            close(&selection);
        }
        self.recording.watch();
    }

    fn prepare(self: &Rc<Self>, selection: &Rc<Selection>) {
        let recording = selection.mode.get().recording();
        let pointer = selection.config.region.show_pointer.get();
        let hiding = pointer && hypr::option_bool("cursor:hide_on_key_press") == Some(true);
        if hiding {
            hypr::set_cursor_hides_on_key(false);
        }
        if pointer {
            hypr::show_cursor();
        }
        let screenshot = screenshot::temp_command(&selection.screen, &selection.path, pointer);
        let selector = Rc::downgrade(self);
        let selection = Rc::downgrade(selection);
        glib::spawn_future_local(async move {
            let running = if recording {
                run(&["pidof", "wf-recorder"]).await
            } else {
                false
            };
            if running {
                if hiding {
                    hypr::set_cursor_hides_on_key(true);
                }
                crate::core::process::launch_subcommand(&["record"]);
                if let Some(selector) = selector.upgrade() {
                    selector.dismiss();
                }
                return;
            }
            let _ = run(&["bash", "-c", &screenshot]).await;
            if hiding {
                hypr::set_cursor_hides_on_key(true);
            }
            let Some(path) = selection.upgrade().map(|selection| selection.path.clone()) else {
                return;
            };
            let texture = crate::ui::image::texture(path, (-1, -1)).await;
            let Some(selection) = selection.upgrade() else {
                return;
            };
            selection.frozen.replace(texture);
            selection.show_mode_region();
            selection.window.set_visible(true);
            selection.show_description();
            selection.rise();
        });
    }

    fn build(self: &Rc<Self>, monitor: &gdk::Monitor) -> Rc<Selection> {
        let screen: String = monitor.connector().map(Into::into).unwrap_or_default();
        let geometry = monitor.geometry();
        let scale = monitor.scale();
        let offset = hypr::json("monitors")
            .and_then(|value| value.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .find(|entry| entry.get("name").and_then(Value::as_str) == Some(screen.as_str()))
            .map(|entry| {
                (
                    entry.get("x").and_then(Value::as_f64).unwrap_or(0.0),
                    entry.get("y").and_then(Value::as_f64).unwrap_or(0.0),
                )
            })
            .unwrap_or((geometry.x() as f64, geometry.y() as f64));
        let (windows, layers) = targets(&screen, offset);

        let paint = Paint::new(|_, _, _| {});
        paint.set_hexpand(true);
        paint.set_vexpand(true);
        paint.set_cursor_from_name(Some("crosshair"));

        let symbol = text::symbol(guide_symbol(Action::Copy), GUIDE_ICON);
        text::set_color(&symbol, "colOnPrimary");
        let (symbol_holder, set_symbol) = text::animate_change(&symbol);
        let description = text::styled(guide_description(Action::Copy));
        text::set_color(&description, "colOnPrimary");
        description.set_margin_start(GUIDE_SPACING - GUIDE_TEXT_INSET);
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        row.set_halign(gtk4::Align::Start);
        row.set_valign(gtk4::Align::Center);
        row.set_margin_start(GUIDE_PADDING);
        row.append(&Centred::new(&symbol_holder));
        row.append(&Centred::new(&description));

        let background = Paint::new({
            let theme = self.theme.clone();
            move |snapshot, width, height| {
                let colour = theme.borrow().colors.col_primary;
                let bounds = graphene::Rect::new(0.0, 0.0, width, height);
                let round = (height / 2.0).min(width / 2.0);
                let size = |radius: f32| graphene::Size::new(radius, radius);
                let corner = (GUIDE_CORNER as f32).min(round);
                let outline = gsk::RoundedRect::new(
                    bounds,
                    size(corner),
                    size(round),
                    size(round),
                    size(round),
                );
                snapshot.push_rounded_clip(&outline);
                snapshot.append_color(&colour, &bounds);
                snapshot.pop();
            }
        });
        let guide_widget = gtk4::Overlay::new();
        guide_widget.set_child(Some(&background));
        guide_widget.add_overlay(&row);
        guide_widget.set_overflow(gtk4::Overflow::Hidden);
        guide_widget.set_can_target(false);

        let layer = gtk4::Fixed::new();
        layer.set_can_target(false);
        layer.put(&guide_widget, 0.0, 0.0);

        let tabs = TabBar::new(&self.theme, &MODES.map(|(_, icon, name)| (icon, name)));
        let tune = text::symbol("tune", TOOLBAR_ICON);
        text::set_color(&tune, "m3onBackground");
        let options_label = text::styled("Options");
        let arrow = text::symbol("keyboard_arrow_up", ARROW_ICON);
        text::set_color(&arrow, "m3onBackground");
        let options_row = gtk4::Box::new(gtk4::Orientation::Horizontal, OPTIONS_SPACING);
        options_row.append(&Centred::integral(&tune));
        options_row.append(&Centred::new(&options_label));
        options_row.append(&Centred::integral(&arrow));
        let options = toolbar::button(&self.theme, &options_row);
        let bar = toolbar::frame();
        bar.append(&tabs.widget);
        bar.append(&toolbar::separator());
        bar.append(&options);
        let (capture, capture_symbol, capture_tip) =
            toolbar::paired_fab(&self.theme, "photo_camera", "Capture");
        let (close_button, _, _) = toolbar::paired_fab(&self.theme, "close", "Close");
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, CONTROLS_SPACING);
        row.append(&bar);
        row.append(&capture);
        row.append(&close_button);
        let config = self.config();
        let (menu, switches, countdown) = options_menu(&self.theme, &config);
        menu.set_visible(false);
        let stack = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        stack.append(&menu);
        stack.append(&row);
        let holder = text::Shift::new(&stack);
        holder.set_halign(gtk4::Align::Center);
        holder.set_valign(gtk4::Align::End);
        holder.set_margin_bottom(CONTROLS_MARGIN);
        holder.set_opacity(0.0);

        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&paint));
        overlay.add_overlay(&layer);
        overlay.add_overlay(&holder);

        let window = gtk4::ApplicationWindow::builder()
            .application(&self.app)
            .child(&overlay)
            .build();
        window.add_css_class("region-selector");
        window.init_layer_shell();
        window.set_namespace(Some(NAMESPACE));
        window.set_monitor(Some(monitor));
        window.set_layer(Layer::Overlay);
        for edge in [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right] {
            window.set_anchor(edge, true);
        }
        window.set_exclusive_zone(-1);
        window.set_keyboard_mode(KeyboardMode::OnDemand);

        let fill_count = windows.len() + layers.len();
        let opacity = config.region.target_opacity;
        let selection = Rc::new(Selection {
            selector: Rc::downgrade(self),
            controls: Controls {
                holder,
                tabs,
                capture,
                capture_symbol,
                capture_tip,
                options,
                arrow,
                menu,
                open: Cell::new(false),
                rise: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                appear: Cell::new(Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
                _switches: switches,
                _countdown: countdown,
            },
            config,
            theme: self.theme.clone(),
            window: window.clone(),
            paint: paint.clone(),
            layer,
            guide: Guide {
                widget: guide_widget,
                background,
                symbol: set_symbol,
                description,
                width: Cell::new(Tween::new(0.0, MOVE_MILLIS, EXPRESSIVE_DEFAULT)),
                fade: Cell::new(Tween::new(1.0, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
                shown: Cell::new(true),
                timeout: RefCell::new(None),
            },
            path: screenshot::temp_path(&screen),
            screen,
            size: (geometry.width() as f64, geometry.height() as f64),
            scale: scale as f64,
            offset,
            mode: Cell::new(self.mode.get()),
            phase: Cell::new(Phase::Select),
            frozen: RefCell::new(None),
            drag: RefCell::new(Drag::default()),
            region: Cell::new([0.0; 4]),
            targeted: Cell::new(None),
            windows,
            layers,
            fills: RefCell::new(vec![
                Tween::new(0.0, FAST_MILLIS, EXPRESSIVE_EFFECTS);
                fill_count
            ]),
            fade: Cell::new(Tween::new(opacity, FAST_MILLIS, EXPRESSIVE_EFFECTS)),
            button: Cell::new(0),
            pointer: Cell::new((0.0, 0.0)),
            action: Cell::new(Action::Copy),
            breathing: Cell::new(0),
            ticking: Cell::new(false),
            snipped: Cell::new(false),
        });
        selection
            .controls
            .tabs
            .set_current(mode_index(selection.mode.get()));
        selection.sync_capture();
        selection.controls.tabs.connect_selected({
            let selection = Rc::downgrade(&selection);
            move |index| {
                if let Some(selection) = selection.upgrade() {
                    selection.set_mode(MODES[index].0);
                }
            }
        });
        selection.controls.options.connect_clicked({
            let selection = Rc::downgrade(&selection);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.toggle_options();
                }
            }
        });
        selection.controls.capture.connect_clicked({
            let selection = Rc::downgrade(&selection);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.capture_whole_screen();
                }
            }
        });
        close_button.connect_clicked({
            let selection = Rc::downgrade(&selection);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.dismiss();
                }
            }
        });
        let action = selection.action();
        selection.action.set(action);
        (selection.guide.symbol)(guide_symbol(action));
        selection
            .guide
            .description
            .set_text(guide_description(action));
        let mut width = selection.guide.width.get();
        width.jump(selection.guide_width(true));
        selection.guide.width.set(width);
        selection.apply_guide(0);

        paint.set_draw({
            let selection = Rc::downgrade(&selection);
            move |snapshot, _, _| {
                if let Some(selection) = selection.upgrade() {
                    selection.draw(snapshot);
                }
            }
        });

        let motion = gtk4::EventControllerMotion::new();
        motion.connect_motion({
            let selection = Rc::downgrade(&selection);
            move |_, x, y| {
                if let Some(selection) = selection.upgrade()
                    && selection.phase.get() == Phase::Select
                {
                    selection.motion(x, y);
                }
            }
        });
        paint.add_controller(motion);

        let click = gtk4::GestureDrag::new();
        click.set_button(0);
        click.connect_drag_begin({
            let selection = Rc::downgrade(&selection);
            move |gesture, x, y| {
                let button = gesture.current_button();
                if button != gdk::BUTTON_PRIMARY && button != gdk::BUTTON_SECONDARY {
                    return;
                }
                if let Some(selection) = selection.upgrade()
                    && selection.phase.get() == Phase::Select
                {
                    selection.pointer.set((x, y));
                    selection.press(button, x, y);
                }
            }
        });
        click.connect_drag_end({
            let selection = Rc::downgrade(&selection);
            move |gesture, _, _| {
                let button = gesture.current_button();
                if button != gdk::BUTTON_PRIMARY && button != gdk::BUTTON_SECONDARY {
                    return;
                }
                if let Some(selection) = selection.upgrade()
                    && selection.phase.get() == Phase::Select
                {
                    selection.release();
                }
            }
        });
        paint.add_controller(click);

        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let selection = Rc::downgrade(&selection);
            move |_, key, _, _| {
                let Some(selection) = selection.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                match key {
                    gdk::Key::Escape => selection.dismiss(),
                    gdk::Key::space => {
                        let next = if selection.mode.get() == Mode::Window {
                            Mode::Region
                        } else {
                            Mode::Window
                        };
                        selection.set_mode(next);
                    }
                    _ => return glib::Propagation::Proceed,
                }
                glib::Propagation::Stop
            }
        });
        window.add_controller(keys);

        selection
    }
}

fn options_menu(
    theme: &SharedTheme,
    config: &Rc<Config>,
) -> (gtk4::Box, Vec<Rc<ConfigSwitch>>, Rc<dyn Fn(bool)>) {
    let menu = gtk4::Box::new(gtk4::Orientation::Vertical, MENU_SPACING);
    menu.add_css_class("region-options");
    menu.set_size_request(MENU_WIDTH, -1);
    menu.set_halign(gtk4::Align::End);
    menu.set_margin_bottom(CONTROLS_MARGIN);

    let label = text::styled("Wait before capturing");
    text::set_color(&label, "colSubtext");
    label.set_xalign(0.0);
    let heading = Centred::filling_width(&label);
    heading.set_margin_start(LABEL_INSET);
    menu.append(&heading);

    let choices = gtk4::Box::new(gtk4::Orientation::Horizontal, MENU_SPACING);
    let buttons: Rc<Vec<(GroupButton, gtk4::Label, i64)>> = Rc::new(
        COUNTDOWNS
            .iter()
            .map(|(value, name)| {
                let label = text::styled(name);
                let content = Centred::integral(&label);
                let width =
                    content.measure(gtk4::Orientation::Horizontal, -1).1 + CHOICE_PADDING.0 * 2;
                let height =
                    content.measure(gtk4::Orientation::Vertical, -1).1 + CHOICE_PADDING.1 * 2;
                let button = GroupButton::new(theme, width as f64, height as f64);
                button.set_bounce(false);
                button.set_look(GroupLook {
                    background: |theme| theme.colors.col_secondary_container,
                    hover: |theme| theme.colors.col_secondary_container_hover,
                    active: |theme| theme.colors.col_secondary_container_active,
                    ..GroupLook::default()
                });
                button.set_content(&content);
                choices.append(&button);
                (button, label, *value)
            })
            .collect(),
    );
    let refresh: Rc<dyn Fn(bool)> = Rc::new({
        let buttons = buttons.clone();
        let config = config.clone();
        move |animate: bool| {
            let current = config.region.countdown.get();
            let last = buttons.len() - 1;
            for (index, (button, label, value)) in buttons.iter().enumerate() {
                let toggled = *value == current;
                button.set_toggled(toggled);
                text::set_color(
                    label,
                    if toggled {
                        "colOnPrimary"
                    } else {
                        "colOnSecondaryContainer"
                    },
                );
                let full = button.height() as f64 / 2.0;
                let full = if full > 0.0 {
                    full
                } else {
                    label.measure(gtk4::Orientation::Vertical, -1).1 as f64 / 2.0
                        + CHOICE_PADDING.1 as f64
                };
                let side = |edge: bool| if toggled || edge { full } else { CHOICE_SQUARE };
                button.set_side_radii(side(index == 0), side(index == last));
                if !animate {
                    button.jump_radius();
                }
            }
        }
    });
    for (button, _, value) in buttons.iter() {
        button.connect_clicked({
            let config = config.clone();
            let refresh = Rc::downgrade(&refresh);
            let value = *value;
            move || {
                config.region.set_countdown(value);
                if let Some(refresh) = refresh.upgrade() {
                    refresh(true);
                }
            }
        });
    }
    refresh(false);
    menu.append(&choices);

    let options = &config.region;
    let save_tip = if options.save_path.is_empty() {
        "No folder is set yet — pick one in Settings".to_owned()
    } else {
        options.save_path.clone()
    };
    let rows: [(&str, &str, bool, fn(&Config, bool)); 4] = [
        (
            "save",
            "Also save to a file",
            options.save.get(),
            |config, on| config.region.set_save(on),
        ),
        (
            "mic",
            "Record the microphone",
            options.record_sound.get(),
            |config, on| config.region.set_record_sound(on),
        ),
        (
            "arrow_selector_tool",
            "Include the pointer",
            options.show_pointer.get(),
            |config, on| config.region.set_show_pointer(on),
        ),
        (
            "history",
            "Start from the last region",
            options.remember_region.get(),
            |config, on| config.region.set_remember_region(on),
        ),
    ];
    let mut switches = Vec::new();
    for (index, (icon, name, checked, apply)) in rows.into_iter().enumerate() {
        let slot: Rc<RefCell<Weak<ConfigSwitch>>> = Rc::new(RefCell::new(Weak::new()));
        let switch = ConfigSwitch::new(theme, icon, name, {
            let config = config.clone();
            let slot = slot.clone();
            move |on| {
                apply(&config, on);
                if let Some(switch) = slot.borrow().upgrade() {
                    switch.set(on);
                }
            }
        });
        slot.replace(Rc::downgrade(&switch));
        switch.set(checked);
        if index == 0 {
            let tip = Tooltip::new(&switch.button, theme, tooltip::Kind::Styled);
            tip.set_text(&save_tip);
            tooltip::hover_delay(&switch.button, &tip, 0);
        }
        menu.append(&switch.button);
        switches.push(switch);
    }
    (menu, switches, refresh)
}

fn close(selection: &Rc<Selection>) {
    if let Some(source) = selection.guide.timeout.borrow_mut().take() {
        source.remove();
    }
    if !selection.snipped.get() {
        let _ = std::fs::remove_file(&selection.path);
    }
    selection.frozen.replace(None);
    selection.window.destroy();
    crate::ui::unload::trim();
}

async fn run(command: &[&str]) -> bool {
    process::finish(process::quiet(command)).await == Some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_outline_keeps_only_what_the_windows_above_leave_visible() {
        let above = |bounds: [f64; 4]| Target {
            bounds,
            label: String::new(),
            radius: 0.0,
        };
        let area = |parts: Vec<graphene::Rect>| {
            parts
                .iter()
                .map(|part| part.width() * part.height())
                .sum::<f32>()
        };
        let window = [0.0, 0.0, 100.0, 100.0];
        assert_eq!(area(uncovered(&window, &[])), 10000.0);
        assert_eq!(
            area(uncovered(&window, &[above([50.0, 50.0, 80.0, 80.0])])),
            7500.0
        );
        assert!(uncovered(&window, &[above([-10.0, -10.0, 200.0, 200.0])]).is_empty());
    }
}
