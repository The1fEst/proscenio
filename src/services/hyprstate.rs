use gtk4::{gio, glib};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hypr::{self, Events};

const EVENTS: [&str; 24] = [
    "workspace",
    "workspacev2",
    "focusedmon",
    "focusedmonv2",
    "createworkspace",
    "createworkspacev2",
    "destroyworkspace",
    "destroyworkspacev2",
    "moveworkspace",
    "moveworkspacev2",
    "activespecial",
    "activespecialv2",
    "openwindow",
    "closewindow",
    "movewindow",
    "movewindowv2",
    "activewindow",
    "activewindowv2",
    "fullscreen",
    "monitoradded",
    "monitoraddedv2",
    "monitorremoved",
    "monitorremovedv2",
    "configreloaded",
];

#[derive(Default)]
pub struct Snapshot {
    pub monitors: Option<Vec<Value>>,
    pub workspaces: Option<Vec<Value>>,
    pub clients: Option<Vec<Value>>,
    pub active_window: Option<String>,
}

impl Snapshot {
    fn fetch() -> Self {
        let list = |command: &str| match hypr::json(command) {
            Some(Value::Array(items)) => Some(items),
            _ => None,
        };
        Snapshot {
            monitors: list("monitors"),
            workspaces: list("workspaces"),
            clients: list("clients"),
            active_window: hypr::json("activewindow").and_then(|window| {
                window
                    .get("address")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            }),
        }
    }
}

#[derive(Default)]
struct Inner {
    snapshot: RefCell<Rc<Snapshot>>,
    fetching: Cell<bool>,
    stale: Cell<bool>,
    listeners: Listeners,
}

#[derive(Clone)]
pub struct HyprState {
    inner: Rc<Inner>,
}

impl HyprState {
    pub fn new(events: &Events) -> Self {
        let state = HyprState {
            inner: Rc::new(Inner {
                snapshot: RefCell::new(Rc::new(Snapshot::fetch())),
                ..Inner::default()
            }),
        };
        events
            .subscribe({
                let state = state.clone();
                move |event, _| {
                    if EVENTS.contains(&event) {
                        state.refresh();
                    }
                }
            })
            .forever();
        state
    }

    pub fn snapshot(&self) -> Rc<Snapshot> {
        self.inner.snapshot.borrow().clone()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.inner.listeners.add(listener)
    }

    fn refresh(&self) {
        if self.inner.fetching.replace(true) {
            self.inner.stale.set(true);
            return;
        }
        let state = self.clone();
        glib::spawn_future_local(async move {
            let fetched = gio::spawn_blocking(Snapshot::fetch).await;
            state.inner.fetching.set(false);
            if let Ok(snapshot) = fetched {
                state.inner.snapshot.replace(Rc::new(snapshot));
                state.inner.listeners.notify();
            }
            if state.inner.stale.replace(false) {
                state.refresh();
            }
        });
    }
}
