use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::{Centred, layout_qt_metrics};
use crate::ui::widgets::flow::Flow;
use crate::ui::widgets::group::{GroupButton, Look};
use crate::ui::widgets::text;

const SPACING: i32 = 2;
const PADDING_X: i32 = 12;
const PADDING_Y: i32 = 8;
const ICON_GAP: i32 = 4;

pub struct Choice {
    pub label: String,
    pub icon: &'static str,
    pub value: Value,
}

struct Item {
    button: GroupButton,
    value: Value,
    parts: Vec<gtk4::Label>,
}

pub struct Selection {
    pub root: Flow,
    items: Vec<Item>,
    lines: RefCell<Vec<usize>>,
    height: i32,
}

impl Selection {
    pub fn new(
        theme: &SharedTheme,
        choices: Vec<Choice>,
        selected: impl Fn(Value) + 'static,
    ) -> Rc<Self> {
        Self::with_family(theme, choices, text::Family::Main, selected)
    }

    pub fn with_family(
        theme: &SharedTheme,
        choices: Vec<Choice>,
        family: text::Family,
        selected: impl Fn(Value) + 'static,
    ) -> Rc<Self> {
        let root = Flow::new(SPACING);
        root.set_hexpand(true);
        let height = line_height() + PADDING_Y * 2;
        let selected = Rc::new(selected);
        let mut items = Vec::with_capacity(choices.len());
        for choice in choices {
            let row = gtk4::Box::new(
                gtk4::Orientation::Horizontal,
                if choice.label.is_empty() { 0 } else { ICON_GAP },
            );
            row.set_margin_start(PADDING_X);
            row.set_margin_end(PADDING_X);
            row.set_margin_top(PADDING_Y);
            row.set_margin_bottom(PADDING_Y);
            let mut parts = Vec::new();
            if !choice.icon.is_empty() {
                let symbol = text::symbol(choice.icon, pixel_size::LARGER as f64);
                row.append(&Centred::integral(&symbol));
                parts.push(symbol);
            }
            if !choice.label.is_empty() {
                let label = text::styled(&choice.label);
                if !matches!(family, text::Family::Main) {
                    text::set_font(&label, family, pixel_size::SMALL as f64, "wght=450");
                }
                row.append(&Centred::new(&label));
                parts.push(label);
            }
            let width = row.measure(gtk4::Orientation::Horizontal, -1).1;
            let button = GroupButton::new(theme, width as f64, height as f64);
            button.set_look(Look {
                background: |theme| theme.colors.col_secondary_container,
                hover: |theme| theme.colors.col_secondary_container_hover,
                active: |theme| theme.colors.col_secondary_container_active,
                ..Look::default()
            });
            button.set_bounce(false);
            button.set_content(&row);
            let value = choice.value;
            button.connect_clicked({
                let (selected, value) = (selected.clone(), value.clone());
                move || selected(value.clone())
            });
            root.append(&button);
            items.push(Item {
                button,
                value,
                parts,
            });
        }
        let lines = vec![0; items.len()];
        let selection = Rc::new(Selection {
            root,
            items,
            lines: RefCell::new(lines),
            height,
        });
        selection.shape();
        selection.root.connect_lines_changed({
            let selection = Rc::downgrade(&selection);
            move |lines| {
                if let Some(selection) = selection.upgrade() {
                    selection.lines.replace(lines.to_vec());
                    selection.shape();
                }
            }
        });
        selection
    }

    pub fn button(&self, index: usize) -> Option<&GroupButton> {
        self.items.get(index).map(|item| &item.button)
    }

    pub fn set_current(&self, current: &Value) {
        for item in &self.items {
            item.button.set_toggled(item.value == *current);
        }
        self.shape();
    }

    fn shape(&self) {
        let lines = self.lines.borrow();
        let last = self.items.len().saturating_sub(1);
        let full = self.height as f64 / 2.0;
        for (index, item) in self.items.iter().enumerate() {
            let toggled = item.button.toggled();
            let leftmost = index == 0 || lines.get(index - 1) != lines.get(index);
            let rightmost = index == last || lines.get(index + 1) != lines.get(index);
            let round = |edge: bool| {
                if toggled || edge {
                    full
                } else {
                    rounding::UNSHARPENMORE as f64
                }
            };
            item.button
                .set_side_radii(round(leftmost), round(rightmost));
            if !self.root.is_mapped() {
                item.button.jump_radius();
            }
            let token = if toggled {
                "colOnPrimary"
            } else {
                "colOnSecondaryContainer"
            };
            for part in &item.parts {
                text::set_color(part, token);
            }
        }
    }
}

fn line_height() -> i32 {
    let probe = text::styled("Abc");
    text::set_font(
        &probe,
        text::Family::Main,
        text::application_pixel_size(),
        "",
    );
    layout_qt_metrics(&probe.layout()).map_or(0, |(height, _)| height as i32)
}
