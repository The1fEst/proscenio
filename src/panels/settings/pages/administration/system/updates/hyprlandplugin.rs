use gtk4::prelude::*;
use std::rc::Rc;

use super::append_to_header;
use super::updatepackages::card;
use crate::core::i18n::tr;
use crate::panels::settings::content::Page;
use crate::platform::hyprland::hyprpm;
use crate::services::updates::pluginrebuild::{self, Status};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::progress::{Colors, ProgressBar};
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;

pub fn add_section(page: &Rc<Page>) {
    let section = page.section("extension", &tr("Hyprland plugin"));
    let (install_button, _) =
        page.icon_button("download", true, &tr("Install"), pluginrebuild::install);
    let (update_button, _) =
        page.icon_button("upgrade", true, &tr("Update"), pluginrebuild::update);
    append_to_header(&section, &[&install_button, &update_button]);
    let notice = page.notice(
        &section,
        "info",
        &tr("Title bars and the fullscreen toggle come from the proscenio_hyprland Hyprland plugin, which is not enabled in hyprpm. Install builds and enables it."),
    );

    let (plugin_card, lines) = card("extension", hyprpm::PLUGIN_NAME, false);
    let status_label = text::styled_sized("", pixel_size::SMALLER);
    status_label.set_xalign(0.0);
    status_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centered::filling_width(&status_label));
    section.append(&plugin_card);
    let progress_bar = ProgressBar::new();
    progress_bar.set_hexpand(true);
    progress_bar.set_value(1.0);
    {
        let theme = page.theme.borrow();
        progress_bar.set_colors(Colors {
            highlight: theme.colors.col_primary,
            track: theme.m3.secondary_container,
        });
    }
    section.append(&progress_bar);

    let show_state = move || {
        let is_running = pluginrebuild::is_running();
        let is_enabled = hyprpm::is_plugin_enabled_in_hyprpm();
        notice.set_visible(!is_enabled);
        install_button.set_visible(!is_enabled);
        install_button.set_sensitive(!is_running);
        update_button.set_visible(is_enabled);
        update_button.set_sensitive(!is_running);
        progress_bar.set_visible(is_running);
        progress_bar.set_wavy(is_running);
        progress_bar.set_wave_moving(is_running);
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
            Status::Busy(message) | Status::Success(message) | Status::Failure(message) => message,
        };
        status_label.set_visible(!status_text.is_empty());
        status_label.set_text(&status_text);
    };
    show_state();
    page.keep(pluginrebuild::subscribe(show_state));
}
