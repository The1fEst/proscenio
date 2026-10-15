use gtk4::glib;
use serde_json::Value;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::core::{config, paths};
use crate::platform::system::locale;

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

#[derive(Default)]
struct Catalog {
    language_code: String,
    texts: HashMap<String, String>,
    plurals: HashMap<String, Vec<String>>,
}

thread_local! {
    static LOADED_CATALOG: RefCell<Option<Catalog>> = const { RefCell::new(None) };
}

pub fn available() -> Vec<&'static str> {
    CATALOGS.iter().map(|(code, _)| *code).collect()
}

pub fn display_name(code: &str) -> String {
    match NATIVE_NAMES
        .iter()
        .find(|(entry_code, _)| *entry_code == code)
    {
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

pub fn language_setting() -> String {
    config::value_str(LANGUAGE).unwrap_or_else(|| AUTO.to_owned())
}

pub fn code() -> String {
    let setting = language_setting();
    if setting != AUTO {
        return setting;
    }
    glib::language_names()
        .iter()
        .map(|name| name.split(['.', '@']).next().unwrap_or_default().to_owned())
        .find(|name| name != "C" && !name.is_empty())
        .unwrap_or_default()
}

fn parse(text: &str) -> Catalog {
    let mut catalog = Catalog::default();
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
        return catalog;
    };
    for (key, value) in map {
        match value {
            Value::String(text) if !text.is_empty() => {
                catalog.texts.insert(key, text);
            }
            Value::Array(forms) => {
                let forms: Option<Vec<String>> = forms
                    .into_iter()
                    .map(|form| match form {
                        Value::String(form) if !form.is_empty() => Some(form),
                        _ => None,
                    })
                    .collect();
                if let Some(forms) = forms.filter(|forms| !forms.is_empty()) {
                    catalog.plurals.insert(key, forms);
                }
            }
            _ => {}
        }
    }
    catalog
}

fn load() -> Catalog {
    let language_code = code();
    let mut catalog = std::fs::read_to_string(
        paths::config()
            .join("translations")
            .join(format!("{language_code}.json")),
    )
    .map(|text| parse(&text))
    .unwrap_or_default();
    if let Some((_, text)) = CATALOGS
        .iter()
        .find(|(catalog_code, _)| *catalog_code == language_code)
    {
        let built_in = parse(text);
        catalog.texts.extend(built_in.texts);
        catalog.plurals.extend(built_in.plurals);
    }
    catalog.language_code = language_code;
    catalog
}

fn strip_keep_suffix(entry: &str) -> &str {
    entry.strip_suffix(KEEP_SUFFIX).map_or(entry, str::trim)
}

pub fn tr(text: &str) -> String {
    LOADED_CATALOG.with_borrow_mut(|catalog| {
        let catalog = catalog.get_or_insert_with(load);
        match catalog.texts.get(text) {
            Some(entry) => strip_keep_suffix(entry).to_owned(),
            None => text.to_owned(),
        }
    })
}

pub fn trn(singular: &str, plural: &str, count: u64) -> String {
    LOADED_CATALOG.with_borrow_mut(|catalog| {
        let catalog = catalog.get_or_insert_with(load);
        let form = match catalog.plurals.get(plural) {
            Some(forms) => {
                let index = plural_index(&catalog.language_code, count);
                forms[index.min(forms.len() - 1)].as_str()
            }
            None => catalog
                .texts
                .get(plural)
                .map_or(if count == 1 { singular } else { plural }, String::as_str),
        };
        strip_keep_suffix(form).replace("%1", &count.to_string())
    })
}

fn plural_index(language_code: &str, count: u64) -> usize {
    match language_code.split('_').next().unwrap_or_default() {
        "id" | "ja" | "tr" | "vi" | "zh" => 0,
        "fr" => usize::from(count > 1),
        "pt" if language_code == "pt_BR" => usize::from(count > 1),
        "ru" | "uk" => match (count % 10, count % 100) {
            (1, ending) if ending != 11 => 0,
            (2..=4, ending) if !(12..=14).contains(&ending) => 1,
            _ => 2,
        },
        _ => usize::from(count != 1),
    }
}

pub fn every_translation<'a>(keys: &HashSet<&'a str>) -> HashMap<&'a str, Vec<String>> {
    let mut translations: HashMap<&'a str, Vec<String>> = HashMap::new();
    for (_, text) in CATALOGS {
        for (key, value) in parse(text).texts {
            if let Some(key) = keys.get(key.as_str()) {
                translations
                    .entry(*key)
                    .or_default()
                    .push(strip_keep_suffix(&value).to_owned());
            }
        }
    }
    translations
}

#[cfg(test)]
pub fn use_language(code: &str) {
    let text = CATALOGS
        .iter()
        .find(|(catalog_code, _)| *catalog_code == code)
        .map_or("{}", |(_, text)| *text);
    LOADED_CATALOG.set(Some(Catalog {
        language_code: code.to_owned(),
        ..parse(text)
    }));
}

pub fn trf(text: &str, arguments: &[&str]) -> String {
    let mut translation = tr(text);
    for (index, argument) in arguments.iter().enumerate().rev() {
        translation = translation.replace(&format!("%{}", index + 1), argument);
    }
    translation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_bundled_catalog_parses_and_has_a_native_name() {
        for (code, text) in CATALOGS {
            assert!(!parse(text).texts.is_empty(), "{code}");
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
                .filter_map(|(index, _)| {
                    let digits: String = text[index + 1..]
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
            let catalog = parse(text);
            assert_eq!(catalog.texts.len(), reference.texts.len(), "{code}");
            assert_eq!(catalog.plurals.len(), reference.plurals.len(), "{code}");
            for key in reference.texts.keys() {
                let value = catalog
                    .texts
                    .get(key)
                    .map(|value| placeholders(strip_keep_suffix(value)));
                assert!(
                    value.is_some_and(|value| value.is_superset(&placeholders(key))),
                    "{code}: {key}"
                );
            }
            let form_count = (0..1000)
                .map(|count| plural_index(code, count))
                .max()
                .unwrap_or_default()
                + 1;
            for key in reference.plurals.keys() {
                let forms = catalog.plurals.get(key);
                assert_eq!(forms.map(Vec::len), Some(form_count), "{code}: {key}");
                assert!(
                    forms
                        .into_iter()
                        .flatten()
                        .all(|form| placeholders(form) == placeholders(key)),
                    "{code}: {key}"
                );
            }
        }
    }

    #[test]
    fn a_count_takes_the_plural_form_its_language_uses() {
        let cases = [
            ("en_US", 0, "0 notifications"),
            ("en_US", 1, "1 notification"),
            ("en_US", 2, "2 notifications"),
            ("fr_FR", 0, "0 notification"),
            ("fr_FR", 2, "2 notifications"),
            ("pt_BR", 0, "0 notificação"),
            ("ru_RU", 1, "1 уведомление"),
            ("ru_RU", 3, "3 уведомления"),
            ("ru_RU", 11, "11 уведомлений"),
            ("ru_RU", 21, "21 уведомление"),
            ("ru_RU", 112, "112 уведомлений"),
            ("uk_UA", 3, "3 сповіщення"),
            ("uk_UA", 5, "5 сповіщень"),
            ("de_DE", 1, "1 Benachrichtigung"),
            ("ja_JP", 1, "1件の通知"),
            ("xx_XX", 1, "1 notification"),
            ("xx_XX", 5, "5 notifications"),
        ];
        for (code, count, text) in cases {
            use_language(code);
            assert_eq!(
                trn("%1 notification", "%1 notifications", count),
                text,
                "{code} {count}"
            );
        }
    }

    #[test]
    fn a_count_string_translated_as_one_text_serves_every_count() {
        LOADED_CATALOG.set(Some(Catalog {
            language_code: "pl_PL".to_owned(),
            ..parse(r#"{"%1 notifications": "Powiadomienia: %1"}"#)
        }));
        for count in [1, 5] {
            assert_eq!(
                trn("%1 notification", "%1 notifications", count),
                format!("Powiadomienia: {count}")
            );
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
