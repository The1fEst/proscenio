use gtk4::prelude::*;
use gtk4::{gdk, graphene, gsk};
use std::cell::Cell;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::i18n::{tr, trf};
use crate::core::scope::Scope;
use crate::platform::system::appicon;
use crate::services::hyprland::screencast::{Kind, Target};
use crate::services::media::deviceusage::{DeviceUsage, Usage};
use crate::services::media::privacy::Application;
use crate::ui::theme::{SharedTheme, mix, pixel_size, rounding};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::overlay::popup;

const CHIP_HEIGHT: i32 = 27;
const CHIP_MIN_WIDTH: i32 = 34;
const CHIP_HORIZONTAL_PADDING: i32 = 9;
const CHIP_SPACING: i32 = 4;
const BADGE_SIZE: i32 = 23;
const BADGE_SYMBOL_SIZE: f64 = 14.0;
const SECTION_SPACING: i32 = 6;
const ICON_SIZE: i32 = 28;
const ROW_SPACING: i32 = 8;
const SECTION_GAP: i32 = 8;
const MAX_TITLE_CHARS: i32 = 34;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Indicator {
    Microphone,
    Screen,
    Camera,
}

impl Indicator {
    pub fn symbol(self) -> &'static str {
        match self {
            Indicator::Microphone => "mic",
            Indicator::Screen => "screen_share",
            Indicator::Camera => "videocam",
        }
    }

    pub fn color(self) -> gdk::RGBA {
        let (red, green, blue) = self.color_bytes();
        gdk::RGBA::new(
            f32::from(red) / 255.0,
            f32::from(green) / 255.0,
            f32::from(blue) / 255.0,
            1.0,
        )
    }

    fn color_bytes(self) -> (u8, u8, u8) {
        match self {
            Indicator::Microphone => (0xD4, 0x6A, 0x00),
            Indicator::Camera => (0x1F, 0x9D, 0x44),
            Indicator::Screen => (0x6D, 0x7C, 0xFD),
        }
    }
}

pub fn list_active_indicators(usage: &Usage) -> Vec<Indicator> {
    [
        (Indicator::Microphone, !usage.microphones.is_empty()),
        (Indicator::Screen, !usage.screens.is_empty()),
        (Indicator::Camera, !usage.cameras.is_empty()),
    ]
    .into_iter()
    .filter(|(_, is_active)| *is_active)
    .map(|(indicator, _)| indicator)
    .collect()
}

fn black() -> gdk::RGBA {
    gdk::RGBA::new(0.0, 0.0, 0.0, 1.0)
}

macro_rules! look_of {
    ($indicator:expr) => {
        Look {
            background: |_| $indicator.color(),
            hover: |_| mix($indicator.color(), black(), 0.88),
            toggled: |_| mix($indicator.color(), black(), 0.88),
            toggled_hover: |_| mix($indicator.color(), black(), 0.88),
            ripple: |_| mix($indicator.color(), black(), 0.8),
            ripple_toggled: |_| mix($indicator.color(), black(), 0.8),
        }
    };
}

fn look_of(indicator: Indicator) -> Look {
    match indicator {
        Indicator::Microphone => look_of!(Indicator::Microphone),
        Indicator::Screen => look_of!(Indicator::Screen),
        Indicator::Camera => look_of!(Indicator::Camera),
    }
}

pub fn build_disc(indicator: Indicator, size: i32) -> Paint {
    let disc = Paint::new(move |snapshot, width, height| {
        let bounds = graphene::Rect::new(0.0, 0.0, width, height);
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(
            bounds,
            width.min(height) / 2.0,
        ));
        snapshot.append_color(&indicator.color(), &bounds);
        snapshot.pop();
    });
    disc.set_size_request(size, size);
    disc
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

pub fn build(
    usage: &DeviceUsage,
    theme: &SharedTheme,
    config: &Config,
    is_vertical: bool,
    scope: &Scope,
) -> gtk4::Widget {
    let orientation = if is_vertical {
        gtk4::Orientation::Vertical
    } else {
        gtk4::Orientation::Horizontal
    };
    let chips = gtk4::Box::new(orientation, CHIP_SPACING);
    let buttons: Vec<(Indicator, RippleButton)> =
        [Indicator::Microphone, Indicator::Screen, Indicator::Camera]
            .into_iter()
            .map(|indicator| {
                let symbol = text::symbol_filled(indicator.symbol(), pixel_size::LARGE as f64, 1.0);
                symbol.add_css_class("privacy-icon");
                let button = RippleButton::new(theme);
                button.set_radius(rounding::FULL as f64);
                button.set_size_request(CHIP_MIN_WIDTH, CHIP_HEIGHT);
                button.set_look(look_of(indicator));
                button.set_content(&Centered::by_ink(&symbol), CHIP_HORIZONTAL_PADDING, 0);
                chips.append(&button);
                (indicator, button)
            })
            .collect();

    let details = popup::column();
    let popup = popup::Popup::new(&chips, popup::Bar::of(config), &details);
    popup.popover().set_autohide(true);
    let is_open = Rc::new(Cell::new(false));
    let set_toggled = {
        let buttons: Vec<RippleButton> = buttons.iter().map(|(_, button)| button.clone()).collect();
        move |is_toggled: bool| {
            for button in &buttons {
                button.set_toggled(is_toggled);
            }
        }
    };
    let set_toggled = Rc::new(set_toggled);
    for (_, button) in &buttons {
        button.connect_down({
            let popup = popup.clone();
            let usage = usage.clone();
            let details = details.clone();
            let is_open = is_open.clone();
            let set_toggled = set_toggled.clone();
            move || {
                let will_open = !is_open.get();
                is_open.set(will_open);
                set_toggled(will_open);
                if will_open {
                    usage.refresh();
                    fill(&details, &usage.usage());
                }
                popup.show(will_open);
            }
        });
    }
    popup.popover().connect_closed({
        let is_open = is_open.clone();
        let set_toggled = set_toggled.clone();
        move |_| {
            is_open.set(false);
            set_toggled(false);
        }
    });

    let holder = Centered::new(&chips);
    if is_vertical {
        holder.set_valign(gtk4::Align::Start);
        holder.set_margin_top(rounding::SCREEN_ROUNDING);
    } else {
        holder.set_halign(gtk4::Align::Start);
        holder.set_valign(gtk4::Align::Center);
        holder.set_margin_start(rounding::SCREEN_ROUNDING);
    }
    let show = {
        let usage = usage.clone();
        let holder = holder.clone();
        move || {
            let current_usage = usage.usage();
            let active = list_active_indicators(&current_usage);
            for (indicator, button) in &buttons {
                button.set_visible(active.contains(indicator));
            }
            holder.set_visible(!active.is_empty());
            if active.is_empty() {
                popup.show(false);
            }
            if popup.popover().is_visible() {
                fill(&details, &current_usage);
            }
        }
    };
    show();
    scope.keep(usage.subscribe(show));

    holder.upcast()
}

fn fill(column: &gtk4::Box, usage: &Usage) {
    while let Some(child) = column.first_child() {
        column.remove(&child);
    }
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

fn append_section(column: &gtk4::Box, indicator: Indicator, title: &str) {
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, SECTION_SPACING);
    header.append(&build_badge(indicator));
    let label = text::styled_sized(title, pixel_size::NORMAL);
    text::set_color(&label, "colOnSurfaceVariant");
    label.set_xalign(0.0);
    header.append(&Centered::new(&label));
    if column.first_child().is_some() {
        header.set_margin_top(SECTION_GAP);
    }
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
    match application {
        Some(application) => row.append(&build_application_icon(application)),
        None => {
            let icon = text::symbol(symbol, ICON_SIZE as f64);
            text::set_color(&icon, "colOnSurfaceVariant");
            row.append(&Centered::new(&icon));
        }
    }

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_valign(gtk4::Align::Center);
    let title_label = text::styled(title);
    text::set_color(&title_label, "colOnSurfaceVariant");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::media::deviceusage::DeviceUse;

    #[test]
    fn each_indicator_has_its_own_fill_color() {
        let cases = [
            (Indicator::Microphone, (0xD4, 0x6A, 0x00)),
            (Indicator::Camera, (0x1F, 0x9D, 0x44)),
            (Indicator::Screen, (0x6D, 0x7C, 0xFD)),
        ];
        for (indicator, expected) in cases {
            assert_eq!(indicator.color_bytes(), expected, "{indicator:?}");
        }
    }

    #[test]
    fn active_indicators_follow_the_order_of_the_chips() {
        let device_use = DeviceUse {
            application: Application::default(),
            device: String::new(),
        };
        let usage = Usage {
            microphones: vec![device_use.clone()],
            cameras: vec![device_use],
            ..Usage::default()
        };
        assert_eq!(
            list_active_indicators(&usage),
            [Indicator::Microphone, Indicator::Camera]
        );
        assert_eq!(list_active_indicators(&Usage::default()), []);
        let screens = Usage {
            screens: vec![Target {
                kind: Kind::Monitor,
                name: "DP-2".to_owned(),
            }],
            ..Usage::default()
        };
        assert_eq!(list_active_indicators(&screens), [Indicator::Screen]);
    }
}
