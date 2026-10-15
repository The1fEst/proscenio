use gtk4::prelude::*;

use crate::ui::shapes::{self, Shape};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::input::ripple::Token;

pub struct MaterialShape {
    pub area: gtk4::DrawingArea,
}

impl MaterialShape {
    pub fn new(theme: &SharedTheme, shape: Shape, size: i32, color: Token) -> Self {
        let area = gtk4::DrawingArea::new();
        area.set_content_width(size);
        area.set_content_height(size);
        area.set_draw_func({
            let theme = theme.clone();
            move |_, cr, width, height| {
                let size = width.min(height) as f64;
                let color = color(&theme.borrow());
                cr.set_source_rgba(
                    color.red() as f64,
                    color.green() as f64,
                    color.blue() as f64,
                    color.alpha() as f64,
                );
                shapes::polygon(shape).trace(cr, 0.0, 0.0, size);
                let _ = cr.fill();
            }
        });
        MaterialShape { area }
    }
}
