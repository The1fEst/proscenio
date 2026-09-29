use gtk4::glib;
use std::process::Command;

use crate::core::process;

pub const COMMAND: &str = "set-system-locale";
const LOCALE_GEN: &str = "/etc/locale.gen";
const LOCALE_CONF: &str = "/etc/locale.conf";
const CHARSET: &str = "UTF-8";

pub fn of_language(code: &str) -> String {
    let code = match code {
        "he_HE" => "he_IL",
        code => code,
    };
    format!("{code}.{CHARSET}")
}

fn valid(locale: &str) -> bool {
    let Some((name, charset)) = locale.split_once('.') else {
        return false;
    };
    let Some((language, region)) = name.split_once('_') else {
        return false;
    };
    charset == CHARSET
        && (2..=3).contains(&language.len())
        && language.chars().all(|c| c.is_ascii_lowercase())
        && region.len() == 2
        && region.chars().all(|c| c.is_ascii_uppercase())
}

fn is_entry(line: &str, locale: &str) -> bool {
    let fields: Vec<&str> = line
        .trim_start_matches(|c: char| c == '#' || c.is_whitespace())
        .split_whitespace()
        .collect();
    fields == [locale, CHARSET]
}

fn enabled(text: &str, locale: &str) -> bool {
    text.lines()
        .any(|line| !line.trim_start().starts_with('#') && is_entry(line, locale))
}

fn enable(text: &str, locale: &str) -> Option<String> {
    if enabled(text, locale) {
        return None;
    }
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| {
            if !found && is_entry(line, locale) {
                found = true;
                format!("{locale} {CHARSET}")
            } else {
                line.to_owned()
            }
        })
        .collect();
    if !found {
        lines.push(format!("{locale} {CHARSET}"));
    }
    Some(lines.join("\n") + "\n")
}

fn chosen(text: &str) -> Option<&str> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix("LANG="))
        .map(|value| value.trim_matches('"'))
}

fn is_current(locale: &str) -> bool {
    let generated = std::fs::read_to_string(LOCALE_GEN).is_ok_and(|text| enabled(&text, locale));
    let chosen =
        std::fs::read_to_string(LOCALE_CONF).is_ok_and(|text| chosen(&text) == Some(locale));
    generated && chosen
}

pub fn set(locale: &str, done: impl FnOnce() + 'static) {
    if is_current(locale) {
        done();
        return;
    }
    let command = process::quiet(&["pkexec", &process::executable(), COMMAND, locale]);
    glib::spawn_future_local(async move {
        process::finish(command).await;
        done();
    });
}

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let [locale] = arguments else {
        return glib::ExitCode::FAILURE;
    };
    if !valid(locale) {
        return glib::ExitCode::FAILURE;
    }
    let Ok(text) = std::fs::read_to_string(LOCALE_GEN) else {
        return glib::ExitCode::FAILURE;
    };
    if let Some(enabled) = enable(&text, locale) {
        if std::fs::write(LOCALE_GEN, enabled).is_err() || !process::run(&["locale-gen"]) {
            return glib::ExitCode::FAILURE;
        }
    }
    let assignment = format!("LANG={locale}");
    let set = Command::new("localectl")
        .args(["set-locale", &assignment])
        .status()
        .is_ok_and(|status| status.success());
    if set {
        glib::ExitCode::SUCCESS
    } else {
        glib::ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_commented_locale_is_uncommented_in_place() {
        let text = "#en_US.UTF-8 UTF-8  \nen_GB.UTF-8 UTF-8\n#ru_RU.KOI8-R KOI8-R  \n#ru_RU.UTF-8 UTF-8  \n";
        assert_eq!(
            enable(text, "ru_RU.UTF-8").as_deref(),
            Some(
                "#en_US.UTF-8 UTF-8  \nen_GB.UTF-8 UTF-8\n#ru_RU.KOI8-R KOI8-R  \nru_RU.UTF-8 UTF-8\n"
            )
        );
    }

    #[test]
    fn a_missing_locale_is_appended_and_an_enabled_one_is_left_alone() {
        assert_eq!(
            enable("en_US.UTF-8 UTF-8  \n", "ja_JP.UTF-8").as_deref(),
            Some("en_US.UTF-8 UTF-8  \nja_JP.UTF-8 UTF-8\n")
        );
        assert_eq!(enable("en_US.UTF-8 UTF-8  \n", "en_US.UTF-8"), None);
    }

    #[test]
    fn every_catalog_maps_to_a_valid_locale() {
        for code in crate::core::i18n::available() {
            assert!(valid(&of_language(code)), "{code}");
        }
        assert_eq!(of_language("he_HE"), "he_IL.UTF-8");
        assert!(!valid("ru_RU.UTF-8 UTF-8"));
        assert!(!valid("../etc"));
    }

    #[test]
    fn the_chosen_locale_is_read_from_locale_conf() {
        assert_eq!(
            chosen("LANG=\"ru_RU.UTF-8\"\nLC_TIME=C\n"),
            Some("ru_RU.UTF-8")
        );
        assert_eq!(chosen("LC_TIME=C\n"), None);
    }
}
