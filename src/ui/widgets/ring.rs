use gtk4::cairo;
use gtk4::pango;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::ui::theme::{SharedTheme, transparentize};

const SIZE: i32 = 20;
const LINE_WIDTH: f64 = 2.0;
const ICON_SIZE: f64 = 16.0;
const ICON_WEIGHT: i32 = 600;

#[derive(Clone, Copy)]
pub struct Shape {
    pub size: i32,
    pub icon_size: f64,
    pub icon_weight: i32,
}

pub struct Ring {
    pub area: gtk4::DrawingArea,
    value: Rc<Cell<f64>>,
    warning: Rc<Cell<bool>>,
    icon: Rc<RefCell<String>>,
}

impl Ring {
    pub fn new(theme: SharedTheme, icon: &str) -> Self {
        Self::shaped(
            theme,
            icon,
            Shape {
                size: SIZE,
                icon_size: ICON_SIZE,
                icon_weight: ICON_WEIGHT,
            },
        )
    }

    pub fn shaped(theme: SharedTheme, icon: &str, shape: Shape) -> Self {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(shape.size);
        area.set_content_height(shape.size);
        area.set_valign(gtk4::Align::Center);

        let value = Rc::new(Cell::new(0.0));
        let warning = Rc::new(Cell::new(false));
        let icon = Rc::new(RefCell::new(icon.to_owned()));

        area.set_draw_func({
            let value = value.clone();
            let warning = warning.clone();
            let icon = icon.clone();
            move |_, cr, width, height| {
                let theme = theme.borrow();
                let primary = if warning.get() {
                    theme.colors.col_error
                } else {
                    theme.colors.col_on_secondary_container
                };
                draw(
                    cr,
                    width,
                    height,
                    shape,
                    value.get(),
                    primary,
                    &icon.borrow(),
                    !warning.get(),
                );
            }
        });

        Ring {
            area,
            value,
            warning,
            icon,
        }
    }

    pub fn set(&self, value: f64, warning: bool) {
        self.value.set(value.clamp(0.0, 1.0));
        self.warning.set(warning);
        self.area.queue_draw();
    }

    pub fn set_icon(&self, icon: &str) {
        self.icon.replace(icon.to_owned());
        self.area.queue_draw();
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    shape: Shape,
    value: f64,
    primary: gtk4::gdk::RGBA,
    icon: &str,
    account_for_light_bleeding: bool,
) {
    let center_x = width as f64 / 2.0;
    let center_y = height as f64 / 2.0;
    let radius = shape.size as f64 / 2.0;
    let arc_radius = radius - LINE_WIDTH / 2.0 - if account_for_light_bleeding { 0.5 } else { 0.0 };

    let secondary = transparentize(primary, 0.5);
    set_source(cr, secondary);
    cr.arc(center_x, center_y, radius, 0.0, 2.0 * PI);
    let _ = cr.fill();

    let start = -PI / 2.0;
    set_source(cr, primary);
    cr.move_to(center_x, center_y);
    cr.arc(
        center_x,
        center_y,
        arc_radius,
        start,
        start + value * 2.0 * PI,
    );
    cr.line_to(center_x, center_y);
    cr.set_line_width(LINE_WIDTH);
    cr.set_line_join(cairo::LineJoin::Bevel);
    cr.set_line_cap(cairo::LineCap::Round);
    let _ = cr.fill_preserve();
    let _ = cr.stroke();

    cr.set_operator(cairo::Operator::DestOut);
    cr.set_source_rgba(0.0, 0.0, 0.0, 1.0);
    let layout = pangocairo::functions::create_layout(cr);
    let mut font = pango::FontDescription::from_string("Material Symbols Rounded");
    font.set_absolute_size(shape.icon_size * pango::SCALE as f64);
    font.set_variations(Some(&format!(
        "FILL=1,wght={},opsz={}",
        shape.icon_weight, shape.icon_size
    )));
    layout.set_font_description(Some(&font));
    layout.set_text(icon);
    let (glyph_width, glyph_height) = layout.pixel_size();
    cr.move_to(
        center_x - glyph_width as f64 / 2.0,
        center_y - glyph_height as f64 / 2.0,
    );
    pangocairo::functions::show_layout(cr, &layout);
    cr.set_operator(cairo::Operator::Over);
}

fn set_source(cr: &cairo::Context, color: gtk4::gdk::RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}
