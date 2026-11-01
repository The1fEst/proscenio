use gtk4::gdk;
use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::ppm;
use super::screenshot::{self, Crop};
use super::stitcher::{Frame, Outcome, Stitcher};
use super::{Phase, Selection};
use crate::core::i18n::{tr, trf};
use crate::core::process;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::toolbar;

const IDLE_INTERVAL: Duration = Duration::from_millis(120);
const MIN_REGION: f64 = 64.0;
const OUTLINE_INSET: f64 = 1.0;
const PANEL_MARGIN: f64 = 16.0;
const PANEL_PADDING: f64 = 8.0;
const PANEL_SPACING: i32 = 8;
const PANEL_MIN_WIDTH: f64 = 150.0;
const PANEL_MAX_WIDTH: f64 = 200.0;
const PANEL_CHROME_HEIGHT: f64 = 130.0;
const PREVIEW_MAX_HEIGHT: f64 = 420.0;
const STATUS_CHARS: i32 = 20;

struct Panel {
    widget: gtk4::Box,
    picture: gtk4::Picture,
    status: gtk4::Label,
    done_button: RippleButton,
    cancel_button: RippleButton,
}

pub(super) struct Scroll {
    panel: Panel,
    stitcher: RefCell<Option<Stitcher>>,
    is_finishing: Cell<bool>,
    preview_width: Cell<f64>,
}

struct Step {
    outcome: Outcome,
    preview: Option<Frame>,
}

impl Scroll {
    pub(super) fn new(theme: &SharedTheme) -> Self {
        let picture = gtk4::Picture::new();
        picture.set_content_fit(gtk4::ContentFit::Fill);
        picture.set_can_shrink(true);
        picture.set_halign(gtk4::Align::Center);
        picture.set_visible(false);
        let status = text::styled(&tr("Scroll the content"));
        status.set_wrap(true);
        status.set_max_width_chars(STATUS_CHARS);
        status.set_justify(gtk4::Justification::Center);
        let cancel_button = build_button(theme, &tr("Cancel"));
        let done_button = build_button(theme, &tr("Done"));
        let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, PANEL_SPACING);
        buttons.set_halign(gtk4::Align::Center);
        buttons.append(&cancel_button);
        buttons.append(&done_button);
        let content = gtk4::Box::new(gtk4::Orientation::Vertical, PANEL_SPACING);
        content.append(&picture);
        content.append(&status);
        content.append(&buttons);
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        widget.add_css_class("region-options");
        widget.append(&content);
        widget.set_halign(gtk4::Align::Start);
        widget.set_valign(gtk4::Align::Start);
        widget.set_visible(false);
        Scroll {
            panel: Panel {
                widget,
                picture,
                status,
                done_button,
                cancel_button,
            },
            stitcher: RefCell::new(None),
            is_finishing: Cell::new(false),
            preview_width: Cell::new(0.0),
        }
    }

    pub(super) fn widget(&self) -> &gtk4::Box {
        &self.panel.widget
    }

    pub(super) fn end(&self) {
        self.stitcher.take();
    }
}

fn build_button(theme: &SharedTheme, text: &str) -> RippleButton {
    let label = text::styled(text);
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    row.append(&Centered::new(&label));
    toolbar::button(theme, &row)
}

impl Selection {
    pub(super) fn connect_scroll_buttons(self: &Rc<Self>) {
        let panel = &self.scroll.panel;
        panel.done_button.connect_clicked({
            let selection = Rc::downgrade(self);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.finish_scroll();
                }
            }
        });
        panel.cancel_button.connect_clicked({
            let selection = Rc::downgrade(self);
            move |_| {
                if let Some(selection) = selection.upgrade() {
                    selection.dismiss();
                }
            }
        });
    }

    pub(super) fn begin_scroll(self: &Rc<Self>) {
        let [x, y, width, height] = self.region.get();
        if width < MIN_REGION || height < MIN_REGION {
            self.dismiss();
            return;
        }
        let arguments = build_capture_arguments(self.offset, [x, y, width, height], self.scale);
        self.phase.set(Phase::Scroll);
        let _ = std::fs::remove_file(&self.path);
        self.screenshot_texture.replace(None);
        self.guide.widget.set_visible(false);
        self.controls.holder.set_visible(false);
        self.scroll.stitcher.replace(Some(Stitcher::new()));
        self.layout_panel();
        self.paint.queue_draw();
        if let Some(selector) = self.selector.upgrade() {
            selector.close_others(&self.screen);
        }
        self.start_capture(arguments);
    }

    fn start_capture(self: &Rc<Self>, arguments: Vec<String>) {
        let weak_selection = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let mut delay = Duration::ZERO;
            loop {
                if !delay.is_zero() {
                    glib::timeout_future(delay).await;
                }
                let Some(selection) = weak_selection.upgrade() else {
                    return;
                };
                if selection.phase.get() != Phase::Scroll {
                    return;
                }
                let Some(stitcher) = selection.scroll.stitcher.take() else {
                    return;
                };
                let preview_limits = selection.preview_limits();
                drop(selection);
                let arguments = arguments.clone();
                let Ok((stitcher, step)) =
                    gio::spawn_blocking(move || capture_step(stitcher, &arguments, preview_limits))
                        .await
                else {
                    if let Some(selection) = weak_selection.upgrade() {
                        selection.dismiss();
                    }
                    return;
                };
                let Some(selection) = weak_selection.upgrade() else {
                    return;
                };
                let Some(next_delay) = selection.apply_step(stitcher, step) else {
                    return;
                };
                delay = next_delay;
            }
        });
    }

    fn apply_step(self: &Rc<Self>, stitcher: Stitcher, step: Step) -> Option<Duration> {
        if let Some(preview) = step.preview {
            self.show_preview(preview);
        }
        self.show_status(&stitcher);
        let is_over = stitcher.has_lost_track() || stitcher.is_full();
        self.scroll.stitcher.replace(Some(stitcher));
        if self.scroll.is_finishing.get() {
            self.complete_scroll();
            return None;
        }
        if is_over {
            return None;
        }
        Some(match step.outcome {
            Outcome::Growth(_) | Outcome::Revisit => Duration::ZERO,
            _ => IDLE_INTERVAL,
        })
    }

    fn show_status(&self, stitcher: &Stitcher) {
        let message = if stitcher.has_lost_track() {
            tr("Lost track of the scrolling")
        } else if stitcher.is_full() {
            tr("Height limit reached")
        } else if stitcher.has_moved() {
            trf("%1 px", &[&stitcher.rows().to_string()])
        } else {
            tr("Scroll the content")
        };
        self.scroll.panel.status.set_text(&message);
        self.layout_panel();
    }

    fn show_preview(&self, preview: Frame) {
        let (width, height) = (preview.width as i32, preview.height as i32);
        let bytes = glib::Bytes::from_owned(preview.pixels);
        let texture = gdk::MemoryTexture::new(
            width,
            height,
            gdk::MemoryFormat::R8g8b8,
            &bytes,
            preview.width * 3,
        );
        let picture = &self.scroll.panel.picture;
        picture.set_paintable(Some(&texture));
        picture.set_size_request(
            (width as f64 / self.scale).round() as i32,
            (height as f64 / self.scale).round() as i32,
        );
        picture.set_visible(true);
        self.layout_panel();
    }

    fn preview_limits(&self) -> (usize, usize) {
        let height =
            (self.size.1 - PANEL_MARGIN * 2.0 - PANEL_CHROME_HEIGHT).clamp(1.0, PREVIEW_MAX_HEIGHT);
        (
            (self.scroll.preview_width.get() * self.scale).round() as usize,
            (height * self.scale).round() as usize,
        )
    }

    fn layout_panel(&self) {
        let panel = &self.scroll.panel.widget;
        let [x, y, width, _] = self.region.get();
        let (screen_width, screen_height) = self.size;
        let (left_gap, right_gap) = (x, screen_width - x - width);
        let preview_width = (left_gap.max(right_gap) - PANEL_MARGIN * 2.0 - PANEL_PADDING * 2.0)
            .min(PANEL_MAX_WIDTH);
        if preview_width < PANEL_MIN_WIDTH {
            self.scroll.preview_width.set(0.0);
            panel.set_visible(false);
            self.set_input_rectangle(None);
            return;
        }
        self.scroll.preview_width.set(preview_width);
        let content_width = preview_width.round() as i32;
        if let Some(content) = panel.first_child() {
            content.set_size_request(content_width, -1);
        }
        panel.set_visible(true);
        let panel_width = panel.measure(gtk4::Orientation::Horizontal, -1).1 as f64;
        let panel_height = panel
            .measure(gtk4::Orientation::Vertical, panel_width as i32)
            .1 as f64;
        let panel_x = if left_gap > right_gap {
            x - PANEL_MARGIN - panel_width
        } else {
            x + width + PANEL_MARGIN
        };
        let panel_y = y.clamp(
            PANEL_MARGIN,
            (screen_height - PANEL_MARGIN - panel_height).max(PANEL_MARGIN),
        );
        panel.set_margin_start(panel_x.round() as i32);
        panel.set_margin_top(panel_y.round() as i32);
        self.set_input_rectangle(Some(gtk4::cairo::RectangleInt::new(
            panel_x.round() as i32,
            panel_y.round() as i32,
            panel_width.ceil() as i32,
            panel_height.ceil() as i32,
        )));
    }

    fn set_input_rectangle(&self, rectangle: Option<gtk4::cairo::RectangleInt>) {
        let Some(surface) = self.window.surface() else {
            return;
        };
        let region = match rectangle {
            Some(rectangle) => gtk4::cairo::Region::create_rectangle(&rectangle),
            None => gtk4::cairo::Region::create(),
        };
        surface.set_input_region(Some(&region));
    }

    pub(super) fn finish_scroll(self: &Rc<Self>) {
        if self.phase.get() != Phase::Scroll {
            return;
        }
        self.scroll.is_finishing.set(true);
        if self.scroll.stitcher.borrow().is_some() {
            self.complete_scroll();
        }
    }

    fn complete_scroll(self: &Rc<Self>) {
        let Some(frame) = self
            .scroll
            .stitcher
            .take()
            .and_then(|stitcher| stitcher.into_frame())
        else {
            self.dismiss();
            return;
        };
        self.scroll.panel.widget.set_visible(false);
        self.set_input_rectangle(None);
        let crop = Crop {
            x: 0.0,
            y: 0.0,
            width: frame.width as f64,
            height: frame.height as f64,
            has_shadow: false,
            radius: 0.0,
        };
        let command = screenshot::command(
            &crop,
            &self.path,
            self.action(),
            &self.save_dir(),
            "",
            &process::executable(),
            self.config.region.should_notify,
        );
        let path = self.path.clone();
        self.has_snipped.set(true);
        let selection = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let is_written = gio::spawn_blocking(move || {
                let is_written = ppm::write(&path, &frame).is_ok();
                if !is_written {
                    let _ = std::fs::remove_file(&path);
                }
                is_written
            })
            .await
            .unwrap_or(false);
            if is_written {
                crate::platform::system::desktop::run_shell(&command);
            }
            if let Some(selection) = selection.upgrade() {
                selection.dismiss();
            }
        });
    }
}

fn build_capture_arguments(
    offset: (f64, f64),
    [x, y, width, height]: [f64; 4],
    scale: f64,
) -> Vec<String> {
    let inset = OUTLINE_INSET;
    let geometry = format!(
        "{},{} {}x{}",
        (offset.0 + x).round() + inset,
        (offset.1 + y).round() + inset,
        width.round() - inset * 2.0,
        height.round() - inset * 2.0
    );
    [
        "grim",
        "-t",
        "ppm",
        "-s",
        &scale.to_string(),
        "-g",
        &geometry,
        "-",
    ]
    .map(str::to_owned)
    .to_vec()
}

fn capture_step(
    mut stitcher: Stitcher,
    arguments: &[String],
    (preview_width, preview_height): (usize, usize),
) -> (Stitcher, Step) {
    let outcome = match grab(arguments) {
        Some(frame) => stitcher.push(frame),
        None => stitcher.reject(),
    };
    let preview = match outcome {
        Outcome::Start | Outcome::Growth(_) | Outcome::Limit => {
            stitcher.build_preview(preview_width, preview_height)
        }
        _ => None,
    };
    (stitcher, Step { outcome, preview })
}

fn grab(arguments: &[String]) -> Option<Frame> {
    let output = process::command(arguments).output().ok()?;
    if !output.status.success() {
        return None;
    }
    ppm::parse(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_capture_covers_the_region_inside_its_outline_at_the_monitor_scale() {
        assert_eq!(
            build_capture_arguments((1920.0, -200.0), [10.4, 20.0, 300.4, 400.0], 1.5),
            [
                "grim",
                "-t",
                "ppm",
                "-s",
                "1.5",
                "-g",
                "1931,-179 298x398",
                "-"
            ]
        );
        assert_eq!(
            build_capture_arguments((0.0, 0.0), [0.0, 0.0, 100.0, 100.0], 1.0)[4],
            "1"
        );
    }
}
