use std::cell::RefCell;
use std::rc::{Rc, Weak};

use super::{
    CHOICE_PADDING, CHOICE_SQUARE, CONTROLS_MARGIN, COUNTDOWNS, LABEL_INSET, MENU_SPACING,
    MENU_WIDTH,
};
use crate::core::config::Config;
use crate::core::i18n::tr;
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ConfigSwitch;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::group::{GroupButton, Look as GroupLook};
use crate::ui::widgets::overlay::tooltip::{self, Tooltip};
use gtk4::prelude::*;

pub(super) fn options_menu(
    theme: &SharedTheme,
    config: &Rc<Config>,
) -> (gtk4::Box, Vec<Rc<ConfigSwitch>>, Rc<dyn Fn(bool)>) {
    let menu = gtk4::Box::new(gtk4::Orientation::Vertical, MENU_SPACING);
    menu.add_css_class("region-options");
    menu.set_size_request(MENU_WIDTH, -1);
    menu.set_halign(gtk4::Align::End);
    menu.set_margin_bottom(CONTROLS_MARGIN);

    let heading_label = text::styled(&tr("Wait before capturing"));
    text::set_color(&heading_label, "colSubtext");
    heading_label.set_xalign(0.0);
    let heading_box = Centered::filling_width(&heading_label);
    heading_box.set_margin_start(LABEL_INSET);
    menu.append(&heading_box);

    let countdown_row = gtk4::Box::new(gtk4::Orientation::Horizontal, MENU_SPACING);
    let buttons: Rc<Vec<(GroupButton, gtk4::Label, i64)>> = Rc::new(
        COUNTDOWNS
            .iter()
            .map(|(value, name)| {
                let label = text::styled(&tr(name));
                let content = Centered::integral(&label);
                let width =
                    content.measure(gtk4::Orientation::Horizontal, -1).1 + CHOICE_PADDING.0 * 2;
                let height =
                    content.measure(gtk4::Orientation::Vertical, -1).1 + CHOICE_PADDING.1 * 2;
                let button = GroupButton::new(theme, width as f64, height as f64);
                button.set_bounce(false);
                button.set_look(GroupLook {
                    background: |theme| theme.colors.col_secondary_container,
                    hover: |theme| theme.colors.col_secondary_container_hover,
                    active: |theme| theme.colors.col_secondary_container_active,
                    ..GroupLook::default()
                });
                button.set_content(&content);
                countdown_row.append(&button);
                (button, label, *value)
            })
            .collect(),
    );
    let update_countdown: Rc<dyn Fn(bool)> = Rc::new({
        let buttons = buttons.clone();
        let config = config.clone();
        move |should_animate: bool| {
            let current_countdown = config.region.countdown.get();
            let last_index = buttons.len() - 1;
            for (index, (button, label, value)) in buttons.iter().enumerate() {
                let is_toggled = *value == current_countdown;
                button.set_toggled(is_toggled);
                text::set_color(
                    label,
                    if is_toggled {
                        "colOnPrimary"
                    } else {
                        "colOnSecondaryContainer"
                    },
                );
                let full_radius = button.height() as f64 / 2.0;
                let full_radius = if full_radius > 0.0 {
                    full_radius
                } else {
                    label.measure(gtk4::Orientation::Vertical, -1).1 as f64 / 2.0
                        + CHOICE_PADDING.1 as f64
                };
                let side_radius = |is_edge: bool| {
                    if is_toggled || is_edge {
                        full_radius
                    } else {
                        CHOICE_SQUARE
                    }
                };
                button.set_side_radii(side_radius(index == 0), side_radius(index == last_index));
                if !should_animate {
                    button.jump_radius();
                }
            }
        }
    });
    for (button, _, value) in buttons.iter() {
        button.connect_clicked({
            let config = config.clone();
            let update_countdown = Rc::downgrade(&update_countdown);
            let value = *value;
            move || {
                config.region.set_countdown(value);
                if let Some(update_countdown) = update_countdown.upgrade() {
                    update_countdown(true);
                }
            }
        });
    }
    update_countdown(false);
    menu.append(&countdown_row);

    let options = &config.region;
    let save_tip = if options.save_path.is_empty() {
        tr("No folder is set yet — pick one in Settings")
    } else {
        options.save_path.clone()
    };
    let rows: [(&str, &str, bool, fn(&Config, bool)); 4] = [
        (
            "save",
            "Also save to a file",
            options.save.get(),
            |config, is_on| config.region.set_save(is_on),
        ),
        (
            "mic",
            "Record the microphone",
            options.record_sound.get(),
            |config, is_on| config.region.set_record_sound(is_on),
        ),
        (
            "arrow_selector_tool",
            "Include the pointer",
            options.show_pointer.get(),
            |config, is_on| config.region.set_show_pointer(is_on),
        ),
        (
            "history",
            "Start from the last region",
            options.remember_region.get(),
            |config, is_on| config.region.set_remember_region(is_on),
        ),
    ];
    let mut switches = Vec::new();
    for (index, (icon, name, is_checked, apply)) in rows.into_iter().enumerate() {
        let slot: Rc<RefCell<Weak<ConfigSwitch>>> = Rc::new(RefCell::new(Weak::new()));
        let switch = ConfigSwitch::new(theme, icon, &tr(name), {
            let config = config.clone();
            let slot = slot.clone();
            move |is_on| {
                apply(&config, is_on);
                if let Some(switch) = slot.borrow().upgrade() {
                    switch.set(is_on);
                }
            }
        });
        slot.replace(Rc::downgrade(&switch));
        switch.set(is_checked);
        if index == 0 {
            let tip = Tooltip::new(&switch.button, theme, tooltip::Kind::Styled);
            tip.set_text(&save_tip);
            tooltip::hover_delay(&switch.button, &tip, 0);
        }
        menu.append(&switch.button);
        switches.push(switch);
    }
    (menu, switches, update_countdown)
}
