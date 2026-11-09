use gtk4::{gio, glib};
use proscenio_protocol::{
    Decoder, ERROR_PREFIX, Event, Request, SOCKET_VARIABLE, TOKEN_VARIABLE, VERSION, hex,
};
use std::cell::{Cell, RefCell};
use std::path::Path;
use std::process::{ChildStdin, Stdio};
use std::rc::Rc;
use std::time::Duration;

use super::HELPER;
use super::output::{MERGED_OUTPUT, decode_events, visible_text, watch_output};
use super::runner::{close_input, send_request};
use crate::core::i18n::{tr, trf};
use crate::core::process;
use crate::platform::dbus::notify::{self, Notification, Urgency};
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprpm::{self, HYPRPM, PluginAction, RebuildOutcome, SYSTEM_TOOLS};

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
                match prepare()
                    .await
                    .and_then(|()| start_session(abi_hash.clone()))
                {
                    Ok(()) => return,
                    Err(reason) => report_failure(&abi_hash, &reason),
                }
            }
            Some(PluginAction::ReportFailure { abi_hash }) => report_failure(&abi_hash, ""),
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

async fn prepare() -> Result<(), String> {
    gio::spawn_blocking(|| {
        let expected_dir = hyprpm::expected_runtime_dir();
        let is_available = process::exists("pkexec")
            && Path::new(HELPER).exists()
            && expected_dir.is_some()
            && std::env::var("XDG_RUNTIME_DIR").ok() == expected_dir;
        if !is_available {
            return Err(trf(
                "A rebuild needs pkexec, %1 and XDG_RUNTIME_DIR set to %2.",
                &[HELPER, expected_dir.as_deref().unwrap_or("/run/user/<uid>")],
            ));
        }
        hyprpm::prepare_tool_dir(
            Path::new(SYSTEM_TOOLS),
            &hyprpm::tool_dir(),
            Path::new(HELPER),
        )
        .map_err(|error| format!("{}: {error}", hyprpm::tool_dir().display()))
    })
    .await
    .unwrap_or_else(|_| {
        Err(format!(
            "{}: preparation stopped",
            hyprpm::tool_dir().display()
        ))
    })
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
        let body = match self.line.borrow().as_str() {
            "" => "hyprpm update".into(),
            line => glib::markup_escape_text(line),
        };
        let progress = self.clone();
        notify::send_then(
            &Notification {
                app: APP,
                summary: &summary,
                body: &body,
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

    fn push_line(self: &Rc<Self>, line: &str) {
        let line = line.trim();
        if line.is_empty() {
            return;
        }
        self.line.replace(line.to_owned());
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

struct Rebuild {
    input: RefCell<Option<ChildStdin>>,
    session_line: RefCell<String>,
    has_session: Cell<bool>,
    progress: Rc<Progress>,
    abi_hash: String,
    running_count: Cell<u32>,
}

impl Rebuild {
    fn end_child(&self) {
        let running_count = self.running_count.get().saturating_sub(1);
        self.running_count.set(running_count);
        if running_count > 0 {
            return;
        }
        self.progress.end();
        let abi_hash = self.abi_hash.clone();
        let reason = match self.session_line.borrow().as_str() {
            "" if !self.has_session.get() => tr("The authentication was canceled or failed."),
            "" => self.progress.line.borrow().clone(),
            session_line => session_line.to_owned(),
        };
        glib::spawn_future_local(async move {
            finish(&abi_hash, &reason).await;
            end_check();
        });
    }
}

fn start_session(abi_hash: String) -> Result<(), String> {
    let mut command = process::own_scope(None, &["pkexec", HELPER, "hyprpm"]);
    command.stdin(Stdio::piped()).stdout(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("pkexec {HELPER} hyprpm: {error}"))?;
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("pkexec {HELPER} hyprpm: no pipes"));
    };
    let rebuild = Rc::new(Rebuild {
        input: RefCell::new(Some(stdin)),
        session_line: RefCell::default(),
        has_session: Cell::new(false),
        progress: Rc::new(Progress::default()),
        abi_hash,
        running_count: Cell::new(1),
    });
    let mut decoder = Decoder::default();
    let on_bytes = {
        let rebuild = rebuild.clone();
        move |bytes: Option<&[u8]>| {
            for message in decode_events(&mut decoder, bytes) {
                match message {
                    Ok(Event::Ready {
                        version,
                        socket,
                        token,
                    }) => {
                        rebuild.has_session.set(true);
                        rebuild.progress.show();
                        let error = if version != VERSION {
                            Some(format!(
                                "{ERROR_PREFIX}{HELPER} speaks protocol {version}, the shell {VERSION}"
                            ))
                        } else {
                            start_update(&rebuild, socket, &token)
                                .err()
                                .map(|error| format!("{ERROR_PREFIX}{error}"))
                        };
                        if let Some(error) = error {
                            rebuild.session_line.replace(error);
                            close_input(&rebuild.input);
                        }
                    }
                    Ok(Event::Line(line)) => {
                        let line = visible_text(&line);
                        if line.starts_with(ERROR_PREFIX) {
                            rebuild.session_line.replace(line.clone());
                        }
                        rebuild.progress.push_line(&line);
                    }
                    _ => {}
                }
            }
        }
    };
    watch_output(stdout, glib::Pid(child.id() as i32), on_bytes, move |_| {
        rebuild.end_child();
    });
    Ok(())
}

fn start_update(rebuild: &Rc<Rebuild>, socket: String, token: &[u8]) -> Result<(), String> {
    let mut command = process::own_scope(
        None,
        &["/usr/bin/sh", "-c", MERGED_OUTPUT, "sh", HYPRPM, "update"],
    );
    command
        .env("PATH", hyprpm::tool_dir())
        .env(SOCKET_VARIABLE, socket)
        .env(TOKEN_VARIABLE, hex(token))
        .stdout(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{HYPRPM} update: {error}"))?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("{HYPRPM} update: no output pipe"));
    };
    send_request(&rebuild.input, Request::Allow(child.id()));
    rebuild.running_count.set(rebuild.running_count.get() + 1);
    let on_bytes = process::split_lines({
        let progress = rebuild.progress.clone();
        move |line: Option<&str>| {
            if let Some(line) = line {
                progress.push_line(&visible_text(line));
            }
        }
    });
    let rebuild = rebuild.clone();
    watch_output(stdout, glib::Pid(child.id() as i32), on_bytes, move |_| {
        close_input(&rebuild.input);
        rebuild.end_child();
    });
    Ok(())
}

async fn finish(abi_hash: &str, reason: &str) {
    let outcome = gio::spawn_blocking(hyprpm::check_rebuild)
        .await
        .unwrap_or(RebuildOutcome::Failure);
    let body = match outcome {
        RebuildOutcome::Failure => {
            report_failure(abi_hash, reason);
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

fn report_failure(abi_hash: &str, reason: &str) {
    let is_reported = REPORTED_ABI_HASH.with_borrow_mut(|reported| {
        reported.replace(abi_hash.to_owned()).as_deref() == Some(abi_hash)
    });
    if is_reported {
        return;
    }
    let advice = tr("Run hyprpm update in a terminal to rebuild them.");
    let body = if reason.is_empty() {
        advice
    } else {
        format!("{}\n{advice}", glib::markup_escape_text(reason))
    };
    notify::send(&Notification {
        app: APP,
        summary: &tr("Could not rebuild the title bars"),
        body: &body,
        icon: ICON,
        ..Default::default()
    });
}
