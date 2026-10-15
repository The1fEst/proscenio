use gtk4::gio;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;

struct Listener {
    id: u64,
    pointer: String,
    relative_ignored_paths: Vec<String>,
    action: Rc<dyn Fn()>,
}

#[derive(Default)]
struct State {
    monitor: Option<gio::FileMonitor>,
    root: Value,
    listeners: Vec<Listener>,
    next_id: u64,
    generation: u64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::default();
}

#[must_use]
pub struct Watch(u64);

impl Drop for Watch {
    fn drop(&mut self) {
        let id = self.0;
        STATE.with_borrow_mut(|state| {
            state.listeners.retain(|listener| listener.id != id);
            if state.listeners.is_empty() {
                state.monitor = None;
            }
        });
    }
}

pub fn config(pointer: &str, action: impl Fn() + 'static) -> Watch {
    config_except(pointer, &[], action)
}

pub fn config_except(pointer: &str, ignored_paths: &[&str], action: impl Fn() + 'static) -> Watch {
    STATE.with_borrow_mut(|state| {
        if state.monitor.is_none() {
            state.root = config::root().unwrap_or(Value::Null);
            state.monitor = monitor();
        }
        state.next_id += 1;
        state.listeners.push(Listener {
            id: state.next_id,
            pointer: pointer.to_owned(),
            relative_ignored_paths: ignored_paths
                .iter()
                .filter_map(|path| path.strip_prefix(pointer))
                .map(str::to_owned)
                .collect(),
            action: Rc::new(action),
        });
        Watch(state.next_id)
    })
}

/// Counts the changes seen so far, for callers that cache what they read from the file.
pub fn generation() -> u64 {
    STATE.with_borrow(|state| state.generation)
}

fn monitor() -> Option<gio::FileMonitor> {
    let monitor = gio::File::for_path(config::config_path())
        .monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        .ok()?;
    monitor.connect_changed(|_, _, _, event| {
        if matches!(
            event,
            gio::FileMonitorEvent::ChangesDoneHint
                | gio::FileMonitorEvent::Created
                | gio::FileMonitorEvent::MovedIn
        ) {
            handle_change();
        }
    });
    Some(monitor)
}

fn handle_change() {
    let Some(root) = config::root() else {
        return;
    };
    let due_actions: Vec<Rc<dyn Fn()>> = STATE.with_borrow_mut(|state| {
        state.generation += 1;
        let previous_root = std::mem::replace(&mut state.root, root);
        state
            .listeners
            .iter()
            .filter(|listener| {
                differs(
                    &previous_root,
                    &state.root,
                    &listener.pointer,
                    &listener.relative_ignored_paths,
                )
            })
            .map(|listener| listener.action.clone())
            .collect()
    });
    for action in due_actions {
        action();
    }
}

fn differs(
    previous: &Value,
    current: &Value,
    pointer: &str,
    relative_ignored_paths: &[String],
) -> bool {
    if relative_ignored_paths.is_empty() {
        return previous.pointer(pointer) != current.pointer(pointer);
    }
    let strip_ignored = |root: &Value| {
        let mut part = root.pointer(pointer).cloned();
        for path in relative_ignored_paths {
            let parent_and_key = path
                .rsplit_once('/')
                .and_then(|(parent, key)| Some((part.as_mut()?.pointer_mut(parent)?, key)));
            if let Some((Value::Object(map), key)) = parent_and_key {
                map.remove(key);
            }
        }
        part
    };
    strip_ignored(previous) != strip_ignored(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_section_sees_changes_below_it_and_nothing_else() {
        let previous = json!({"background": {"parallax": {"zoom": 1.1}}, "bar": {"top": true}});
        let deeper = json!({"background": {"parallax": {"zoom": 1.2}}, "bar": {"top": true}});
        let elsewhere = json!({"background": {"parallax": {"zoom": 1.1}}, "bar": {"top": false}});
        let cases = [
            (&deeper, "/background", true),
            (&deeper, "/background/parallax/zoom", true),
            (&deeper, "/bar", false),
            (&elsewhere, "/background", false),
            (&elsewhere, "/bar/top", true),
            (&previous, "/missing", false),
        ];
        for (current, pointer, should_differ) in cases {
            assert_eq!(
                differs(&previous, current, pointer, &[]),
                should_differ,
                "{pointer}"
            );
        }
    }

    #[test]
    fn an_ignored_key_below_a_section_changes_nothing_for_it() {
        let previous = json!({"bar": {"top": true, "weather": {"city": "a", "enable": true}}});
        let city = json!({"bar": {"top": true, "weather": {"city": "b", "enable": true}}});
        let enable = json!({"bar": {"top": true, "weather": {"city": "a", "enable": false}}});
        let relative_ignored_paths = ["/weather/city".to_owned()];
        assert!(!differs(&previous, &city, "/bar", &relative_ignored_paths));
        assert!(differs(&previous, &enable, "/bar", &relative_ignored_paths));
        assert!(differs(&previous, &city, "/bar", &[]));
    }
}
