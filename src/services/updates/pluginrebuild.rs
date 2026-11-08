use gtk4::{gio, glib};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::process::Stdio;
use std::rc::Rc;
use std::time::Duration;

use super::output::{MERGED_OUTPUT, visible_text, watch_output};
use crate::core::i18n::tr;
use crate::core::process;
use crate::platform::dbus::notify::{self, Notification, Urgency};
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprpm::{
    self, HYPRPM, PluginAction, RUN0, RebuildOutcome, SYSTEM_TOOLS,
};

const APP: &str = "Shell";
const ICON: &str = "system-software-update";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

thread_local! {
    static IS_CHECKING: Cell<bool> = const { Cell::new(false) };
    static IS_CHECK_QUEUED: Cell<bool> = const { Cell::new(false) };
    static ATTEMPTED_ABI_HASH: RefCell<Option<String>> = const { RefCell::new(None) };
    static REPORTED_ABI_HASH: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn check() {
    if IS_CHECKING.replace(true) {
        IS_CHECK_QUEUED.set(true);
        return;
    }
    let attempted_hash = ATTEMPTED_ABI_HASH.with_borrow(Clone::clone);
    glib::spawn_future_local(async move {
        match gio::spawn_blocking(move || hyprpm::check_plugin(attempted_hash.as_deref()))
            .await
            .ok()
            .flatten()
        {
            Some(PluginAction::Load) => {
                let _ = gio::spawn_blocking(|| process::quiet(&[HYPRPM, "reload"]).status()).await;
            }
            Some(PluginAction::Rebuild { abi_hash }) => {
                ATTEMPTED_ABI_HASH.set(Some(abi_hash.clone()));
                if prepare().await && start_update(abi_hash.clone()) {
                    return;
                }
                report_failure(&abi_hash);
            }
            Some(PluginAction::ReportFailure { abi_hash }) => report_failure(&abi_hash),
            None => {}
        }
        end_check();
    });
}

fn end_check() {
    IS_CHECKING.set(false);
    if IS_CHECK_QUEUED.replace(false) {
        check();
    }
}

async fn prepare() -> bool {
    gio::spawn_blocking(|| {
        Path::new(RUN0).exists()
            && hyprpm::prepare_tool_dir(Path::new(SYSTEM_TOOLS), &hyprpm::tool_dir()).is_ok()
    })
    .await
    .unwrap_or(false)
}

#[derive(Default)]
struct Progress {
    id: Cell<u32>,
    line: RefCell<String>,
    is_update_queued: Cell<bool>,
    has_ended: Cell<bool>,
}

impl Progress {
    fn show(self: &Rc<Self>) {
        let summary = tr("Rebuilding the title bars");
        let progress = self.clone();
        notify::send_then(
            &Notification {
                app: APP,
                summary: &summary,
                body: &self.line.borrow(),
                icon: ICON,
                urgency: Urgency::Low,
                replaces_id: self.id.get(),
                ..Default::default()
            },
            move |id| {
                if progress.has_ended.get() {
                    notify::close(id);
                } else {
                    progress.id.set(id);
                }
            },
        );
    }

    fn push_line(self: &Rc<Self>, line: String) {
        self.line.replace(line);
        if self.is_update_queued.replace(true) {
            return;
        }
        let progress = self.clone();
        glib::timeout_add_local_once(PROGRESS_INTERVAL, move || {
            progress.is_update_queued.set(false);
            if progress.id.get() != 0 && !progress.has_ended.get() {
                progress.show();
            }
        });
    }

    fn end(&self) {
        self.has_ended.set(true);
        if self.id.get() != 0 {
            notify::close(self.id.get());
        }
    }
}

fn start_update(abi_hash: String) -> bool {
    let mut command = process::own_scope(
        None,
        &["/usr/bin/sh", "-c", MERGED_OUTPUT, "sh", HYPRPM, "update"],
    );
    command
        .env("PATH", hyprpm::tool_dir())
        .stdout(Stdio::piped());
    let Ok(mut child) = command.spawn() else {
        return false;
    };
    let Some(stdout) = child.stdout.take() else {
        return false;
    };
    let progress = Rc::new(Progress::default());
    progress.line.replace("hyprpm update".to_owned());
    progress.show();
    let on_bytes = process::split_lines({
        let progress = progress.clone();
        move |line: Option<&str>| {
            let Some(line) = line.map(visible_text) else {
                return;
            };
            let line = line.trim();
            if !line.is_empty() {
                progress.push_line(line.to_owned());
            }
        }
    });
    watch_output(stdout, glib::Pid(child.id() as i32), on_bytes, move |_| {
        progress.end();
        glib::spawn_future_local(async move {
            finish(&abi_hash).await;
            end_check();
        });
    });
    true
}

async fn finish(abi_hash: &str) {
    let outcome = gio::spawn_blocking(hyprpm::check_rebuild)
        .await
        .unwrap_or(RebuildOutcome::Failure);
    let body = match outcome {
        RebuildOutcome::Failure => {
            report_failure(abi_hash);
            return;
        }
        RebuildOutcome::Loadable => String::new(),
        RebuildOutcome::LoadableAfterLogin => tr("They load after you log in again."),
    };
    let is_loadable = outcome == RebuildOutcome::Loadable;
    let _ = gio::spawn_blocking(move || {
        if is_loadable {
            let _ = process::quiet(&[HYPRPM, "reload"]).status();
        }
        hypr::request("dismissnotify");
    })
    .await;
    notify::send(&Notification {
        app: APP,
        summary: &tr("Title bars rebuilt"),
        body: &body,
        icon: ICON,
        urgency: Urgency::Low,
        ..Default::default()
    });
}

fn report_failure(abi_hash: &str) {
    let is_reported = REPORTED_ABI_HASH.with_borrow_mut(|reported| {
        reported.replace(abi_hash.to_owned()).as_deref() == Some(abi_hash)
    });
    if is_reported {
        return;
    }
    notify::send(&Notification {
        app: APP,
        summary: &tr("Could not rebuild the title bars"),
        body: &tr("Run hyprpm update in a terminal to rebuild them."),
        icon: ICON,
        ..Default::default()
    });
}
