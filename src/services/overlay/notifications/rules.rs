use serde_json::{Map, Value};

use super::APP_URGENCY;
use crate::core::config;

const URGENCY_NAMES: [&str; 3] = ["low", "normal", "critical"];

fn parse_urgency(name: &str) -> Option<u8> {
    URGENCY_NAMES
        .iter()
        .position(|candidate| *candidate == name)
        .map(|index| index as u8)
}

fn find_override(rules: &Value, app_name: &str) -> Option<u8> {
    rules.get(app_name)?.as_str().and_then(parse_urgency)
}

fn replace_override(rules: Option<Value>, app_name: &str, urgency: Option<u8>) -> Value {
    let mut entries: Map<String, Value> = rules
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default();
    entries.remove(app_name);
    if let Some(name) = urgency.and_then(|level| URGENCY_NAMES.get(level as usize)) {
        entries.insert(app_name.to_owned(), Value::from(*name));
    }
    if entries.is_empty() {
        Value::Null
    } else {
        Value::Object(entries)
    }
}

pub fn load_override(app_name: &str) -> Option<u8> {
    find_override(&config::value(APP_URGENCY)?, app_name)
}

pub fn store_override(app_name: &str, urgency: Option<u8>) {
    let rules = replace_override(config::value(APP_URGENCY), app_name, urgency);
    config::store_value(APP_URGENCY, rules);
}

pub fn resolve_urgency(sender_urgency: u8, app_override: Option<u8>) -> u8 {
    app_override.unwrap_or(sender_urgency)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_app_override_is_a_known_urgency_name_under_the_app_name() {
        let rules = json!({
            "Chat": "critical",
            "Mail": "low",
            "Clock": "normal",
            "Odd": "urgent",
            "Number": 2,
        });
        let cases = [
            ("Chat", Some(2)),
            ("Mail", Some(0)),
            ("Clock", Some(1)),
            ("Odd", None),
            ("Number", None),
            ("Other", None),
        ];
        for (app_name, expected) in cases {
            assert_eq!(find_override(&rules, app_name), expected, "{app_name}");
        }
        assert_eq!(find_override(&Value::Null, "Chat"), None);
    }

    #[test]
    fn an_override_replaces_the_urgency_the_sender_asks_for() {
        let cases = [
            (1, None, 1),
            (0, None, 0),
            (1, Some(2), 2),
            (2, Some(0), 0),
            (0, Some(1), 1),
        ];
        for (sender_urgency, app_override, expected) in cases {
            assert_eq!(
                resolve_urgency(sender_urgency, app_override),
                expected,
                "{sender_urgency} {app_override:?}"
            );
        }
    }

    #[test]
    fn replacing_an_override_keeps_the_others_and_drops_an_empty_map() {
        let rules = json!({ "Chat": "critical", "Mail": "low" });
        let cases = [
            (
                Some(rules.clone()),
                "Chat",
                Some(1),
                json!({ "Chat": "normal", "Mail": "low" }),
            ),
            (
                Some(rules.clone()),
                "Clock",
                Some(2),
                json!({ "Chat": "critical", "Mail": "low", "Clock": "critical" }),
            ),
            (Some(rules.clone()), "Chat", None, json!({ "Mail": "low" })),
            (Some(json!({ "Chat": "low" })), "Chat", None, Value::Null),
            (None, "Chat", Some(0), json!({ "Chat": "low" })),
            (None, "Chat", None, Value::Null),
            (
                Some(json!("broken")),
                "Chat",
                Some(2),
                json!({ "Chat": "critical" }),
            ),
            (Some(rules), "Chat", Some(7), json!({ "Mail": "low" })),
        ];
        for (current, app_name, urgency, expected) in cases {
            assert_eq!(
                replace_override(current.clone(), app_name, urgency),
                expected,
                "{current:?} {app_name} {urgency:?}"
            );
        }
    }
}
