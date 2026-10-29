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
    pub(super) is_minimized: bool,
}

pub(super) struct Entry {
    pub(super) id: String,
    pub(super) windows: Vec<Window>,
}

pub(super) struct WindowList {
    pub(super) focused_address: String,
    pub(super) clients: Vec<Value>,
    pub(super) shown_special_workspaces: Option<Vec<String>>,
}

pub(super) fn fetch_windows() -> WindowList {
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
    let shown_special_workspaces = match hypr::json("monitors") {
        Some(Value::Array(monitors)) => Some(
            monitors
                .iter()
                .filter_map(|monitor| monitor.pointer("/specialWorkspace/name")?.as_str())
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .collect(),
        ),
        _ => None,
    };
    WindowList {
        focused_address,
        clients,
        shown_special_workspaces,
    }
}

fn is_minimized(client: &Value, shown_special_workspaces: &[String]) -> bool {
    client.pointer("/workspace/type").and_then(Value::as_str) == Some("special")
        && !client
            .pointer("/workspace/name")
            .and_then(Value::as_str)
            .is_some_and(|name| shown_special_workspaces.iter().any(|shown| shown == name))
}

pub(super) fn collect_entries(
    pinned_ids: &[String],
    ignore_patterns: &[glib::Regex],
    windows: &WindowList,
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

    for client in &windows.clients {
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
        let is_active = address == windows.focused_address;
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
            is_minimized: windows
                .shown_special_workspaces
                .as_deref()
                .is_some_and(|shown| is_minimized(client, shown)),
        });
    }
    entries
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum DotStyle {
    Active,
    Idle,
    Minimized,
}

pub(super) fn choose_dot_styles(windows: &[Window], max_count: usize) -> Vec<DotStyle> {
    let shown_count = windows.iter().filter(|window| !window.is_minimized).count();
    let minimized_count = windows.len() - shown_count;
    let shown_style = if windows
        .iter()
        .any(|window| window.is_active && !window.is_minimized)
    {
        DotStyle::Active
    } else {
        DotStyle::Idle
    };
    let hollow_slots = usize::from(minimized_count > 0);
    let shown_dots = shown_count.min(max_count.saturating_sub(hollow_slots));
    let minimized_dots = minimized_count.min(max_count.saturating_sub(shown_dots));
    std::iter::repeat_n(shown_style, shown_dots)
        .chain(std::iter::repeat_n(DotStyle::Minimized, minimized_dots))
        .collect()
}

pub(super) fn targets(windows: &[Window]) -> Vec<Target> {
    windows
        .iter()
        .map(|window| Target {
            address: window.address.clone(),
            title: window.title.clone(),
            is_minimized: window.is_minimized,
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
            &WindowList {
                focused_address: "0x2".to_owned(),
                clients: vec![
                    client("0x1", "firefox", true),
                    client("0x2", "kitty", true),
                    client("0x3", "steam_app_42", true),
                    client("0x4", "kitty", false),
                ],
                shown_special_workspaces: None,
            },
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

    #[test]
    fn a_window_on_a_special_workspace_that_no_monitor_shows_is_minimized() {
        let client = |address: &str, workspace_type: &str, workspace: &str| {
            serde_json::json!({
                "address": address,
                "class": "kitty",
                "mapped": true,
                "workspace": {"type": workspace_type, "name": workspace},
            })
        };
        let minimized_states = |shown_special_workspaces: Option<Vec<String>>| {
            let entries = collect_entries(
                &[],
                &[],
                &WindowList {
                    focused_address: String::new(),
                    clients: vec![
                        client("0x1", "normal", "1"),
                        client("0x2", "special", "special:minimized"),
                        client("0x3", "special", "special:magic"),
                    ],
                    shown_special_workspaces,
                },
            );
            entries[0]
                .windows
                .iter()
                .map(|window| (window.address.clone(), window.is_minimized))
                .collect::<Vec<_>>()
        };
        assert_eq!(
            minimized_states(Some(vec!["special:magic".to_owned()])),
            [
                ("0x1".to_owned(), false),
                ("0x2".to_owned(), true),
                ("0x3".to_owned(), false)
            ]
        );
        assert!(
            minimized_states(None)
                .iter()
                .all(|(_, is_minimized)| !is_minimized)
        );
    }

    #[test]
    fn minimized_dots_come_last_and_the_dots_beyond_the_limit_are_dropped() {
        let window = |is_active: bool, is_minimized: bool| Window {
            address: String::new(),
            class: String::new(),
            title: String::new(),
            is_active,
            is_minimized,
        };
        let styles = |windows: &[Window]| choose_dot_styles(windows, 3);
        assert_eq!(styles(&[]), []);
        assert_eq!(styles(&[window(false, false)]), [DotStyle::Idle]);
        assert_eq!(
            styles(&[window(false, true), window(true, false)]),
            [DotStyle::Active, DotStyle::Minimized]
        );
        assert_eq!(
            styles(&[
                window(false, true),
                window(false, true),
                window(false, false),
                window(false, false),
            ]),
            [DotStyle::Idle, DotStyle::Idle, DotStyle::Minimized]
        );
        assert_eq!(
            styles(&[
                window(false, false),
                window(false, false),
                window(false, false),
                window(false, true),
            ]),
            [DotStyle::Idle, DotStyle::Idle, DotStyle::Minimized]
        );
        assert_eq!(
            styles(&[window(true, true), window(false, false)]),
            [DotStyle::Idle, DotStyle::Minimized]
        );
    }
}
