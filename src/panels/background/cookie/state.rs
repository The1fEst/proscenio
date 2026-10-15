use serde_json::Value;

use crate::ui::anim::{EMPHASIZED, EXPRESSIVE_EFFECTS, Ease, Tween};

const FADE_MILLIS: f64 = 200.0;
const RESIZE_MILLIS: f64 = 300.0;
const HAND_MILLIS: f64 = 300.0;
const SECOND_MILLIS: f64 = 1000.0;
const MAX_SWEEP_SECONDS: i64 = 90;

#[derive(Clone, PartialEq)]
pub struct Look {
    pub sides: i64,
    pub dial: String,
    pub hour_hand: String,
    pub minute_hand: String,
    pub second_hand: String,
    pub date: String,
    pub time_indicators: bool,
    pub hour_marks: bool,
    pub rotate: bool,
    pub sine: bool,
    pub seconds: bool,
}

impl Look {
    pub fn read(root: &Value) -> Self {
        let cookie = |key: &str| root.pointer(&format!("/background/widgets/clock/cookie/{key}"));
        let text = |key: &str, default: &str| {
            cookie(key)
                .and_then(Value::as_str)
                .unwrap_or(default)
                .to_owned()
        };
        let flag =
            |key: &str, default: bool| cookie(key).and_then(Value::as_bool).unwrap_or(default);
        Look {
            sides: cookie("sides").and_then(Value::as_i64).unwrap_or(14),
            dial: text("dialNumberStyle", "full"),
            hour_hand: text("hourHandStyle", "fill"),
            minute_hand: text("minuteHandStyle", "medium"),
            second_hand: text("secondHandStyle", "dot"),
            date: text("dateStyle", "bubble"),
            time_indicators: flag("timeIndicators", true),
            hour_marks: flag("hourMarks", false),
            rotate: flag("constantlyRotate", false),
            sine: flag("useSineCookie", false),
            seconds: root
                .pointer("/time/secondPrecision")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        }
    }

    pub(super) fn shows_hour_marks(&self) -> bool {
        self.hour_marks && (self.dial == "dots" || self.dial == "full")
    }

    pub(super) fn shows_column(&self) -> bool {
        self.time_indicators && self.dial != "numbers"
    }
}

#[derive(Clone, Default, PartialEq)]
pub struct Moment {
    pub local_seconds: i64,
    pub hour: i32,
    pub minute: i32,
    pub second: i32,
    pub numbers: Vec<String>,
    pub day: String,
    pub day_padded: String,
    pub month: String,
    pub weekday_day: String,
}

impl Moment {
    pub(super) fn can_sweep_from(&self, previous: &Moment) -> bool {
        (0..=MAX_SWEEP_SECONDS).contains(&(self.local_seconds - previous.local_seconds))
    }
}

#[derive(Clone, Copy)]
pub struct Fades {
    pub(super) dots: Tween,
    pub(super) numbers: Tween,
    pub(super) lines: Tween,
    pub(super) hour_marks: Tween,
    pub(super) column: Tween,
    pub(super) column_scale_progress: Tween,
    pub(super) column_small: Tween,
    pub(super) minute: Tween,
    pub(super) hour: Tween,
    pub(super) second: Tween,
    pub(super) classic_dot: Tween,
    pub(super) classic_dot_width: Tween,
    pub(super) center: Tween,
    pub(super) border: Tween,
    pub(super) border_radius: Tween,
    pub(super) rect: Tween,
    pub(super) bubble: Tween,
    pub(super) hour_fill: Tween,
    pub(super) hour_classic: Tween,
    pub(super) minute_width: Tween,
    pub(super) minute_classic: Tween,
    pub(super) second_width: Tween,
    pub(super) second_height: Tween,
    pub(super) hour_angle: Tween,
    pub(super) minute_angle: Tween,
    pub(super) second_angle: Tween,
}

impl Default for Fades {
    fn default() -> Self {
        let fade = Tween::new(0.0, FADE_MILLIS, EXPRESSIVE_EFFECTS);
        let resize = Tween::new(0.0, RESIZE_MILLIS, EMPHASIZED);
        let hand = Tween::new(0.0, HAND_MILLIS, EMPHASIZED);
        Fades {
            dots: fade,
            numbers: fade,
            lines: fade,
            hour_marks: fade,
            column: fade,
            column_scale_progress: resize,
            column_small: resize,
            minute: fade,
            hour: fade,
            second: fade,
            classic_dot: fade,
            classic_dot_width: resize,
            center: fade,
            border: fade,
            border_radius: resize,
            rect: fade,
            bubble: fade,
            hour_fill: resize,
            hour_classic: resize,
            minute_width: resize,
            minute_classic: resize,
            second_width: resize,
            second_height: resize,
            hour_angle: hand,
            minute_angle: hand,
            second_angle: Tween::new(0.0, SECOND_MILLIS, Ease::InOutQuad),
        }
    }
}

impl Fades {
    pub(super) fn all(&self) -> [&Tween; 26] {
        [
            &self.dots,
            &self.numbers,
            &self.lines,
            &self.hour_marks,
            &self.column,
            &self.column_scale_progress,
            &self.column_small,
            &self.minute,
            &self.hour,
            &self.second,
            &self.classic_dot,
            &self.classic_dot_width,
            &self.center,
            &self.border,
            &self.border_radius,
            &self.rect,
            &self.bubble,
            &self.hour_fill,
            &self.hour_classic,
            &self.minute_width,
            &self.minute_classic,
            &self.second_width,
            &self.second_height,
            &self.hour_angle,
            &self.minute_angle,
            &self.second_angle,
        ]
    }
}

pub(super) fn steer(tween: &mut Tween, target: f64, now: i64, should_animate: bool) {
    if should_animate {
        tween.retarget(target, now);
    } else {
        tween.jump(target);
    }
}

pub(super) fn steer_clockwise(tween: &mut Tween, target: f64, now: i64, should_animate: bool) {
    let mut target = target;
    let from = tween.target();
    while target < from {
        target += 360.0;
    }
    while target - from >= 360.0 {
        target -= 360.0;
    }
    steer(tween, target, now, should_animate);
}

pub(super) fn progress_of(is_shown: bool) -> f64 {
    if is_shown { 1.0 } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hands_sweep_only_to_a_time_up_to_ninety_seconds_later() {
        let moment = |local_seconds: i64| Moment {
            local_seconds,
            ..Moment::default()
        };
        let start = 1_760_000_000;
        let cases = [
            (moment(start + 1), moment(start), true),
            (moment(start + 61), moment(start), true),
            (moment(start + 90), moment(start), true),
            (moment(start + 91), moment(start), false),
            (moment(start + 12 * 3600 + 30), moment(start), false),
            (moment(start), moment(start + 10), false),
        ];
        for (current, previous, expected) in cases {
            assert_eq!(current.can_sweep_from(&previous), expected);
        }
    }
}
