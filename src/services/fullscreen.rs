use serde_json::Value;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hypr::{self, Events};
use crate::ui::widgets::coalesce;

const EVENTS: [&str; 16] = [
    "fullscreen",
    "workspace",
    "workspacev2",
    "focusedmon",
    "focusedmonv2",
    "moveworkspace",
    "moveworkspacev2",
    "openwindow",
    "closewindow",
    "movewindow",
    "movewindowv2",
    "monitoradded",
    "monitoraddedv2",
    "monitorremoved",
    "monitorremovedv2",
    "configreloaded",
];

#[derive(Clone)]
pub struct Fullscreen {
    covered: Rc<RefCell<HashSet<String>>>,
    listeners: Rc<Listeners>,
}

impl Fullscreen {
    pub fn new(events: &Events) -> Self {
        let fullscreen = Fullscreen {
            covered: Rc::new(RefCell::new(covered())),
            listeners: Rc::default(),
        };
        let refresh = coalesce(Rc::new({
            let fullscreen = fullscreen.clone();
            move || fullscreen.refresh()
        }));
        events
            .subscribe(move |event, _| {
                if EVENTS.contains(&event) {
                    refresh();
                }
            })
            .forever();
        fullscreen
    }

    pub fn covers(&self, monitor: &str) -> bool {
        self.covered.borrow().contains(monitor)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    fn refresh(&self) {
        let now = covered();
        if *self.covered.borrow() == now {
            return;
        }
        self.covered.replace(now);
        self.listeners.notify();
    }
}

fn covered() -> HashSet<String> {
    let Some(monitors) = hypr::json("monitors").and_then(|value| value.as_array().cloned()) else {
        return HashSet::new();
    };
    let Some(clients) = hypr::json("clients").and_then(|value| value.as_array().cloned()) else {
        return HashSet::new();
    };
    monitors
        .iter()
        .filter_map(|monitor| {
            let name = monitor.get("name").and_then(Value::as_str)?;
            let active = monitor
                .pointer("/activeWorkspace/id")
                .and_then(Value::as_i64)?;
            clients
                .iter()
                .any(|client| {
                    client.get("fullscreen").and_then(Value::as_i64) == Some(2)
                        && client.pointer("/workspace/id").and_then(Value::as_i64) == Some(active)
                })
                .then(|| name.to_owned())
        })
        .collect()
}
