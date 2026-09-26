use gtk4::prelude::*;
use std::rc::Rc;

use crate::ui::theme::SharedTheme;
use crate::ui::widgets::column::Column;
use crate::ui::widgets::{reveal, text, tooltip};

const ICON_SIZE: f64 = 14.0;
const SPACING: i32 = -5;
const TOOLTIP_DELAY: u64 = 500;

pub struct Hint {
    pub holder: gtk4::Box,
    icon: gtk4::Label,
    reveal: Rc<dyn Fn(bool)>,
}

impl Hint {
    pub fn new(icon: &str, tip: &str, theme: &SharedTheme) -> Self {
        let symbol = glyph(icon);
        let column = Column::new(SPACING, false);
        column.append(&glyph("keyboard_arrow_up"));
        column.append(&symbol);
        column.append(&glyph("keyboard_arrow_down"));
        column.set_valign(gtk4::Align::Center);

        let tooltip = tooltip::Tooltip::new(&column, theme, tooltip::Kind::Popup);
        tooltip.set_text(tip);
        tooltip::hover_delay(&column, &tooltip, TOOLTIP_DELAY);

        let (holder, reveal) = reveal::with_gap(&column, false, reveal::Gap::None);
        holder.set_valign(gtk4::Align::Center);

        Hint {
            holder,
            icon: symbol,
            reveal,
        }
    }

    pub fn set_icon(&self, icon: &str) {
        self.icon.set_text(icon);
    }

    pub fn show(&self, shown: bool) {
        (self.reveal)(shown);
    }
}

fn glyph(name: &str) -> gtk4::Label {
    let label = text::symbol(name, ICON_SIZE);
    text::set_color(&label, "colSubtext");
    label
}
