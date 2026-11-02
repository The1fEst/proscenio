use gtk4::prelude::*;

use crate::ui::theme::{SharedTheme, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;

pub fn build(theme: &SharedTheme, name: &str, glyph_size: i32, button_size: i32) -> RippleButton {
    let symbol = text::symbol(name, glyph_size as f64);
    text::set_color(&symbol, "colOnLayer1");
    let button = RippleButton::new(theme);
    button.set_radius(rounding::FULL as f64);
    button.set_size_request(button_size, button_size);
    button.set_valign(gtk4::Align::Center);
    button.set_content(&Centered::integral(&symbol), 0, 0);
    button
}
