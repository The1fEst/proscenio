use gtk4::prelude::*;

use crate::core::scope::Scope;
use crate::services::xkb::Xkb;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::text;

pub fn build(xkb: &Xkb, scope: &Scope) -> gtk4::Widget {
    let label = text::styled("");
    label.set_justify(gtk4::Justification::Center);
    label.connect_label_notify(|label| {
        let size = if label.text().contains('\n') {
            pixel_size::SMALLIE
        } else {
            pixel_size::SMALL
        };
        text::set_font(label, text::Family::Main, size as f64, "wght=450");
    });
    let (holder, change) = text::animate_change(&label);
    holder.set_valign(gtk4::Align::Center);

    let show = {
        let xkb = xkb.clone();
        let holder = holder.clone();
        let label = label.clone();
        let first = std::cell::Cell::new(true);
        move || {
            holder.set_visible(xkb.layout_codes.borrow().len() > 1);
            let value = abbreviate(&xkb.current_code.borrow());
            if first.replace(false) {
                label.set_text(&value);
            } else {
                change(&value);
            }
        }
    };
    show();
    scope.keep(xkb.subscribe(show));

    holder
}

fn abbreviate(code: &str) -> String {
    code.split(':')
        .map(|layout| {
            layout
                .split('-')
                .next()
                .unwrap_or(layout)
                .chars()
                .take(4)
                .collect::<String>()
                .to_uppercase()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
