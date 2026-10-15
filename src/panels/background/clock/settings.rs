use serde_json::Value;

use crate::core::config;
use crate::panels::bar::clock::tick_period_seconds;

#[derive(Clone, PartialEq)]
pub(super) struct Settings {
    pub(super) enable: bool,
    only_locked: bool,
    pub(super) free: bool,
    pub(super) x: f64,
    pub(super) y: f64,
    style: String,
    style_locked: String,
    pub(super) quote: String,
    pub(super) adaptive: bool,
    pub(super) show_date: bool,
    pub(super) animate: bool,
    pub(super) vertical: bool,
    pub(super) family: String,
    pub(super) weight: f64,
    pub(super) width: f64,
    pub(super) size: f64,
    pub(super) roundness: f64,
    pub(super) center: bool,
    pub(super) locked_text: bool,
    pub(super) blur: bool,
    pub(super) time_format: String,
    pub(super) date_format: String,
    pub(super) parallax: f64,
    pub(super) zoom: f64,
}

impl Settings {
    pub(super) fn read(root: &Value) -> Self {
        let at = |pointer: &str| root.pointer(pointer);
        let text = |pointer: &str, default: &str| {
            at(pointer)
                .and_then(Value::as_str)
                .unwrap_or(default)
                .to_owned()
        };
        let flag =
            |pointer: &str, default: bool| at(pointer).and_then(Value::as_bool).unwrap_or(default);
        let number =
            |pointer: &str, default: f64| at(pointer).and_then(Value::as_f64).unwrap_or(default);
        let clock = "/background/widgets/clock";
        let quote = if flag(&format!("{clock}/quote/enable"), false) {
            text(&format!("{clock}/quote/text"), "")
        } else {
            String::new()
        };
        Settings {
            enable: flag(&format!("{clock}/enable"), true),
            only_locked: flag(&format!("{clock}/showOnlyWhenLocked"), false),
            free: text(&format!("{clock}/placementStrategy"), "random") == "free",
            x: number(&format!("{clock}/x"), 100.0),
            y: number(&format!("{clock}/y"), 100.0),
            style: text(&format!("{clock}/style"), "cookie"),
            style_locked: text(&format!("{clock}/styleLocked"), "cookie"),
            quote,
            adaptive: flag(&format!("{clock}/digital/adaptiveAlignment"), true),
            show_date: flag(&format!("{clock}/digital/showDate"), true),
            animate: flag(&format!("{clock}/digital/animateChange"), true),
            vertical: flag(&format!("{clock}/digital/vertical"), false),
            family: text(&format!("{clock}/digital/font/family"), "Google Sans Flex"),
            weight: number(&format!("{clock}/digital/font/weight"), 350.0),
            width: number(&format!("{clock}/digital/font/width"), 100.0),
            size: number(&format!("{clock}/digital/font/size"), 90.0),
            roundness: number(&format!("{clock}/digital/font/roundness"), 0.0),
            center: flag("/lock/centerClock", true),
            locked_text: flag("/lock/showLockedText", true),
            blur: flag("/lock/blur/enable", true),
            time_format: text("/time/format", config::TIME_FORMAT),
            date_format: text("/time/dateFormat", config::DATE_FORMAT),
            parallax: number("/background/parallax/widgetsFactor", 1.2),
            zoom: number("/background/parallax/workspaceZoom", 1.07),
        }
    }

    pub(super) fn style(&self, is_locked: bool) -> &str {
        if is_locked {
            &self.style_locked
        } else {
            &self.style
        }
    }

    pub(super) fn is_shown(&self, is_locked: bool) -> bool {
        !self.only_locked || is_locked
    }

    pub(super) fn period_seconds(
        &self,
        is_locked: bool,
        uses_second_precision: bool,
    ) -> Option<u64> {
        if !self.enable || !self.is_shown(is_locked) {
            return None;
        }
        if is_locked {
            return Some(1);
        }
        let shows_date = self.show_date && self.style(is_locked) == "digital";
        let date_format = if shows_date {
            self.date_format.as_str()
        } else {
            ""
        };
        Some(tick_period_seconds(
            uses_second_precision,
            &self.time_format,
            date_format,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_ticks_every_second_when_its_shown_date_has_seconds() {
        let settings = |style: &str, shows_date: bool, is_only_locked: bool| {
            Settings::read(&serde_json::json!({
                "time": { "format": "hh:mm", "dateFormat": "dd/MM ss" },
                "background": { "widgets": { "clock": {
                    "style": style,
                    "showOnlyWhenLocked": is_only_locked,
                    "digital": { "showDate": shows_date },
                } } },
            }))
        };
        assert_eq!(
            settings("digital", true, false).period_seconds(false, false),
            Some(1)
        );
        assert_eq!(
            settings("digital", false, false).period_seconds(false, false),
            Some(60)
        );
        assert_eq!(
            settings("cookie", true, false).period_seconds(false, false),
            Some(60)
        );
        assert_eq!(
            settings("digital", false, false).period_seconds(false, true),
            Some(1)
        );
        assert_eq!(
            settings("cookie", false, false).period_seconds(true, false),
            Some(1)
        );
    }

    #[test]
    fn the_clock_does_not_tick_while_it_is_hidden() {
        let settings = |is_enabled: bool, is_only_locked: bool| {
            Settings::read(&serde_json::json!({
                "time": { "format": "hh:mm:ss" },
                "background": { "widgets": { "clock": {
                    "enable": is_enabled,
                    "showOnlyWhenLocked": is_only_locked,
                } } },
            }))
        };
        assert_eq!(settings(false, false).period_seconds(false, true), None);
        assert_eq!(settings(false, false).period_seconds(true, true), None);
        assert_eq!(settings(true, true).period_seconds(false, true), None);
        assert_eq!(settings(true, true).period_seconds(true, false), Some(1));
        assert_eq!(settings(true, false).period_seconds(false, false), Some(1));

        let digital = Settings::read(&serde_json::json!({
            "time": { "format": "hh:mm", "dateFormat": "dd/MM ss" },
            "background": { "widgets": { "clock": {
                "style": "digital",
                "showOnlyWhenLocked": true,
                "digital": { "showDate": true },
            } } },
        }));
        assert_eq!(digital.period_seconds(false, false), None);
    }
}
