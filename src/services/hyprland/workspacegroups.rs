use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config;
use crate::core::listeners::Subscription;
use crate::core::watch::{self, Watch};
use crate::platform::hyprland::hypr::{self, Events};
use crate::platform::hyprland::hyprconfig::{self, Area};
use crate::platform::hyprland::{monitorrules, workspacegroups, workspacekeys};
use crate::services::hardware::displays::{order_main_first, read_monitors};

const SIZE_POINTER: &str = "/bar/workspaces/shown";
const MONITOR_EVENTS: [&str; 3] = ["configreloaded", "monitoradded", "monitoraddedv2"];
const WRITE_DELAY: Duration = Duration::from_secs(1);

pub struct WorkspaceGroupWriter {
    write_timer: Cell<Option<glib::SourceId>>,
    is_refocus_pending: Cell<bool>,
    size_watch: RefCell<Option<Watch>>,
    monitor_subscription: RefCell<Option<Subscription>>,
}

impl WorkspaceGroupWriter {
    pub fn new(events: &Events) -> Rc<Self> {
        let writer = Rc::new(WorkspaceGroupWriter {
            write_timer: Cell::new(None),
            is_refocus_pending: Cell::new(false),
            size_watch: RefCell::new(None),
            monitor_subscription: RefCell::new(None),
        });
        let weak = Rc::downgrade(&writer);
        writer.size_watch.replace(Some(watch::config(SIZE_POINTER, {
            let weak = weak.clone();
            move || {
                if let Some(writer) = weak.upgrade() {
                    writer.schedule();
                }
            }
        })));
        writer
            .monitor_subscription
            .replace(Some(events.subscribe(move |event, _| {
                if !MONITOR_EVENTS.contains(&event) {
                    return;
                }
                let Some(writer) = weak.upgrade() else {
                    return;
                };
                if event == "configreloaded" && writer.is_refocus_pending.take() {
                    refocus_displays();
                }
                writer.schedule();
            })));
        writer.schedule();
        writer
    }

    fn schedule(self: &Rc<Self>) {
        if let Some(source) = self.write_timer.take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        self.write_timer
            .set(Some(glib::timeout_add_local_once(WRITE_DELAY, move || {
                if let Some(writer) = weak.upgrade() {
                    writer.write_timer.set(None);
                    if write_groups() {
                        writer.is_refocus_pending.set(true);
                    }
                }
            })));
    }
}

impl Drop for WorkspaceGroupWriter {
    fn drop(&mut self) {
        if let Some(source) = self.write_timer.take() {
            source.remove();
        }
    }
}

fn write_groups() -> bool {
    let Some(monitors) = read_monitors() else {
        return false;
    };
    let outputs = order_main_first(&monitors, &monitorrules::read().primary);
    let size = config::current().workspaces_shown.max(1);
    let mut starts = Vec::new();
    let Some(are_rules_written) =
        write_if_changed(&hyprconfig::path(workspacegroups::AREA), |text| {
            let new_text = workspacegroups::with_groups(text, size, &outputs);
            starts = workspacegroups::find_group_starts(&new_text);
            new_text
        })
    else {
        return false;
    };
    let are_keys_written = write_if_changed(&hyprconfig::path(Area::Binds), |text| {
        workspacekeys::with_keys(text, size, &starts)
    });
    if are_keys_written.is_some() {
        remove_obsolete_size_file();
    }
    if are_rules_written || are_keys_written == Some(true) {
        hypr::request("reload");
    }
    are_rules_written
}

fn remove_obsolete_size_file() {
    let path = hyprconfig::settings_path(workspacegroups::OBSOLETE_SIZE_FILE);
    if let Err(error) = std::fs::remove_file(&path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        eprintln!(
            "workspace groups: {} could not be removed: {error}",
            path.display()
        );
    }
}

fn refocus_displays() {
    let Some(monitors) = read_monitors() else {
        return;
    };
    let Ok(text) = hyprconfig::read_file_for_edit(&hyprconfig::path(workspacegroups::AREA)) else {
        return;
    };
    let active_workspaces: Vec<(String, i64)> = monitors
        .iter()
        .filter(|monitor| !monitor.is_disabled && monitor.mirror_of == "none")
        .map(|monitor| (monitor.name.clone(), monitor.active_workspace_id))
        .collect();
    let misplaced = workspacegroups::find_misplaced_outputs(&text, &active_workspaces);
    if misplaced.is_empty() {
        return;
    }
    let focused = hypr::focused_monitor();
    for (output, workspace) in misplaced {
        hypr::focus_monitor(&output);
        hypr::focus_workspace(workspace);
    }
    if let Some(name) = focused {
        hypr::focus_monitor(&name);
    }
}

fn write_if_changed(path: &Path, edit: impl FnOnce(&str) -> String) -> Option<bool> {
    let result = hyprconfig::read_file_for_edit(path).and_then(|text| {
        let new_text = edit(&text);
        if new_text == text {
            return Ok(false);
        }
        hyprconfig::replace_file(path, &new_text).map(|()| true)
    });
    result
        .inspect_err(|error| {
            eprintln!(
                "workspace groups: {} could not be written: {error}",
                path.display()
            );
        })
        .ok()
}
