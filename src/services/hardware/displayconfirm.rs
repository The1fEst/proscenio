use gtk4::glib;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprconfig::{self, Area};

pub const COUNTDOWN_SECONDS: u32 = 10;
const CONFIRMED_KEYS: [&str; 8] = [
    "mode",
    "scale",
    "transform",
    "cm",
    "bitdepth",
    "supports_hdr",
    "supports_wide_color",
    "vrr",
];

pub fn needs_confirmation(
    current_pairs: &[(String, String)],
    new_pairs: &[(String, String)],
) -> bool {
    let current_value = |key: &str| {
        current_pairs
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value.as_str())
    };
    new_pairs.iter().any(|(key, value)| match key.as_str() {
        "disabled" => value == "true" && current_value(key) != Some("true"),
        key if CONFIRMED_KEYS.contains(&key) => current_value(key) != Some(value.as_str()),
        _ => false,
    })
}

#[derive(Debug, PartialEq)]
struct PendingChange {
    restore_text: String,
    remaining_seconds: u32,
}

impl PendingChange {
    fn begin(current: Option<PendingChange>, previous_text: String) -> PendingChange {
        PendingChange {
            restore_text: current.map_or(previous_text, |pending| pending.restore_text),
            remaining_seconds: COUNTDOWN_SECONDS,
        }
    }

    fn tick(&mut self) -> bool {
        self.remaining_seconds = self.remaining_seconds.saturating_sub(1);
        self.remaining_seconds == 0
    }
}

thread_local! {
    static SHARED: Rc<DisplayConfirmation> = Rc::new(DisplayConfirmation::default());
}

#[derive(Default)]
pub struct DisplayConfirmation {
    pending: RefCell<Option<PendingChange>>,
    restore_error: RefCell<Option<String>>,
    ticker: RefCell<Option<glib::SourceId>>,
    changed: Listeners,
    reverted: Listeners,
}

impl DisplayConfirmation {
    pub fn shared() -> Rc<Self> {
        SHARED.with(Rc::clone)
    }

    pub fn remaining_seconds(&self) -> Option<u32> {
        self.pending
            .borrow()
            .as_ref()
            .map(|pending| pending.remaining_seconds)
    }

    pub fn restore_error(&self) -> Option<String> {
        self.restore_error.borrow().clone()
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) -> Subscription {
        self.changed.add(listener)
    }

    pub fn connect_reverted(&self, listener: impl Fn() + 'static) -> Subscription {
        self.reverted.add(listener)
    }

    pub fn begin(self: &Rc<Self>, previous_text: String) {
        let current = self.pending.take();
        self.pending
            .replace(Some(PendingChange::begin(current, previous_text)));
        self.restore_error.take();
        self.stop_ticker();
        let confirmation = Rc::downgrade(self);
        let ticker = glib::timeout_add_seconds_local(1, move || {
            confirmation
                .upgrade()
                .map_or(glib::ControlFlow::Break, |confirmation| confirmation.tick())
        });
        self.ticker.replace(Some(ticker));
        self.changed.notify();
    }

    pub fn keep(&self) {
        self.finish();
    }

    pub fn revert(&self) {
        let Some(restore_text) = self
            .pending
            .borrow()
            .as_ref()
            .map(|pending| pending.restore_text.clone())
        else {
            return;
        };
        self.stop_ticker();
        if let Err(error) = hyprconfig::write(Area::Displays, &restore_text) {
            self.restore_error.replace(Some(error.to_string()));
            self.changed.notify();
            return;
        }
        hypr::request("reload");
        self.finish();
        self.reverted.notify();
    }

    fn tick(&self) -> glib::ControlFlow {
        let Some(is_expired) = self.pending.borrow_mut().as_mut().map(PendingChange::tick) else {
            self.ticker.take();
            return glib::ControlFlow::Break;
        };
        if is_expired {
            self.ticker.take();
            self.revert();
            return glib::ControlFlow::Break;
        }
        self.changed.notify();
        glib::ControlFlow::Continue
    }

    fn stop_ticker(&self) {
        if let Some(ticker) = self.ticker.take() {
            ticker.remove();
        }
    }

    fn finish(&self) {
        self.stop_ticker();
        self.pending.take();
        self.restore_error.take();
        self.changed.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn only_changes_that_can_blank_a_display_need_confirmation() {
        let current_pairs = pairs(&[
            ("mode", "2560x1440@144.00"),
            ("position", "0x0"),
            ("scale", "1"),
            ("transform", "0"),
            ("bitdepth", "8"),
            ("cm", "srgb"),
            ("vrr", "-1"),
            ("supports_hdr", "0"),
            ("sdrbrightness", "1"),
        ]);
        for (changes, expected) in [
            (&[("mode", "1920x1080@60.00")][..], true),
            (&[("mode", "2560x1440@120.00")][..], true),
            (&[("scale", "1.25")][..], true),
            (&[("transform", "1")][..], true),
            (&[("cm", "hdr"), ("bitdepth", "10")][..], true),
            (&[("bitdepth", "10")][..], true),
            (&[("supports_hdr", "1")][..], true),
            (&[("supports_wide_color", "1")][..], true),
            (&[("vrr", "1")][..], true),
            (&[("disabled", "true")][..], true),
            (&[("mode", "2560x1440@144.00"), ("scale", "1")][..], false),
            (&[("position", "1920x0")][..], false),
            (&[("mirror", "HDMI-A-1")][..], false),
            (&[("sdrbrightness", "2")][..], false),
            (&[("icc", "/icc/a.icc")][..], false),
            (&[("reserved_area", "{ top = 5 }")][..], false),
            (&[("disabled", "false")][..], false),
        ] {
            assert_eq!(
                needs_confirmation(&current_pairs, &pairs(changes)),
                expected,
                "{changes:?}"
            );
        }
    }

    #[test]
    fn a_display_that_is_already_off_needs_no_confirmation_to_stay_off() {
        let current_pairs = pairs(&[("disabled", "true")]);
        assert!(!needs_confirmation(
            &current_pairs,
            &pairs(&[("disabled", "true")])
        ));
        assert!(needs_confirmation(&[], &pairs(&[("disabled", "true")])));
    }

    #[test]
    fn a_key_with_no_current_value_counts_as_changed() {
        assert!(needs_confirmation(&[], &pairs(&[("scale", "1")])));
    }

    #[test]
    fn a_second_change_keeps_the_first_restore_text_and_restarts_the_countdown() {
        let first = PendingChange::begin(None, "first".to_owned());
        assert_eq!(
            first,
            PendingChange {
                restore_text: "first".to_owned(),
                remaining_seconds: COUNTDOWN_SECONDS,
            }
        );
        let mut pending = first;
        assert!(!pending.tick());
        assert_eq!(pending.remaining_seconds, COUNTDOWN_SECONDS - 1);
        let second = PendingChange::begin(Some(pending), "second".to_owned());
        assert_eq!(
            second,
            PendingChange {
                restore_text: "first".to_owned(),
                remaining_seconds: COUNTDOWN_SECONDS,
            }
        );
    }

    #[test]
    fn the_countdown_expires_after_the_set_number_of_ticks() {
        let mut pending = PendingChange::begin(None, String::new());
        for _ in 1..COUNTDOWN_SECONDS {
            assert!(!pending.tick());
        }
        assert!(pending.tick());
        assert_eq!(pending.remaining_seconds, 0);
        assert!(pending.tick());
    }
}
