use gtk4::glib;
use serde_json::Value;
use std::rc::Rc;

use crate::core::config;
use crate::panels::settings::content::{Choice, Context, Page, Style};

const TWENTY_FOUR_HOUR: &str = "hh:mm";

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let main = page.section("", "");
    let format = page.subsection(&main, "Time Format", "");
    page.selection(
        &format,
        vec![
            Choice {
                label: "24h".to_owned(),
                icon: "",
                value: Value::from(TWENTY_FOUR_HOUR),
            },
            Choice {
                label: "12h am/pm".to_owned(),
                icon: "",
                value: Value::from("h:mm ap"),
            },
            Choice {
                label: "12h AM/PM".to_owned(),
                icon: "",
                value: Value::from("h:mm AP"),
            },
        ],
        "/time/format",
        Value::from("h:mm AP"),
        |value| {
            let twelve_hour = value.as_str() != Some(TWENTY_FOUR_HOUR);
            switch_hyprlock_clock(twelve_hour);
            config::store_value("/time/format", value);
        },
    );

    let clock = page.section("nest_clock_farsight_analog", "Clock & Calendar");
    let seconds = page.config_switch(&clock, "pace", "Seconds", "/time/secondPrecision", false);
    page.tip(
        &seconds.button,
        "Enable if you want clocks to show seconds accurately",
    );
    let dates = page.subsection(
        &clock,
        "Date formats",
        "Qt date format strings, see https://doc.qt.io/qt-6/qdate.html#toString",
    );
    page.config_text(
        &dates,
        Style::Outlined,
        "Date (e.g. ddd, dd/MM)",
        "/time/dateFormat",
        "ddd d MMM",
    );
    page.config_text(
        &dates,
        Style::Outlined,
        "Short date (e.g. dd/MM)",
        "/time/shortDateFormat",
        "dd/MM",
    );
    page.config_text(
        &dates,
        Style::Outlined,
        "Date with year (e.g. dd/MM/yyyy)",
        "/time/dateWithYearFormat",
        "dd/MM/yyyy",
    );

    let pomodoro = page.section("timer", "Pomodoro");
    let first = page.uniform_row(&pomodoro);
    page.config_spin_multiple(
        &first,
        "target",
        "Focus (min)",
        "/time/pomodoro/focus",
        1500,
        60,
        (1, 180),
        5,
    );
    page.config_spin_multiple(
        &first,
        "coffee",
        "Break (min)",
        "/time/pomodoro/breakTime",
        300,
        60,
        (1, 60),
        1,
    );
    let second = page.uniform_row(&pomodoro);
    page.config_spin_multiple(
        &second,
        "airline_seat_recline_extra",
        "Long break (min)",
        "/time/pomodoro/longBreak",
        900,
        60,
        (1, 120),
        5,
    );
    let (cycles, _) = page.config_spin(
        &second,
        "repeat",
        "Cycles before long break",
        "/time/pomodoro/cyclesBeforeLongBreak",
        4,
        (1, 12),
        1,
    );
    page.tip(&cycles, "Cycles before long break");
    page
}

fn switch_hyprlock_clock(twelve_hour: bool) {
    let path = glib::user_config_dir().join("hypr/hyprlock.conf");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return;
    };
    let (from, to) = if twelve_hour {
        ("TIME", "TIME12")
    } else {
        ("TIME12", "TIME")
    };
    let switched = replace_word_ends(&text, from, to);
    if switched != text {
        let _ = std::fs::write(&path, switched);
    }
}

/// The first `from` on each line that ends at a word boundary becomes `to`, the substitution the QML shell runs.
fn replace_word_ends(text: &str, from: &str, to: &str) -> String {
    let word = |c: char| c.is_alphanumeric() || c == '_';
    text.split_inclusive('\n')
        .map(|line| {
            let found = line
                .match_indices(from)
                .find(|(at, _)| !line[at + from.len()..].chars().next().is_some_and(word));
            match found {
                Some((at, _)) => format!("{}{to}{}", &line[..at], &line[at + from.len()..]),
                None => line.to_owned(),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprlock_clock_words_switch_like_sed() {
        let cases = [
            ("text = $TIME\n", "TIME", "TIME12", "text = $TIME12\n"),
            ("text = $TIME12\n", "TIME", "TIME12", "text = $TIME12\n"),
            ("text = $TIME12\n", "TIME12", "TIME", "text = $TIME\n"),
            (
                "$TIME $TIME\n$TIME",
                "TIME",
                "TIME12",
                "$TIME12 $TIME\n$TIME12",
            ),
            ("TIMES TIME", "TIME", "TIME12", "TIMES TIME12"),
        ];
        for (text, from, to, expected) in cases {
            assert_eq!(replace_word_ends(text, from, to), expected, "{text:?}");
        }
    }
}
