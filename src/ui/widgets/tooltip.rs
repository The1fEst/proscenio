use gtk4::cairo;
use gtk4::glib;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::ui::anim::{self, EXPRESSIVE_EFFECTS};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::text::{self, Family};

const HORIZONTAL_PADDING: f64 = 10.0;
const VERTICAL_PADDING: f64 = 5.0;
const STYLED_GAP: i32 = 3;

pub enum Kind {
    Popup,
    Styled,
}

const QT_GAP: i32 = 3;
const QT_MARGIN: f32 = 6.0;

pub struct Tooltip {
    popover: gtk4::Popover,
    area: gtk4::DrawingArea,
    text: Rc<RefCell<String>>,
    shown: Rc<anim::Motion>,
    open: Cell<bool>,
    qt_placement: Cell<bool>,
    pointing: Cell<Option<(i32, i32, i32, i32)>>,
    exact: Rc<Cell<Option<(f64, f64)>>>,
}

impl Tooltip {
    pub fn new(target: &impl IsA<gtk4::Widget>, theme: &SharedTheme, kind: Kind) -> Rc<Self> {
        let area = gtk4::DrawingArea::new();
        let text = Rc::new(RefCell::new(String::new()));
        let shown = anim::Motion::new(&area, 0.0, 200.0, EXPRESSIVE_EFFECTS);
        let exact: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));

        area.set_draw_func({
            let text = text.clone();
            let theme = theme.clone();
            let shown = shown.clone();
            let exact = exact.clone();
            move |area, cr, width, height| {
                draw(
                    area,
                    cr,
                    width,
                    height,
                    &text.borrow(),
                    &theme,
                    shown.get(),
                    exact.get(),
                );
            }
        });

        let popover = gtk4::Popover::new();
        popover.add_css_class("tooltip-popover");
        popover.set_has_arrow(false);
        popover.set_autohide(false);
        popover.set_can_focus(false);
        popover.set_can_target(false);
        popover.set_position(gtk4::PositionType::Top);
        if let Kind::Styled = kind {
            popover.set_offset(0, -STYLED_GAP);
        }
        popover.set_child(Some(&area));
        popover.set_parent(target.as_ref());
        target.as_ref().connect_destroy({
            let popover = popover.downgrade();
            move |_| {
                if let Some(popover) = popover.upgrade() {
                    popover.unparent();
                }
            }
        });
        popover.connect_map(|popover| {
            if let Some(surface) = popover.native().and_then(|native| native.surface()) {
                surface.set_input_region(Some(&cairo::Region::create()));
            }
        });

        let margins = match kind {
            Kind::Popup => (HORIZONTAL_PADDING as i32, VERTICAL_PADDING as i32),
            Kind::Styled => (0, 0),
        };
        area.set_margin_start(margins.0);
        area.set_margin_end(margins.0);
        area.set_margin_top(margins.1);
        area.set_margin_bottom(margins.1);

        Rc::new(Tooltip {
            popover,
            area,
            text,
            shown,
            open: Cell::new(false),
            qt_placement: Cell::new(false),
            pointing: Cell::new(None),
            exact,
        })
    }

    pub fn point_at(&self, x: i32, y: i32, width: i32, height: i32) {
        self.pointing.set(Some((x, y, width, height)));
        self.popover
            .set_pointing_to(Some(&gtk4::gdk::Rectangle::new(x, y, width, height)));
        if self.open.get() {
            self.popover.present();
        }
    }

    pub fn below(&self) {
        self.popover.set_position(gtk4::PositionType::Bottom);
    }

    pub fn place_like_qt(&self) {
        self.qt_placement.set(true);
    }

    fn place(&self) {
        let Some(target) = self.popover.parent() else {
            return;
        };
        let Some(native) = target.native() else {
            return;
        };
        let Some(bounds) = target.compute_bounds(&native) else {
            return;
        };
        let (top, span) = match self.pointing.get() {
            Some((_, y, _, height)) => (bounds.y() + y as f32, height as f32),
            None => (bounds.y(), bounds.height()),
        };
        let height = self.area.content_height() as f32 + 2.0 * VERTICAL_PADDING as f32;
        let window = native.height() as f32;
        let gap = QT_GAP as f32;
        let above = top - height - gap;
        let fits = |top: f32| top >= QT_MARGIN && top + height <= window - QT_MARGIN;
        let below = top + span + gap;
        let inset = QT_GAP + VERTICAL_PADDING as i32;
        if !fits(above) && fits(below) {
            self.popover.set_position(gtk4::PositionType::Bottom);
            self.popover.set_offset(0, inset);
            self.area.set_valign(gtk4::Align::Start);
        } else {
            self.popover.set_position(gtk4::PositionType::Top);
            self.popover.set_offset(0, -inset);
            self.area.set_valign(gtk4::Align::End);
        }
    }

    pub fn set_text(&self, value: &str) {
        self.text.replace(value.to_owned());
        if self.qt_placement.get() {
            let (width, height) = exact_size(&self.area, value);
            self.exact.set(Some((width, height)));
            self.area.set_content_width(width.ceil() as i32);
            self.area.set_content_height(height.ceil() as i32);
        } else {
            let (width, height) = measure(&self.area, value);
            self.area.set_content_width(width);
            self.area.set_content_height(height);
        }
        self.area.queue_draw();
    }

    pub fn show(&self, visible: bool) {
        if self.open.replace(visible) == visible {
            return;
        }
        if visible {
            if self.qt_placement.get() {
                self.place();
            }
            self.shown.jump(0.0);
            self.popover.popup();
            self.shown.to(1.0);
        } else {
            self.popover.popdown();
            self.shown.jump(0.0);
        }
    }
}

fn layout(widget: &impl IsA<gtk4::Widget>, value: &str) -> pango::Layout {
    let layout = widget.create_pango_layout(Some(value));
    layout.set_font_description(Some(&text::font(
        Family::Main,
        pixel_size::SMALLER as f64,
        "wght=450",
    )));
    layout
}

fn exact_size(widget: &impl IsA<gtk4::Widget>, value: &str) -> (f64, f64) {
    let (_, logical) = layout(widget, value).extents();
    let scale = pango::SCALE as f64;
    (
        logical.width() as f64 / scale + 2.0 * HORIZONTAL_PADDING,
        logical.height() as f64 / scale + 2.0 * VERTICAL_PADDING,
    )
}

fn measure(widget: &impl IsA<gtk4::Widget>, value: &str) -> (i32, i32) {
    let (width, height) = layout(widget, value).pixel_size();
    (
        width + 2 * HORIZONTAL_PADDING as i32,
        height + 2 * VERTICAL_PADDING as i32,
    )
}

fn draw(
    area: &gtk4::DrawingArea,
    cr: &cairo::Context,
    width: i32,
    height: i32,
    value: &str,
    theme: &SharedTheme,
    shown: f64,
    exact: Option<(f64, f64)>,
) {
    if shown <= 0.0 {
        return;
    }
    let theme = theme.borrow();
    let (width, height) = (width as f64, height as f64);
    let (full_width, full_height) = exact.unwrap_or((width, height));
    let (bubble_width, bubble_height) = (full_width * shown, full_height * shown);
    let left = (width - bubble_width) / 2.0;
    let top = height - bubble_height;
    let radius = (rounding::VERYSMALL as f64)
        .min(bubble_width / 2.0)
        .min(bubble_height / 2.0);

    cr.push_group();
    rounded(cr, left, top, bubble_width, bubble_height, radius);
    cr.clip();
    let background = theme.colors.col_tooltip;
    cr.set_source_rgba(
        background.red() as f64,
        background.green() as f64,
        background.blue() as f64,
        background.alpha() as f64,
    );
    let _ = cr.paint();

    let layout = layout(area, value);
    let foreground = theme.colors.col_on_tooltip;
    cr.set_source_rgba(
        foreground.red() as f64,
        foreground.green() as f64,
        foreground.blue() as f64,
        foreground.alpha() as f64,
    );
    if exact.is_some() {
        let (_, logical) = layout.extents();
        let scale = pango::SCALE as f64;
        let half = |size: f64| (size / 2.0 + 0.5).floor();
        cr.move_to(
            left + half(bubble_width) - half(logical.width() as f64 / scale),
            top + half(bubble_height) - half(logical.height() as f64 / scale),
        );
    } else {
        let (text_width, text_height) = layout.pixel_size();
        cr.move_to(
            (left + (bubble_width - text_width as f64) / 2.0).round(),
            (top + (bubble_height - text_height as f64) / 2.0).round(),
        );
    }
    pangocairo::functions::show_layout(cr, &layout);
    let _ = cr.pop_group_to_source();
    let _ = cr.paint_with_alpha(shown);
}

fn rounded(cr: &cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    use std::f64::consts::PI;
    cr.new_sub_path();
    cr.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    cr.arc(
        x + width - radius,
        y + height - radius,
        radius,
        0.0,
        PI / 2.0,
    );
    cr.arc(x + radius, y + height - radius, radius, PI / 2.0, PI);
    cr.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    cr.close_path();
}

pub fn hover_delay(target: &impl IsA<gtk4::Widget>, tooltip: &Rc<Tooltip>, delay: u64) {
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::new(RefCell::new(None));
    let motion = gtk4::EventControllerMotion::new();
    motion.connect_enter({
        let tooltip = tooltip.clone();
        let pending = pending.clone();
        move |_, _, _| {
            if delay == 0 {
                tooltip.show(true);
                return;
            }
            let tooltip = tooltip.clone();
            let slot = pending.clone();
            let id =
                glib::timeout_add_local_once(std::time::Duration::from_millis(delay), move || {
                    slot.replace(None);
                    tooltip.show(true);
                });
            if let Some(old) = pending.replace(Some(id)) {
                old.remove();
            }
        }
    });
    motion.connect_leave({
        let tooltip = tooltip.clone();
        move |_| {
            if let Some(id) = pending.replace(None) {
                id.remove();
            }
            tooltip.show(false);
        }
    });
    target.add_controller(motion);
}
