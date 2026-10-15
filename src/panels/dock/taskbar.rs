use gtk4::glib;
use gtk4::glib::IntoGStr;
use serde_json::Value;

use crate::panels::dock::preview::Target;
use crate::platform::hyprland::hypr;

pub(super) const SEPARATOR: &str = "SEPARATOR";

#[derive(Clone)]
pub(super) struct Window {
    pub(super) address: String,
    pub(super) class: String,
    title: String,
    pub(super) is_active: bool,
}

pub(super) struct Entry {
    pub(super) id: String,
    pub(super) windows: Vec<Window>,
}

pub(super) fn fetch_windows() -> (String, Vec<Value>) {
    let focused_address = hypr::json("activewindow")
        .and_then(|value| {
            value
                .get("address")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();
    let clients = match hypr::json("clients") {
        Some(Value::Array(clients)) => clients,
        _ => Vec::new(),
    };
    (focused_address, clients)
}

pub(super) fn collect_entries(
    pinned_ids: &[String],
    ignore_patterns: &[glib::Regex],
    focused_address: &str,
    clients: &[Value],
) -> Vec<Entry> {
    let mut entries: Vec<Entry> = Vec::new();
    for id in pinned_ids {
        let id = id.to_lowercase();
        if entries.iter().any(|entry| entry.id == id) {
            continue;
        }
        entries.push(Entry {
            id,
            windows: Vec::new(),
        });
    }
    if !pinned_ids.is_empty() {
        entries.push(Entry {
            id: SEPARATOR.to_owned(),
            windows: Vec::new(),
        });
    }

    for client in clients {
        if client.get("mapped").and_then(Value::as_bool) == Some(false) {
            continue;
        }
        let class = client
            .get("class")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        if class.as_str().run_with_gstr(|class| {
            ignore_patterns.iter().any(|pattern| {
                pattern
                    .match_(class, glib::RegexMatchFlags::empty())
                    .is_ok_and(|match_info| match_info.matches())
            })
        }) {
            continue;
        }
        let address = client
            .get("address")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let id = class.to_lowercase();
        let place = match entries.iter().position(|entry| entry.id == id) {
            Some(place) => place,
            None => {
                entries.push(Entry {
                    id: id.clone(),
                    windows: Vec::new(),
                });
                entries.len() - 1
            }
        };
        let is_active = address == focused_address;
        let title = client
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        entries[place].windows.push(Window {
            address,
            class,
            title,
            is_active,
        });
    }
    entries
}

pub(super) fn targets(windows: &[Window]) -> Vec<Target> {
    windows
        .iter()
        .map(|window| Target {
            address: window.address.clone(),
            title: window.title.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_taskbar_groups_mapped_windows_by_class_after_the_pins_and_skips_ignored_ones() {
        let client = |address: &str, class: &str, is_mapped: bool| {
            serde_json::json!({
                "address": address,
                "class": class,
                "title": "",
                "mapped": is_mapped,
            })
        };
        let ignore_patterns = [glib::Regex::new(
            "^Steam_App",
            glib::RegexCompileFlags::CASELESS,
            glib::RegexMatchFlags::empty(),
        )
        .unwrap()
        .unwrap()];
        let entries = collect_entries(
            &["Firefox".to_owned()],
            &ignore_patterns,
            "0x2",
            &[
                client("0x1", "firefox", true),
                client("0x2", "kitty", true),
                client("0x3", "steam_app_42", true),
                client("0x4", "kitty", false),
            ],
        );
        let summary: Vec<(&str, Vec<(&str, bool)>)> = entries
            .iter()
            .map(|entry| {
                (
                    entry.id.as_str(),
                    entry
                        .windows
                        .iter()
                        .map(|window| (window.address.as_str(), window.is_active))
                        .collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("firefox", vec![("0x1", false)]),
                (SEPARATOR, vec![]),
                ("kitty", vec![("0x2", true)]),
            ]
        );
    }
}
