use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::panels::settings::Settings;
use crate::services::hardware::xkb::Xkb;
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::tooltip;

const SETTINGS_PAGE: &str = "keyboard";
const HORIZONTAL_BAR_PADDING: (i32, i32) = (10, 5);
const VERTICAL_BAR_PADDING: (i32, i32) = (6, 4);
const SPACING: i32 = 15;
const VERTICAL_SPACING: i32 = 8;

pub fn build(
    xkb: &Xkb,
    theme: &SharedTheme,
    settings: &Rc<Settings>,
    is_vertical: bool,
    scope: &Scope,
) -> gtk4::Widget {
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

    let button = RippleButton::new(theme);
    button.add_css_class("bar-indicators");
    button.set_size_request(-1, -1);
    button.set_radius(rounding::FULL as f64);
    let (horizontal_padding, vertical_padding) = if is_vertical {
        VERTICAL_BAR_PADDING
    } else {
        HORIZONTAL_BAR_PADDING
    };
    button.set_content(&holder, horizontal_padding, vertical_padding);
    button.set_valign(gtk4::Align::Center);
    button.set_halign(gtk4::Align::Center);
    button.connect_clicked({
        let xkb = xkb.clone();
        move |_| xkb.cycle_layout()
    });
    button.connect_alt({
        let settings = settings.clone();
        move |_| settings.open(Some(SETTINGS_PAGE))
    });

    let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll({
        let xkb = xkb.clone();
        move |_, _, delta| {
            if delta > 0.0 {
                xkb.cycle_layout();
            } else if delta < 0.0 {
                xkb.cycle_layout_backward();
            }
            gtk4::glib::Propagation::Stop
        }
    });
    button.add_controller(wheel);

    let tip = tooltip::Tooltip::new(&button, theme, tooltip::Kind::Popup);
    if !is_vertical {
        tip.below();
    }
    tooltip::hover_delay(&button, &tip, 0);

    let module = if is_vertical {
        gtk4::Box::new(gtk4::Orientation::Vertical, VERTICAL_SPACING)
    } else {
        gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING)
    };
    module.set_halign(gtk4::Align::Center);
    module.set_valign(gtk4::Align::Center);
    module.append(&button);
    let bullet = text::styled_sized("•", pixel_size::LARGER);
    text::set_color(&bullet, "colSubtext");
    module.append(&Centered::new(&bullet));

    let show = {
        let xkb = xkb.clone();
        let module = module.clone();
        let label = label.clone();
        let first = std::cell::Cell::new(true);
        move || {
            module.set_visible(xkb.layout_codes.borrow().len() > 1);
            tip.set_text(&xkb.current_name.borrow());
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

    module.upcast()
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
