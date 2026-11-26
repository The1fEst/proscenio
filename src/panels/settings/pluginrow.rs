use gtk4::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::Page;
use crate::panels::settings::pages::administration::system::updates::updatepackages::card;
use crate::platform::hyprland::hyprpm;
use crate::services::updates::pluginrebuild::{self, Status};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;

const BUTTON_END_MARGIN_PX: i32 = 12;

pub fn follow(page: &Rc<Page>, section: &gtk4::Box, add_options: fn(&Page, &gtk4::Box)) {
    let is_shown_enabled = Cell::new(hyprpm::is_plugin_enabled_in_hyprpm());
    fill(page, section, is_shown_enabled.get(), add_options);
    page.keep(pluginrebuild::subscribe({
        let page = Rc::downgrade(page);
        let section = section.downgrade();
        move || {
            if pluginrebuild::is_running() {
                return;
            }
            let is_enabled = hyprpm::is_plugin_enabled_in_hyprpm();
            if is_shown_enabled.replace(is_enabled) == is_enabled {
                return;
            }
            if let (Some(page), Some(section)) = (page.upgrade(), section.upgrade()) {
                fill(&page, &section, is_enabled, add_options);
            }
        }
    }));
}

fn fill(page: &Page, section: &gtk4::Box, is_enabled: bool, add_options: fn(&Page, &gtk4::Box)) {
    while let Some(child) = section.last_child() {
        section.remove(&child);
    }
    if is_enabled {
        add_options(page, section);
        add_row(
            page,
            section,
            "upgrade",
            &tr("Update"),
            pluginrebuild::update,
        );
        return;
    }
    page.notice(
        section,
        "info",
        &tr("Title bars and the fullscreen toggle come from the proscenio_hyprland Hyprland plugin, which is not enabled in hyprpm. Install builds and enables it."),
    );
    add_row(
        page,
        section,
        "download",
        &tr("Install"),
        pluginrebuild::install,
    );
}

fn add_row(page: &Page, section: &gtk4::Box, icon: &str, label: &str, on_clicked: fn()) {
    let (plugin_card, lines) = card("extension", hyprpm::PLUGIN_NAME, false);
    let status_label = text::styled_sized("", pixel_size::SMALLER);
    status_label.set_xalign(0.0);
    status_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centered::filling_width(&status_label));
    let (button, _) = page.icon_button(icon, true, label, on_clicked);
    button.set_margin_end(BUTTON_END_MARGIN_PX);
    plugin_card.append(&button);
    section.append(&plugin_card);
    let show_status = {
        let status_label = status_label.downgrade();
        let button = button.downgrade();
        move || {
            let (Some(status_label), Some(button)) = (status_label.upgrade(), button.upgrade())
            else {
                return;
            };
            button.set_sensitive(!pluginrebuild::is_running());
            let status = pluginrebuild::status();
            text::set_color(
                &status_label,
                if matches!(status, Status::Failure(_)) {
                    "colError"
                } else {
                    "colSubtext"
                },
            );
            let status_text = match status {
                Status::Idle => String::new(),
                Status::Busy(message) | Status::Success(message) | Status::Failure(message) => {
                    message
                }
            };
            status_label.set_visible(!status_text.is_empty());
            status_label.set_text(&status_text);
        }
    };
    show_status();
    page.keep(pluginrebuild::subscribe(show_status));
}
