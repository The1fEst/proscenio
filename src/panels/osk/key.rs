use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::layouts::{Key, Kind, Shape};
use crate::core::scope::Scope;
use crate::platform::ydotool::{SHIFT_KEYS, Shift, Ydotool};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::text;

const BASE: f64 = 45.0;
const CAPS_WINDOW: Duration = Duration::from_millis(300);

pub struct OskKey {
    pub button: RippleButton,
    label: gtk4::Label,
    key: &'static Key,
    ydotool: Ydotool,
    latched: Cell<bool>,
    caps_started: Cell<bool>,
    can_caps: Cell<bool>,
    caps_timer: RefCell<Option<glib::SourceId>>,
}

impl OskKey {
    pub fn new(
        theme: &SharedTheme,
        key: &'static Key,
        ydotool: &Ydotool,
        scope: &Scope,
    ) -> Rc<Self> {
        let button = RippleButton::new(theme);
        let empty = key.shape == Shape::Empty;
        button.set_look(Look {
            background: if empty {
                |theme| transparentize(theme.colors.col_layer1, 1.0)
            } else {
                |theme| theme.colors.col_layer1
            },
            ..Look::default()
        });
        button.set_radius(rounding::SMALL as f64);
        let (width, height) = multipliers(key.shape);
        button.set_size_request(
            (BASE * width).round() as i32,
            (BASE * height).round() as i32,
        );
        button.set_hexpand(matches!(key.shape, Shape::Space | Shape::Expand));
        button.set_sensitive(!empty);

        let label = match icon(key) {
            Some(name) => text::symbol(name, pixel_size::HUGE as f64),
            None => text::styled_sized(
                key.label,
                if key.shape == Shape::Fn {
                    pixel_size::SMALL
                } else {
                    pixel_size::LARGE
                },
            ),
        };
        button.set_content(&Centred::new(&label), 0, 0);

        let osk_key = Rc::new(OskKey {
            button: button.clone(),
            label,
            key,
            ydotool: ydotool.clone(),
            latched: Cell::new(false),
            caps_started: Cell::new(false),
            can_caps: Cell::new(false),
            caps_timer: RefCell::new(None),
        });
        button.connect_down({
            let osk_key = Rc::downgrade(&osk_key);
            move || {
                if let Some(osk_key) = osk_key.upgrade() {
                    osk_key.down();
                }
            }
        });
        button.connect_release({
            let osk_key = Rc::downgrade(&osk_key);
            move || {
                if let Some(osk_key) = osk_key.upgrade() {
                    osk_key.release();
                }
            }
        });
        scope.keep(ydotool.subscribe({
            let osk_key = Rc::downgrade(&osk_key);
            move || {
                let Some(osk_key) = osk_key.upgrade() else {
                    return;
                };
                if osk_key.is_shift() && osk_key.ydotool.shift() == Shift::Off {
                    osk_key.caps_started.set(false);
                }
                osk_key.show();
            }
        }));
        osk_key.show();
        osk_key
    }

    fn is_shift(&self) -> bool {
        self.key.kind != Kind::Spacer && SHIFT_KEYS.contains(&self.key.code)
    }

    fn toggled(&self) -> bool {
        if self.is_shift() {
            self.ydotool.shift() != Shift::Off
        } else {
            self.latched.get()
        }
    }

    fn show(&self) {
        let toggled = self.toggled();
        self.button.set_toggled(toggled);
        text::set_color(
            &self.label,
            if toggled {
                "m3onPrimary"
            } else {
                "colOnLayer1"
            },
        );
        if icon(self.key).is_some() {
            return;
        }
        let key = self.key;
        let shown = match self.ydotool.shift() {
            Shift::Locked => key.caps.or(key.shift).unwrap_or(key.label),
            Shift::On => key.shift.unwrap_or(key.label),
            Shift::Off => key.label,
        };
        self.label.set_text(shown);
    }

    fn down(&self) {
        self.ydotool.press(self.key.code);
        if self.is_shift() && self.ydotool.shift() == Shift::Off {
            self.ydotool.set_shift(Shift::On);
        }
    }

    fn release(self: &Rc<Self>) {
        if self.key.kind == Kind::Normal {
            self.ydotool.release(self.key.code);
            if self.ydotool.shift() == Shift::On {
                self.ydotool.release_shift_keys();
            }
        } else if self.is_shift() {
            match self.ydotool.shift() {
                Shift::On if !self.caps_started.get() => self.wait_for_caps(),
                Shift::On if self.can_caps.get() => self.ydotool.set_shift(Shift::Locked),
                Shift::On | Shift::Locked => self.ydotool.release_shift_keys(),
                Shift::Off => {}
            }
        } else if self.key.kind == Kind::Modkey {
            self.latched.set(!self.latched.get());
            if !self.latched.get() {
                self.ydotool.release(self.key.code);
            }
            self.show();
        }
    }

    fn wait_for_caps(self: &Rc<Self>) {
        self.caps_started.set(true);
        self.can_caps.set(true);
        let osk_key = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(CAPS_WINDOW, move || {
            if let Some(osk_key) = osk_key.upgrade() {
                osk_key.caps_timer.take();
                osk_key.can_caps.set(false);
            }
        });
        if let Some(previous) = self.caps_timer.replace(Some(timer)) {
            previous.remove();
        }
    }
}

impl Drop for OskKey {
    fn drop(&mut self) {
        if let Some(timer) = self.caps_timer.take() {
            timer.remove();
        }
    }
}

fn icon(key: &Key) -> Option<&'static str> {
    match key.label.to_lowercase().as_str() {
        "backspace" => Some("backspace"),
        "enter" | "return" => Some("subdirectory_arrow_left"),
        _ => None,
    }
}

fn multipliers(shape: Shape) -> (f64, f64) {
    match shape {
        Shape::Fn => (1.0, 0.7),
        Shape::Tab => (1.6, 1.0),
        Shape::Shift => (2.5, 1.0),
        Shape::Control => (1.3, 1.0),
        _ => (1.0, 1.0),
    }
}
