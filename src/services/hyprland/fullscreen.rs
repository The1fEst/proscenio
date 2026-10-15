use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::services::hyprland::hyprstate::{HyprState, Snapshot};

#[derive(Clone)]
pub struct Fullscreen {
    covered_monitors: Rc<RefCell<HashSet<String>>>,
    listeners: Rc<Listeners>,
}

impl Fullscreen {
    pub fn new(hypr: &HyprState) -> Self {
        let fullscreen = Fullscreen {
            covered_monitors: Rc::new(RefCell::new(find_covered_monitors(&hypr.snapshot()))),
            listeners: Rc::default(),
        };
        hypr.subscribe({
            let fullscreen = fullscreen.clone();
            let hypr = hypr.clone();
            move || fullscreen.refresh(&hypr.snapshot())
        })
        .detach();
        fullscreen
    }

    pub fn covers(&self, monitor: &str) -> bool {
        self.covered_monitors.borrow().contains(monitor)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    fn refresh(&self, snapshot: &Snapshot) {
        let covered_monitors = find_covered_monitors(snapshot);
        if *self.covered_monitors.borrow() == covered_monitors {
            return;
        }
        self.covered_monitors.replace(covered_monitors);
        self.listeners.notify();
    }
}

fn find_covered_monitors(snapshot: &Snapshot) -> HashSet<String> {
    let (Some(monitors), Some(clients)) = (&snapshot.monitors, &snapshot.clients) else {
        return HashSet::new();
    };
    monitors
        .iter()
        .filter_map(|monitor| {
            let name = monitor.get("name").and_then(Value::as_str)?;
            let active_workspace_id = monitor
                .pointer("/activeWorkspace/id")
                .and_then(Value::as_i64)?;
            clients
                .iter()
                .any(|client| {
                    client.get("fullscreen").and_then(Value::as_i64) == Some(2)
                        && client.pointer("/workspace/id").and_then(Value::as_i64)
                            == Some(active_workspace_id)
                })
                .then(|| name.to_owned())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_monitor_is_covered_by_a_real_fullscreen_window_on_its_active_workspace() {
        let snapshot = Snapshot {
            monitors: Some(vec![
                json!({"name": "DP-1", "activeWorkspace": {"id": 1}}),
                json!({"name": "DP-2", "activeWorkspace": {"id": 2}}),
                json!({"name": "HDMI-A-1", "activeWorkspace": {"id": 3}}),
            ]),
            clients: Some(vec![
                json!({"fullscreen": 2, "workspace": {"id": 1}}),
                json!({"fullscreen": 1, "workspace": {"id": 2}}),
                json!({"fullscreen": 2, "workspace": {"id": 4}}),
            ]),
            ..Snapshot::default()
        };
        assert_eq!(
            find_covered_monitors(&snapshot),
            HashSet::from(["DP-1".to_owned()])
        );
        let unread_snapshot = Snapshot {
            clients: None,
            ..snapshot
        };
        assert!(find_covered_monitors(&unread_snapshot).is_empty());
    }
}
