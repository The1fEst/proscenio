use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config;
use crate::core::listeners::Subscription;
use crate::core::watch::{self, Watch};
use crate::platform::hyprland::hypr::{self, Events};
use crate::platform::hyprland::{hyprconfig, monitorrules, workspacegroups};
use crate::services::hardware::displays::{order_main_first, read_monitors};

const SIZE_POINTER: &str = "/bar/workspaces/shown";
const MONITOR_EVENTS: [&str; 3] = ["configreloaded", "monitoradded", "monitoraddedv2"];
const WRITE_DELAY: Duration = Duration::from_secs(1);

pub struct WorkspaceGroupWriter {
    write_timer: Cell<Option<glib::SourceId>>,
    size_watch: RefCell<Option<Watch>>,
    monitor_subscription: RefCell<Option<Subscription>>,
}

impl WorkspaceGroupWriter {
    pub fn new(events: &Events) -> Rc<Self> {
        let writer = Rc::new(WorkspaceGroupWriter {
            write_timer: Cell::new(None),
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
                if MONITOR_EVENTS.contains(&event)
                    && let Some(writer) = weak.upgrade()
                {
                    writer.schedule();
                }
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
                    write_groups();
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

fn write_groups() {
    let Some(monitors) = read_monitors() else {
        return;
    };
    let outputs = order_main_first(&monitors, &monitorrules::read().primary);
    let size = config::current().workspaces_shown.max(1);
    let Some(are_rules_written) =
        write_if_changed(&hyprconfig::path(workspacegroups::AREA), |text| {
            workspacegroups::with_groups(text, size, &outputs)
        })
    else {
        return;
    };
    let size_path = hyprconfig::settings_path(workspacegroups::SIZE_FILE);
    let is_size_written =
        write_if_changed(&size_path, |_| workspacegroups::render_size_file(size)).unwrap_or(false);
    if are_rules_written || is_size_written {
        hypr::request("reload");
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
