use gtk4::prelude::*;

use crate::ui::shapes::{self, Shape};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::ripple::Token;

pub struct MaterialShape {
    pub area: gtk4::DrawingArea,
}

impl MaterialShape {
    pub fn new(theme: &SharedTheme, shape: Shape, size: i32, colour: Token) -> Self {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(size);
        area.set_content_height(size);
        area.set_draw_func({
            let theme = theme.clone();
            move |_, cr, width, height| {
                let size = width.min(height) as f64;
                let colour = colour(&theme.borrow());
                cr.set_source_rgba(
                    colour.red() as f64,
                    colour.green() as f64,
                    colour.blue() as f64,
                    colour.alpha() as f64,
                );
                shapes::polygon(shape).trace(cr, 0.0, 0.0, size);
                let _ = cr.fill();
            }
        });
        MaterialShape { area }
    }
}
