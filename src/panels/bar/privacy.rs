use gtk4::prelude::*;
use gtk4::{gdk, graphene, gsk};
use std::rc::Rc;

use crate::core::scope::Scope;
use crate::panels::notepad::Notepad;
use crate::services::media::deviceusage::{DeviceUsage, Usage};
use crate::ui::theme::{SharedTheme, mix, pixel_size, rounding};
use crate::ui::widgets::display::paint::Paint;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};
use crate::ui::widgets::layout::centered::Centered;

const CHIP_HEIGHT: i32 = 27;
const CHIP_MIN_WIDTH: i32 = 34;
const CHIP_HORIZONTAL_PADDING: i32 = 9;
const CHIP_SPACING: i32 = 4;

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

pub fn build(
    usage: &DeviceUsage,
    theme: &SharedTheme,
    notepad: &Rc<Notepad>,
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
                button.connect_down({
                    let notepad = Rc::downgrade(notepad);
                    move || {
                        if let Some(notepad) = notepad.upgrade() {
                            notepad.toggle();
                        }
                    }
                });
                chips.append(&button);
                (indicator, button)
            })
            .collect();

    scope.keep(notepad.watch({
        let buttons: Vec<_> = buttons
            .iter()
            .map(|(_, button)| button.downgrade())
            .collect();
        move |is_open| {
            for button in buttons.iter().filter_map(|button| button.upgrade()) {
                button.set_toggled(is_open);
            }
        }
    }));

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
            let active = list_active_indicators(&usage.usage());
            for (indicator, button) in &buttons {
                button.set_visible(active.contains(indicator));
            }
            holder.set_visible(!active.is_empty());
        }
    };
    show();
    scope.keep(usage.subscribe(show));

    holder.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::hyprland::screencast::{Kind, Target};
    use crate::services::media::deviceusage::DeviceUse;
    use crate::services::media::privacy::Application;

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
