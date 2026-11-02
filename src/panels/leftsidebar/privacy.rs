use gtk4::prelude::*;

use crate::core::i18n::{tr, trf};
use crate::panels::bar::privacy::{Indicator, build_disc, list_active_indicators};
use crate::platform::system::appicon;
use crate::services::hyprland::screencast::{Kind, Target};
use crate::services::media::deviceusage::{DeviceUsage, Usage};
use crate::services::media::privacy::Application;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::layout::centered::Centered;

const MARGIN: i32 = 10;
const BADGE_SIZE: i32 = 23;
const BADGE_SYMBOL_SIZE: f64 = 14.0;
const SECTION_SPACING: i32 = 6;
const ICON_SIZE: i32 = 28;
const ROW_SPACING: i32 = 8;
const SECTION_GAP: i32 = 8;
const MAX_TITLE_CHARS: i32 = 34;

pub struct Privacy {
    pub widget: gtk4::Box,
    usage: DeviceUsage,
}

impl Privacy {
    pub fn new(usage: &DeviceUsage) -> Self {
        let widget = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
        widget.add_css_class("notepad-page");
        widget.set_margin_top(MARGIN);
        widget.set_visible(false);
        Privacy {
            widget,
            usage: usage.clone(),
        }
    }

    pub fn show(&self) {
        self.usage.refresh();
        self.refresh();
    }

    pub fn refresh(&self) {
        let usage = self.usage.usage();
        while let Some(child) = self.widget.first_child() {
            self.widget.remove(&child);
        }
        self.widget
            .set_visible(!list_active_indicators(&usage).is_empty());
        self.fill(&usage);
        if let Some(last) = self.widget.last_child() {
            last.set_margin_bottom(MARGIN);
        }
    }

    fn fill(&self, usage: &Usage) {
        let column = &self.widget;
        if !usage.microphones.is_empty() {
            append_section(column, Indicator::Microphone, &tr("Microphone"));
            for entry in &usage.microphones {
                column.append(&build_entry_row(
                    Some(&entry.application),
                    "mic",
                    &entry.application.name,
                    &entry.device,
                ));
            }
        }
        if !usage.screens.is_empty() {
            append_section(column, Indicator::Screen, &tr("Screen sharing"));
            let subjects: Vec<String> = usage.screens.iter().map(describe_target).collect();
            if usage.screen_applications.is_empty() {
                for subject in &subjects {
                    column.append(&build_entry_row(None, "screen_share", subject, ""));
                }
            }
            for application in &usage.screen_applications {
                column.append(&build_entry_row(
                    Some(application),
                    "screen_share",
                    &application.name,
                    &subjects.join(", "),
                ));
            }
        }
        if !usage.cameras.is_empty() {
            append_section(column, Indicator::Camera, &tr("Camera"));
            for entry in &usage.cameras {
                column.append(&build_entry_row(
                    Some(&entry.application),
                    "videocam",
                    &entry.application.name,
                    &entry.device,
                ));
            }
        }
    }
}

fn build_badge(indicator: Indicator) -> gtk4::Widget {
    let symbol = text::symbol_filled(indicator.symbol(), BADGE_SYMBOL_SIZE, 1.0);
    symbol.add_css_class("privacy-icon");
    let badge = gtk4::Overlay::new();
    badge.set_child(Some(&build_disc(indicator, BADGE_SIZE)));
    badge.add_overlay(&Centered::by_ink(&symbol));
    badge.set_valign(gtk4::Align::Center);
    badge.upcast()
}

fn append_section(column: &gtk4::Box, indicator: Indicator, title: &str) {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, SECTION_SPACING);
    header.append(&build_badge(indicator));
    let label = text::styled_sized(title, pixel_size::NORMAL);
    text::set_color(&label, "colOnLayer1");
    label.set_xalign(0.0);
    header.append(&Centered::new(&label));
    header.set_margin_start(MARGIN);
    header.set_margin_end(MARGIN);
    header.set_margin_top(if column.first_child().is_some() {
        SECTION_GAP
    } else {
        MARGIN
    });
    column.append(&header);
}

fn describe_target(target: &Target) -> String {
    let text = match target.kind {
        Kind::Monitor => "Monitor: %1",
        Kind::Window => "Window: %1",
        Kind::Region => "Region: %1",
        Kind::Workspace => "Workspace: %1",
    };
    trf(text, &[&target.name]).trim_end().to_owned()
}

fn build_entry_row(
    application: Option<&Application>,
    symbol: &str,
    title: &str,
    subtitle: &str,
) -> gtk4::Widget {
    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, ROW_SPACING);
    row.set_margin_start(MARGIN);
    row.set_margin_end(MARGIN);
    match application {
        Some(application) => row.append(&build_application_icon(application)),
        None => {
            let icon = text::symbol(symbol, ICON_SIZE as f64);
            text::set_color(&icon, "colOnLayer1");
            row.append(&Centered::new(&icon));
        }
    }

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_valign(gtk4::Align::Center);
    let title_label = text::styled(title);
    text::set_color(&title_label, "colOnLayer1");
    title_label.set_xalign(0.0);
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title_label.set_max_width_chars(MAX_TITLE_CHARS);
    lines.append(&title_label);
    if !subtitle.is_empty() {
        let subtitle_label = text::styled_sized(subtitle, pixel_size::SMALLER);
        text::set_color(&subtitle_label, "colSubtext");
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        subtitle_label.set_max_width_chars(MAX_TITLE_CHARS);
        lines.append(&subtitle_label);
    }
    row.append(&lines);
    row.upcast()
}

fn build_application_icon(application: &Application) -> gtk4::Image {
    let image = gtk4::Image::new();
    image.set_pixel_size(ICON_SIZE);
    if let Some(display) = gtk4::gdk::Display::default() {
        let theme = gtk4::IconTheme::for_display(&display);
        let icon_name = appicon::guess(&theme, &application.icon);
        let name = if !application.icon.is_empty() && theme.has_icon(&icon_name) {
            icon_name
        } else {
            appicon::guess(&theme, &application.name)
        };
        image.set_icon_name(Some(&name));
    }
    image
}
