use gtk4::glib;
use serde_json::Value;
use std::path::Path;

use crate::core::{config, paths};

pub fn run() {
    let legacy_state = paths::state_home().join("quickshell");
    let copies = [
        (
            legacy_state.join("user/todo.json"),
            paths::state().join("todo.json"),
        ),
        (
            legacy_state.join("states.json"),
            paths::state().join("states.json"),
        ),
        (
            legacy_state.join("user/first_run.txt"),
            paths::state().join("first_run.txt"),
        ),
        (
            paths::state().join("first_run.txt"),
            paths::state().join("defaults_applied.txt"),
        ),
        (
            glib::user_cache_dir().join("quickshell/notifications/notifications.json"),
            paths::cache().join("notifications.json"),
        ),
        (
            glib::user_config_dir().join("illogical-impulse/actions"),
            paths::config().join("actions"),
        ),
        (legacy_state.join("user/generated"), paths::generated()),
    ];
    for (from, to) in copies {
        if !to.exists() {
            copy(&from, &to);
        }
    }
    import_config();
}

fn copy(from: &Path, to: &Path) {
    if from.is_dir() {
        let Ok(entries) = std::fs::read_dir(from) else {
            return;
        };
        let _ = std::fs::create_dir_all(to);
        for entry in entries.flatten() {
            copy(&entry.path(), &to.join(entry.file_name()));
        }
    } else if from.is_file() {
        if let Some(parent) = to.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(from, to);
    }
}

fn import_config() {
    if config::config_path().exists() {
        return;
    }
    let legacy = glib::user_config_dir().join("illogical-impulse/config.json");
    let Some(root) = std::fs::read_to_string(legacy)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return;
    };
    if let Some(text) = to_toml(root) {
        config::write(&text);
    }
}

fn to_toml(root: Value) -> Option<String> {
    toml_edit::ser::to_string_pretty(&without_nulls(root)).ok()
}

fn without_nulls(value: Value) -> Value {
    match value {
        Value::Object(entries) => Value::Object(
            entries
                .into_iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| (key, without_nulls(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(
            items
                .into_iter()
                .filter(|value| !value.is_null())
                .map(without_nulls)
                .collect(),
        ),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_json_config_reads_back_the_same_from_toml() {
        let json = json!({
            "appearance": { "palette": { "accentColor": "", "type": "auto" } },
            "background": { "wallpaperPath": "/w.png", "thumbnailPath": null },
            "bar": { "workspaces": { "shown": 10 }, "screenList": [] },
            "sidebar": { "quickToggles": [{ "size": 2, "type": "network" }, { "size": 1, "type": "bluetooth" }] },
            "light": { "night": { "colorTemperature": 5000, "automatic": false } },
            "terminal": { "harmony": 0.6 },
        });
        let text = to_toml(json.clone()).unwrap();
        let back: Value = toml_edit::de::from_str(&text).unwrap();
        assert_eq!(back, without_nulls(json));
    }
}
