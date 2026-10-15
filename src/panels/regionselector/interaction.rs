use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4_layer_shell::{KeyboardMode, LayerShell};
use serde_json::Value;
use std::rc::Rc;

use super::screenshot::{self, Action, Crop};
use super::targets::Target;
use super::{
    CONTROLS_MARGIN, DESCRIPTION_TIMEOUT, Drag, GUIDE_HEIGHT, GUIDE_MARGIN, GUIDE_PADDING,
    GUIDE_TEXT_INSET, Mode, Phase, REGION_RADIUS, Selection, mode_index,
};
use crate::core::i18n::tr;
use crate::core::persistent;

impl Selection {
    pub(super) fn now(&self) -> i64 {
        self.paint
            .frame_clock()
            .map(|clock| clock.frame_time())
            .unwrap_or_else(glib::monotonic_time)
    }

    pub(super) fn action(&self) -> Action {
        if self.mode.get().is_recording() {
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

    fn update_action(self: &Rc<Self>) {
        let action = self.action();
        if self.action.replace(action) == action {
            return;
        }
        (self.guide.set_symbol)(guide_symbol(action));
        self.guide
            .description_label
            .set_text(&tr(guide_description(action)));
        self.show_description();
    }

    pub(super) fn show_description(self: &Rc<Self>) {
        self.set_shows_description(true);
        if let Some(source) = self.guide.hide_timer.borrow_mut().take() {
            source.remove();
        }
        let selection = Rc::downgrade(self);
        let source = glib::timeout_add_local_once(DESCRIPTION_TIMEOUT, move || {
            if let Some(selection) = selection.upgrade() {
                selection.guide.hide_timer.borrow_mut().take();
                selection.set_shows_description(false);
            }
        });
        self.guide.hide_timer.replace(Some(source));
    }

    fn set_shows_description(self: &Rc<Self>, shows_description: bool) {
        let now = self.now();
        let mut width_tween = self.guide.width_tween.get();
        width_tween.retarget(self.guide_width(shows_description), now);
        self.guide.width_tween.set(width_tween);
        let mut fade = self.guide.fade.get();
        fade.retarget(if shows_description { 1.0 } else { 0.0 }, now);
        self.guide.fade.set(fade);
        self.animate();
    }

    pub(super) fn guide_width(&self, shows_description: bool) -> f64 {
        if !shows_description {
            return GUIDE_HEIGHT as f64;
        }
        let row = self.guide.widget.last_child();
        let natural_width = row
            .map(|row| row.measure(gtk4::Orientation::Horizontal, -1).1)
            .unwrap_or(0);
        (natural_width + GUIDE_PADDING + GUIDE_TEXT_INSET) as f64
    }

    pub(super) fn set_mode(self: &Rc<Self>, mode: Mode) {
        self.mode.set(mode);
        self.controls.tabs.set_current(mode_index(mode));
        self.show_mode_region();
        self.update_action();
        self.sync_capture();
        self.animate();
    }

    pub(super) fn sync_capture(&self) {
        let mode = self.mode.get();
        let controls = &self.controls;
        controls.capture_button.set_visible(mode.is_whole_screen());
        let (icon, tip) = if mode.is_recording() {
            ("screen_record", "Record")
        } else {
            ("photo_camera", "Capture")
        };
        controls.capture_symbol.set_text(icon);
        controls.capture_tip.set_text(&tr(tip));
    }

    pub(super) fn toggle_options(&self) {
        let controls = &self.controls;
        let is_open = !controls.is_open.get();
        controls.is_open.set(is_open);
        controls.options_button.set_toggled(is_open);
        controls.menu.set_visible(is_open);
        controls.arrow_symbol.set_text(if is_open {
            "keyboard_arrow_down"
        } else {
            "keyboard_arrow_up"
        });
    }

    pub(super) fn rise(self: &Rc<Self>) {
        let now = self.now();
        let height = self
            .controls
            .holder
            .measure(gtk4::Orientation::Vertical, -1)
            .1;
        let mut rise_tween = self.controls.rise_tween.get();
        rise_tween.jump((height + CONTROLS_MARGIN) as f64);
        rise_tween.retarget(0.0, now);
        self.controls.rise_tween.set(rise_tween);
        let mut appear_tween = self.controls.appear_tween.get();
        appear_tween.retarget(1.0, now);
        self.controls.appear_tween.set(appear_tween);
        self.animate();
    }

    pub(super) fn show_mode_region(&self) {
        let mut drag = self.drag.borrow_mut();
        if self.mode.get().is_whole_screen() {
            drag.start = (0.0, 0.0);
            drag.end = self.size;
        } else {
            drag.start = (0.0, 0.0);
            drag.end = (0.0, 0.0);
            if let Some((start, end)) = self.saved_corners() {
                drag.start = start;
                drag.end = end;
            }
        }
        let region = region_of(&drag);
        drop(drag);
        self.region.set(region);
    }

    fn saved_corners(&self) -> Option<((f64, f64), (f64, f64))> {
        if !self.config.region.remember_region.get() {
            return None;
        }
        let entry = persistent::read(&["regionSelector"])?;
        let number = |key: &str| entry.get(key).and_then(Value::as_f64).unwrap_or(0.0);
        if entry.get("screen").and_then(Value::as_str) != Some(self.screen.as_str())
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

    fn update_target_bounds(&self, x: f64, y: f64) {
        let is_hit = |target: &&Target| {
            let [left, top, width, height] = target.bounds;
            left <= x && x <= left + width && top <= y && y <= top + height
        };
        let target_bounds = self
            .layers
            .iter()
            .find(is_hit)
            .or_else(|| self.windows.iter().find(is_hit))
            .map(|target| target.bounds);
        if self.target_bounds.replace(target_bounds) == target_bounds {
            return;
        }
        let now = self.now();
        let mut fills = self.fills.borrow_mut();
        for (tween, target) in fills
            .iter_mut()
            .zip(self.windows.iter().chain(self.layers.iter()))
        {
            let is_targeted = target_bounds == Some(target.bounds);
            tween.retarget(if is_targeted { 1.0 } else { 0.0 }, now);
        }
    }

    pub(super) fn press(self: &Rc<Self>, button: u32, x: f64, y: f64) {
        self.button.set(button);
        self.update_action();
        if self.mode.get().is_whole_screen() {
            return;
        }
        let mut drag = self.drag.borrow_mut();
        drag.start = (x, y);
        drag.end = (x, y);
        drag.is_dragging = true;
        let region = region_of(&drag);
        drop(drag);
        self.region.set(region);
        self.animate();
    }

    pub(super) fn handle_motion(self: &Rc<Self>, x: f64, y: f64) {
        self.pointer.set((x, y));
        self.place_guide();
        if self.mode.get().is_whole_screen() {
            self.animate();
            return;
        }
        self.update_target_bounds(x, y);
        let mut drag = self.drag.borrow_mut();
        if drag.is_dragging {
            let was_away = drag.delta != (0.0, 0.0);
            drag.end = (x, y);
            drag.delta = (x - drag.start.0, y - drag.start.1);
            drag.points.push((x, y));
            let region = region_of(&drag);
            let is_away = drag.delta != (0.0, 0.0);
            drop(drag);
            self.region.set(region);
            self.place_guide();
            if is_away && !was_away {
                let mut fade = self.fade.get();
                fade.retarget(0.0, self.now());
                self.fade.set(fade);
            }
        } else {
            drop(drag);
        }
        self.animate();
    }

    pub(super) fn release(self: &Rc<Self>) {
        if self.mode.get().is_whole_screen() {
            self.capture_whole_screen();
            return;
        }
        let drag = std::mem::take(&mut *self.drag.borrow_mut());
        if drag.points.iter().all(|point| *point == drag.start) {
            if let Some([x, y, width, height]) = self.target_bounds.get() {
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
        } else if self.mode.get().is_circle() {
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

    pub(super) fn capture_whole_screen(self: &Rc<Self>) {
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
        let has_shadow = self.mode.get() == Mode::Window;
        let crop = Crop {
            x: x * self.scale,
            y: y * self.scale,
            width: width * self.scale,
            height: height * self.scale,
            has_shadow,
            radius: if has_shadow {
                self.target_bounds
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
        let is_recording = self.mode.get().is_recording();
        let command = screenshot::command(
            &crop,
            &self.path,
            self.action(),
            &save_dir,
            &record_region,
            &crate::core::process::executable(),
        );
        let script = screenshot::delayed_command(
            command,
            options.countdown.get(),
            &self.screen,
            (!is_recording).then_some(self.path.as_path()),
            options.show_pointer.get(),
        );
        self.has_snipped.set(true);
        crate::platform::system::desktop::run_shell(&script);
        if !is_recording {
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
        self.breath_start.set(self.now());
        self.animate();
        if let Some(selector) = self.selector.upgrade() {
            selector.handle_recording_start(&self.screen);
        }
    }

    pub(super) fn dismiss(&self) {
        if let Some(selector) = self.selector.upgrade() {
            selector.dismiss();
        }
    }

    fn place_guide(&self) {
        let drag = self.drag.borrow();
        let (x, y) = if drag.is_dragging {
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
        if self.is_ticking.replace(true) {
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
            if selection.is_running(now) {
                return glib::ControlFlow::Continue;
            }
            selection.is_ticking.set(false);
            glib::ControlFlow::Break
        });
    }

    pub(super) fn apply_guide(&self, now: i64) {
        self.controls
            .holder
            .set_offset(self.controls.rise_tween.get().value(now) as f32);
        self.controls
            .holder
            .set_opacity(self.controls.appear_tween.get().value(now));
        let width = self.guide.width_tween.get().value(now).round() as i32;
        self.guide.widget.set_size_request(width, GUIDE_HEIGHT);
        self.guide.background.queue_draw();
        self.guide
            .description_label
            .set_opacity(self.guide.fade.get().value(now));
    }

    fn is_running(&self, now: i64) -> bool {
        self.phase.get() == Phase::Post
            || self.fade.get().is_running(now)
            || self.controls.rise_tween.get().is_running(now)
            || self.controls.appear_tween.get().is_running(now)
            || self.guide.width_tween.get().is_running(now)
            || self.guide.fade.get().is_running(now)
            || self
                .fills
                .borrow()
                .iter()
                .any(|tween| tween.is_running(now))
    }
}

fn region_of(drag: &Drag) -> [f64; 4] {
    [
        drag.start.0.min(drag.end.0),
        drag.start.1.min(drag.end.1),
        (drag.end.0 - drag.start.0).abs(),
        (drag.end.1 - drag.start.1).abs(),
    ]
}

pub(super) fn guide_symbol(action: Action) -> &'static str {
    match action {
        Action::Copy | Action::Edit => "content_cut",
        Action::Record | Action::RecordWithSound => "videocam",
    }
}

pub(super) fn guide_description(action: Action) -> &'static str {
    match action {
        Action::Copy | Action::Edit => "Copy region (LMB) or annotate (RMB)",
        Action::Record | Action::RecordWithSound => "Record region",
    }
}
