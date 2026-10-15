use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::PathBuf;

use super::Notification;
use super::images::{named_icon, resolve_file_location, resolve_image_location};

impl Notification {
    fn to_json(&self) -> Value {
        json!({
            "notificationId": self.id,
            "actions": self.actions.iter().map(|(identifier, text)| json!({
                "identifier": identifier,
                "text": text,
            })).collect::<Vec<_>>(),
            "appIcon": self.app_icon,
            "appName": self.app_name,
            "body": self.body,
            "image": self.image,
            "summary": self.summary,
            "time": self.time_millis,
            "urgency": self.urgency.to_string(),
        })
    }

    fn from_json(entry: &Value) -> Option<Self> {
        let string = |key: &str| {
            entry
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Some(Notification {
            id: entry.get("notificationId").and_then(Value::as_u64)? as u32,
            actions: Vec::new(),
            reply: None,
            app_icon: string("appIcon"),
            app_name: string("appName"),
            body: string("body"),
            image: string("image"),
            summary: string("summary"),
            time_millis: entry.get("time").and_then(Value::as_i64).unwrap_or(0),
            urgency: string("urgency").parse().unwrap_or(1),
            is_transient: false,
            is_popup: false,
            timeout_millis: 0,
        })
    }
}

fn store_path() -> PathBuf {
    crate::core::paths::cache().join("notifications.json")
}

pub(super) fn read_store() -> Vec<Notification> {
    let mut list: Vec<Notification> = std::fs::read_to_string(store_path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.as_array().cloned())
        .map(|entries| entries.iter().filter_map(Notification::from_json).collect())
        .unwrap_or_default();
    drop_missing_files(&mut list);
    let mut icons: HashMap<String, String> = HashMap::new();
    for notification in list.iter_mut().filter(|entry| entry.app_icon.is_empty()) {
        notification.app_icon = icons
            .entry(notification.app_name.clone())
            .or_insert_with(|| named_icon(&notification.app_name))
            .clone();
    }
    list
}

fn drop_missing_files(list: &mut [Notification]) {
    for notification in list {
        notification.image = resolve_image_location(&notification.image).unwrap_or_default();
        notification.app_icon = resolve_file_location(std::mem::take(&mut notification.app_icon));
    }
}

pub(super) fn write_store(list: &[Notification]) {
    let path = store_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let entries: Vec<Value> = list.iter().map(Notification::to_json).collect();
    if let Ok(text) = serde_json::to_string_pretty(&Value::Array(entries)) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::overlay::notifications::sample_group;

    #[test]
    fn a_restored_notification_drops_files_that_are_gone() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-restored-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let existing_path = folder.join("kept.png").to_string_lossy().into_owned();
        std::fs::write(&existing_path, b"png").unwrap();
        let missing_path = folder.join("gone.png").to_string_lossy().into_owned();
        let mut list = sample_group(&[false, false, false]).notifications;
        (list[0].image, list[0].app_icon) = (existing_path.clone(), missing_path.clone());
        (list[1].image, list[1].app_icon) = (missing_path.clone(), existing_path.clone());
        (list[2].image, list[2].app_icon) = ("audio-x-generic".to_owned(), "My App".to_owned());
        list.push(list[2].clone());
        list[3].image = "~/pic.png".to_owned();
        drop_missing_files(&mut list);
        let pairs: Vec<(&str, &str)> = list
            .iter()
            .map(|entry| (entry.image.as_str(), entry.app_icon.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                (existing_path.as_str(), ""),
                ("", existing_path.as_str()),
                ("audio-x-generic", "My App"),
                ("", "My App")
            ]
        );
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
