use serde_json::Value;

use super::REGION_RADIUS;
use crate::platform::hyprland::hypr;

#[derive(Clone)]
pub(super) struct Target {
    pub(super) bounds: [f64; 4],
    pub(super) label: String,
    pub(super) radius: f64,
}

fn intersection_over_union(a: &[f64; 4], b: &[f64; 4]) -> f64 {
    let left = a[0].max(b[0]);
    let top = a[1].max(b[1]);
    let right = (a[0] + a[2]).min(b[0] + b[2]);
    let bottom = (a[1] + a[3]).min(b[1] + b[3]);
    let intersection = (right - left).max(0.0) * (bottom - top).max(0.0);
    let union = a[2] * a[3] + b[2] * b[3] - intersection;
    if union > 0.0 {
        intersection / union
    } else {
        0.0
    }
}

fn pair(value: &Value, key: &str) -> (f64, f64) {
    let list = value.get(key).and_then(Value::as_array);
    let read_number = |index: usize| {
        list.and_then(|list| list.get(index))
            .and_then(Value::as_f64)
            .unwrap_or(0.0)
    };
    (read_number(0), read_number(1))
}

fn has_title_bar(client: &Value) -> bool {
    let text = |key: &str| client.get(key).and_then(Value::as_str).unwrap_or_default();
    let fullscreen = client
        .get("fullscreen")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    client.get("floating").and_then(Value::as_bool) == Some(true)
        && fullscreen < 2
        && (!text("class").is_empty() || !text("title").is_empty())
        && text("class") != crate::APP_ID
}

fn window_area(client: &Value, title_bar_height: Option<i32>) -> ((f64, f64), (f64, f64)) {
    let (position, size) = (pair(client, "at"), pair(client, "size"));
    match title_bar_height {
        Some(height) if has_title_bar(client) => {
            let height = height as f64;
            ((position.0, position.1 - height), (size.0, size.1 + height))
        }
        _ => (position, size),
    }
}

pub(super) fn targets(
    name: &str,
    offset: (f64, f64),
    title_bar_height: Option<i32>,
) -> (Vec<Target>, Vec<Target>) {
    let monitor = hypr::json("monitors")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some(name));
    let active_workspace_id = monitor
        .as_ref()
        .and_then(|entry| entry.pointer("/activeWorkspace/id"))
        .and_then(Value::as_i64)
        .unwrap_or(0) as i32;
    let bounds = |position: (f64, f64), size: (f64, f64)| {
        [position.0 - offset.0, position.1 - offset.1, size.0, size.1]
    };
    let number = |value: &Value, key: &str| value.get(key).and_then(Value::as_f64).unwrap_or(0.0);
    let layers: Vec<Target> = hypr::json("layers")
        .and_then(|value| value.get(name)?.pointer("/levels/2")?.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|layer| {
            let namespace = layer.get("namespace")?.as_str()?.to_owned();
            if [":bar", ":verticalBar", ":dock"]
                .iter()
                .any(|excluded| namespace.contains(excluded))
            {
                return None;
            }
            Some(Target {
                bounds: bounds(
                    (number(layer, "x"), number(layer, "y")),
                    (number(layer, "w"), number(layer, "h")),
                ),
                label: namespace,
                radius: REGION_RADIUS,
            })
        })
        .collect();
    let mut clients: Vec<Value> = hypr::json("clients")
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter(|client| {
            client.pointer("/workspace/id").and_then(Value::as_i64)
                == Some(active_workspace_id as i64)
        })
        .collect();
    clients.sort_by_key(|client| {
        let is_floating = client.get("floating").and_then(Value::as_bool) == Some(true);
        let focus_history_id = client
            .get("focusHistoryID")
            .and_then(Value::as_i64)
            .unwrap_or(i64::MAX);
        (!is_floating, if is_floating { focus_history_id } else { 0 })
    });
    let default_rounding = hypr::option_int("decoration:rounding").unwrap_or(0) as f64;
    let window_rounding = |client: &Value| {
        if client
            .get("fullscreen")
            .and_then(Value::as_i64)
            .unwrap_or(0)
            >= 2
        {
            return 0.0;
        }
        client
            .get("address")
            .and_then(Value::as_str)
            .and_then(|address| hypr::request(&format!("getprop address:{address} rounding")))
            .and_then(|reply| reply.trim().parse::<f64>().ok())
            .unwrap_or(default_rounding)
    };
    let windows = clients
        .iter()
        .map(|client| Target {
            bounds: {
                let (position, size) = window_area(client, title_bar_height);
                bounds(position, size)
            },
            label: client
                .get("class")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            radius: window_rounding(client),
        })
        .filter(|window| {
            layers
                .iter()
                .all(|layer| intersection_over_union(&window.bounds, &layer.bounds) <= 0.0)
        })
        .collect();
    (windows, layers)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn build_client(floating: bool, fullscreen: i64, class: &str, title: &str) -> Value {
        json!({
            "at": [100, 200],
            "size": [800, 600],
            "floating": floating,
            "fullscreen": fullscreen,
            "class": class,
            "title": title,
        })
    }

    #[test]
    fn title_bar_extends_a_window_upward_only_when_it_is_drawn() {
        let bare = ((100.0, 200.0), (800.0, 600.0));
        let with_bar = ((100.0, 160.0), (800.0, 640.0));
        let cases = [
            (
                "floating",
                build_client(true, 0, "foot", "shell"),
                Some(40),
                with_bar,
            ),
            (
                "tiled",
                build_client(false, 0, "foot", "shell"),
                Some(40),
                bare,
            ),
            (
                "fullscreen",
                build_client(true, 2, "foot", "shell"),
                Some(40),
                bare,
            ),
            (
                "maximized",
                build_client(true, 1, "foot", "shell"),
                Some(40),
                with_bar,
            ),
            (
                "class only",
                build_client(true, 0, "foot", ""),
                Some(40),
                with_bar,
            ),
            (
                "title only",
                build_client(true, 0, "", "shell"),
                Some(40),
                with_bar,
            ),
            ("popup", build_client(true, 0, "", ""), Some(40), bare),
            (
                "own window",
                build_client(true, 0, crate::APP_ID, "settings"),
                Some(40),
                bare,
            ),
            (
                "no plugin",
                build_client(true, 0, "foot", "shell"),
                None,
                bare,
            ),
        ];
        for (name, client, title_bar_height, expected) in cases {
            assert_eq!(window_area(&client, title_bar_height), expected, "{name}");
        }
    }
}
