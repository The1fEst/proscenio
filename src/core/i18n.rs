use gtk4::glib;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::core::{config, paths};
use crate::platform::locale;

pub const LANGUAGE: &str = "/language/ui";
pub const AUTO: &str = "auto";
const KEEP_SUFFIX: &str = "/*keep*/";

macro_rules! catalog {
    ($code:literal) => {
        (
            $code,
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/assets/translations/",
                $code,
                ".json"
            )),
        )
    };
}

const CATALOGS: [(&str, &str); 14] = [
    catalog!("de_DE"),
    catalog!("en_US"),
    catalog!("es_MX"),
    catalog!("fr_FR"),
    catalog!("he_HE"),
    catalog!("id_ID"),
    catalog!("it_IT"),
    catalog!("ja_JP"),
    catalog!("pt_BR"),
    catalog!("ru_RU"),
    catalog!("tr_TR"),
    catalog!("uk_UA"),
    catalog!("vi_VN"),
    catalog!("zh_CN"),
];

const NATIVE_NAMES: [(&str, &str); 14] = [
    ("de_DE", "Deutsch"),
    ("en_US", "American English"),
    ("es_MX", "Español de México"),
    ("fr_FR", "Français"),
    ("he_HE", "עברית"),
    ("id_ID", "Indonesia"),
    ("it_IT", "Italiano"),
    ("ja_JP", "日本語"),
    ("pt_BR", "Português"),
    ("ru_RU", "Русский"),
    ("tr_TR", "Türkçe"),
    ("uk_UA", "Українська"),
    ("vi_VN", "Tiếng Việt"),
    ("zh_CN", "中文"),
];

thread_local! {
    static ENTRIES: RefCell<Option<HashMap<String, String>>> = const { RefCell::new(None) };
}

pub fn available() -> Vec<&'static str> {
    CATALOGS.iter().map(|(code, _)| *code).collect()
}

pub fn display_name(code: &str) -> String {
    match NATIVE_NAMES.iter().find(|(known, _)| *known == code) {
        Some((_, name)) => format!("\u{200E}{name} ({code})"),
        None => code.to_owned(),
    }
}

pub fn choose(code: &str, restart: impl FnOnce() + 'static) {
    config::store_value(LANGUAGE, Value::from(code));
    if code == AUTO {
        restart();
        return;
    }
    locale::set(&locale::of_language(code), restart);
}

pub fn chosen() -> String {
    config::value_str(LANGUAGE).unwrap_or_else(|| AUTO.to_owned())
}

pub fn code() -> String {
    let chosen = chosen();
    if chosen != AUTO {
        return chosen;
    }
    glib::language_names()
        .iter()
        .map(|name| name.split(['.', '@']).next().unwrap_or_default().to_owned())
        .find(|name| name != "C" && !name.is_empty())
        .unwrap_or_default()
}

fn parse(text: &str) -> HashMap<String, String> {
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return HashMap::new();
    };
    map.into_iter()
        .filter_map(|(key, value)| Some((key, value.as_str()?.to_owned())))
        .filter(|(_, value)| !value.is_empty())
        .collect()
}

fn load() -> HashMap<String, String> {
    let code = code();
    let mut entries = std::fs::read_to_string(
        paths::config()
            .join("translations")
            .join(format!("{code}.json")),
    )
    .map(|text| parse(&text))
    .unwrap_or_default();
    if let Some((_, text)) = CATALOGS.iter().find(|(known, _)| *known == code) {
        entries.extend(parse(text));
    }
    entries
}

fn shown(found: &str) -> &str {
    found.strip_suffix(KEEP_SUFFIX).map_or(found, str::trim)
}

pub fn tr(text: &str) -> String {
    ENTRIES.with_borrow_mut(|entries| {
        let entries = entries.get_or_insert_with(load);
        match entries.get(text) {
            Some(found) => shown(found).to_owned(),
            None => text.to_owned(),
        }
    })
}

pub fn every_translation<'a>(keys: &HashSet<&'a str>) -> HashMap<&'a str, Vec<String>> {
    let mut found: HashMap<&'a str, Vec<String>> = HashMap::new();
    for (_, text) in CATALOGS {
        for (key, value) in parse(text) {
            if let Some(key) = keys.get(key.as_str()) {
                found
                    .entry(*key)
                    .or_default()
                    .push(shown(&value).to_owned());
            }
        }
    }
    found
}

#[cfg(test)]
pub fn use_language(code: &str) {
    let text = CATALOGS
        .iter()
        .find(|(known, _)| *known == code)
        .map_or("{}", |(_, text)| *text);
    ENTRIES.set(Some(parse(text)));
}

pub fn trf(text: &str, arguments: &[&str]) -> String {
    let mut translated = tr(text);
    for (index, argument) in arguments.iter().enumerate().rev() {
        translated = translated.replace(&format!("%{}", index + 1), argument);
    }
    translated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_catalog_parses_and_has_a_native_name() {
        for (code, text) in CATALOGS {
            assert!(!parse(text).is_empty(), "{code}");
            assert!(
                display_name(code).starts_with(|c: char| !c.is_ascii_punctuation()),
                "{code}"
            );
            assert_ne!(display_name(code), code, "{code}");
        }
    }

    #[test]
    fn every_catalog_translates_every_key_and_keeps_its_placeholders() {
        let placeholders = |text: &str| -> HashSet<String> {
            text.match_indices('%')
                .filter_map(|(at, _)| {
                    let digits: String = text[at + 1..]
                        .chars()
                        .take_while(char::is_ascii_digit)
                        .collect();
                    (!digits.is_empty()).then_some(digits)
                })
                .collect()
        };
        let reference = CATALOGS
            .iter()
            .find(|(code, _)| *code == "en_US")
            .map(|(_, text)| parse(text))
            .unwrap_or_default();
        for (code, text) in CATALOGS {
            let entries = parse(text);
            assert_eq!(entries.len(), reference.len(), "{code}");
            for key in reference.keys() {
                let value = entries.get(key).map(|value| placeholders(shown(value)));
                assert!(
                    value.is_some_and(|value| value.is_superset(&placeholders(key))),
                    "{code}: {key}"
                );
            }
        }
    }

    #[test]
    fn a_right_to_left_name_keeps_its_code_on_the_right() {
        assert_eq!(display_name("he_HE"), "\u{200E}עברית (he_HE)");
    }

    #[test]
    fn numbered_placeholders_are_filled_highest_first() {
        let arguments = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
        assert_eq!(
            trf("proscenio test: %1 of %10 and %2", &arguments),
            "proscenio test: a of j and b"
        );
    }
}
