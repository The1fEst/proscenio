use serde_json::Value;
use std::cell::Cell;
use std::rc::Rc;

use crate::platform::hyprland::hypr::{self, Events};
use crate::services::hyprland::hyprstate::{HyprState, Snapshot};

const OPTION: &str = "cursor:hide_on_key_press";
const FULLSCREEN_MODE: i64 = 2;

#[derive(Default)]
struct Inner {
    is_enabled: Cell<Option<bool>>,
    is_hiding: Cell<Option<bool>>,
    is_fullscreen_focused: Cell<bool>,
    visible_holds: Cell<u32>,
}

#[derive(Clone)]
pub struct CursorHider {
    inner: Rc<Inner>,
}

pub struct CursorVisibleGuard {
    hider: CursorHider,
}

impl CursorHider {
    pub fn new(events: &Events, hypr: &HyprState) -> Self {
        let is_enabled = hypr::option_bool(OPTION);
        let hider = CursorHider {
            inner: Rc::new(Inner {
                is_enabled: Cell::new(is_enabled),
                is_hiding: Cell::new(is_enabled),
                is_fullscreen_focused: Cell::new(is_fullscreen_focused(&hypr.snapshot())),
                ..Inner::default()
            }),
        };
        hider.apply();
        events
            .subscribe({
                let hider = hider.clone();
                move |event, _| {
                    if event == "configreloaded" {
                        hider.reload_option();
                    }
                }
            })
            .detach();
        hypr.subscribe({
            let hider = hider.clone();
            let hypr = hypr.clone();
            move || {
                hider
                    .inner
                    .is_fullscreen_focused
                    .set(is_fullscreen_focused(&hypr.snapshot()));
                hider.apply();
            }
        })
        .detach();
        hider
    }

    pub fn keep_cursor_visible(&self) -> CursorVisibleGuard {
        let inner = &self.inner;
        inner.visible_holds.set(inner.visible_holds.get() + 1);
        self.apply();
        CursorVisibleGuard {
            hider: self.clone(),
        }
    }

    fn reload_option(&self) {
        let Some(is_enabled) = hypr::option_bool(OPTION) else {
            return;
        };
        self.inner.is_enabled.set(Some(is_enabled));
        self.inner.is_hiding.set(Some(is_enabled));
        self.apply();
    }

    fn apply(&self) {
        let inner = &self.inner;
        let Some(is_enabled) = inner.is_enabled.get() else {
            return;
        };
        let should_hide = should_hide_on_key(
            is_enabled,
            inner.is_fullscreen_focused.get(),
            inner.visible_holds.get(),
        );
        if inner.is_hiding.get() == Some(should_hide) {
            return;
        }
        hypr::set_cursor_hides_on_key(should_hide);
        inner.is_hiding.set(Some(should_hide));
    }
}

impl Drop for CursorVisibleGuard {
    fn drop(&mut self) {
        let inner = &self.hider.inner;
        inner.visible_holds.set(inner.visible_holds.get() - 1);
        self.hider.apply();
    }
}

fn should_hide_on_key(is_enabled: bool, is_fullscreen_focused: bool, visible_holds: u32) -> bool {
    is_enabled && !is_fullscreen_focused && visible_holds == 0
}

fn is_fullscreen_focused(snapshot: &Snapshot) -> bool {
    let (Some(monitors), Some(clients)) = (&snapshot.monitors, &snapshot.clients) else {
        return false;
    };
    let Some(monitor) = monitors
        .iter()
        .find(|monitor| monitor.get("focused").and_then(Value::as_bool) == Some(true))
    else {
        return false;
    };
    let monitor_id = monitor.get("id").and_then(Value::as_i64);
    let workspace_id = monitor
        .pointer("/activeWorkspace/id")
        .and_then(Value::as_i64);
    clients.iter().any(|client| {
        client.get("focusHistoryID").and_then(Value::as_i64) == Some(0)
            && client.get("fullscreen").and_then(Value::as_i64) == Some(FULLSCREEN_MODE)
            && client.get("monitor").and_then(Value::as_i64) == monitor_id
            && client.pointer("/workspace/id").and_then(Value::as_i64) == workspace_id
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_cursor_hides_only_when_enabled_and_nothing_keeps_it_visible() {
        for (is_enabled, is_fullscreen_focused, visible_holds, expected) in [
            (true, false, 0, true),
            (false, false, 0, false),
            (true, true, 0, false),
            (true, false, 2, false),
            (false, true, 1, false),
        ] {
            assert_eq!(
                should_hide_on_key(is_enabled, is_fullscreen_focused, visible_holds),
                expected,
                "is_enabled {is_enabled}, fullscreen {is_fullscreen_focused}, holds {visible_holds}"
            );
        }
    }

    #[test]
    fn only_a_fullscreen_window_with_focus_on_the_focused_monitor_counts() {
        let monitors = vec![
            json!({"id": 0, "focused": true, "activeWorkspace": {"id": 1}}),
            json!({"id": 1, "focused": false, "activeWorkspace": {"id": 2}}),
        ];
        let snapshot = |client: Value| Snapshot {
            monitors: Some(monitors.clone()),
            clients: Some(vec![client]),
            ..Snapshot::default()
        };
        let client = |history: i64, fullscreen: i64, monitor: i64, workspace: i64| {
            json!({
                "focusHistoryID": history,
                "fullscreen": fullscreen,
                "monitor": monitor,
                "workspace": {"id": workspace},
            })
        };
        assert!(is_fullscreen_focused(&snapshot(client(0, 2, 0, 1))));
        assert!(!is_fullscreen_focused(&snapshot(client(1, 2, 0, 1))));
        assert!(!is_fullscreen_focused(&snapshot(client(0, 1, 0, 1))));
        assert!(!is_fullscreen_focused(&snapshot(client(0, 2, 1, 2))));
        assert!(!is_fullscreen_focused(&snapshot(client(0, 2, 0, 3))));
        assert!(!is_fullscreen_focused(&Snapshot::default()));
    }
}
