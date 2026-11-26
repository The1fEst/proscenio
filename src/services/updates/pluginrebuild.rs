use gtk4::{gio, glib};
use proscenio_protocol::{
    Decoder, ERROR_PREFIX, Event, Request, SOCKET_VARIABLE, TOKEN_VARIABLE, VERSION, hex,
};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::Path;
use std::process::{ChildStdin, Stdio};
use std::rc::Rc;
use std::time::Duration;

use super::HELPER;
use super::output::{MERGED_OUTPUT, decode_events, exit_code, visible_text, watch_output};
use super::runner::{close_input, send_request};
use crate::core::gsettings;
use crate::core::i18n::{tr, trf};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::dbus::notify::{self, Notification, Urgency};
use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprpm::{self, HYPRPM, PluginAction, RebuildOutcome, SYSTEM_TOOLS};

const APP: &str = "Shell";
const ICON: &str = "system-software-update";
const PROGRESS_INTERVAL: Duration = Duration::from_millis(500);
const UPDATE_COMMAND: [&str; 1] = ["update"];
const ADD_COMMAND: [&str; 2] = ["add", hyprpm::REPOSITORY_URL];
const ENABLE_COMMAND: [&str; 2] = ["enable", hyprpm::PLUGIN_NAME];

#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub enum Status {
    #[default]
    Idle,
    Busy(String),
    Success(String),
    Failure(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum PluginJob {
    Rebuild { abi_hash: String },
    Update,
    Install,
}

impl PluginJob {
    fn progress_summary(&self) -> String {
        match self {
            PluginJob::Install => tr("Installing the title bars"),
            PluginJob::Rebuild { .. } | PluginJob::Update => tr("Rebuilding the title bars"),
        }
    }

    fn success_summary(&self) -> String {
        match self {
            PluginJob::Install => tr("Title bars installed"),
            PluginJob::Rebuild { .. } | PluginJob::Update => tr("Title bars rebuilt"),
        }
    }

    fn failure_summary(&self) -> String {
        match self {
            PluginJob::Install => tr("Could not install the title bars"),
            PluginJob::Rebuild { .. } | PluginJob::Update => tr("Could not rebuild the title bars"),
        }
    }
}

thread_local! {
    static IS_RUNNING: Cell<bool> = const { Cell::new(false) };
    static IS_CHECK_QUEUED: Cell<bool> = const { Cell::new(false) };
    static ATTEMPTED_ABI_HASH: RefCell<Option<String>> = const { RefCell::new(None) };
    static REPORTED_ABI_HASH: RefCell<Option<String>> = const { RefCell::new(None) };
    static STATUS: RefCell<Status> = const { RefCell::new(Status::Idle) };
    static LISTENERS: Listeners = Listeners::default();
}

pub fn is_running() -> bool {
    IS_RUNNING.get()
}

pub fn status() -> Status {
    STATUS.with_borrow(Clone::clone)
}

pub fn subscribe(listener: impl Fn() + 'static) -> Subscription {
    LISTENERS.with(|listeners| listeners.add(listener))
}

fn set_status(status: Status) {
    STATUS.set(status);
    LISTENERS.with(Listeners::notify);
}

fn set_running(is_running: bool) {
    IS_RUNNING.set(is_running);
    LISTENERS.with(Listeners::notify);
}

pub fn check() {
    if IS_RUNNING.get() {
        IS_CHECK_QUEUED.set(true);
        return;
    }
    set_running(true);
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
                start(PluginJob::Rebuild { abi_hash }).await;
                return;
            }
            Some(PluginAction::ReportFailure { abi_hash }) => {
                notify_failure(&PluginJob::Rebuild { abi_hash }, "");
            }
            None => {}
        }
        end_run();
    });
}

pub fn install() {
    launch(PluginJob::Install);
}

pub fn update() {
    launch(PluginJob::Update);
}

fn launch(job: PluginJob) {
    if IS_RUNNING.get() {
        return;
    }
    set_running(true);
    glib::spawn_future_local(start(job));
}

fn end_run() {
    set_running(false);
    if IS_CHECK_QUEUED.replace(false) {
        check();
    }
}

fn plan_commands(job: &PluginJob, has_repository: bool) -> VecDeque<&'static [&'static str]> {
    match job {
        PluginJob::Install if has_repository => {
            VecDeque::from([&UPDATE_COMMAND[..], &ENABLE_COMMAND])
        }
        PluginJob::Install => VecDeque::from([&UPDATE_COMMAND[..], &ADD_COMMAND, &ENABLE_COMMAND]),
        PluginJob::Rebuild { .. } | PluginJob::Update => VecDeque::from([&UPDATE_COMMAND[..]]),
    }
}

async fn start(job: PluginJob) {
    set_status(Status::Busy(job.progress_summary()));
    let has_repository = job == PluginJob::Install
        && gio::spawn_blocking(hyprpm::is_repository_in_hyprpm)
            .await
            .unwrap_or(false);
    let commands = plan_commands(&job, has_repository);
    let result = match prepare().await {
        Ok(()) => start_session(job.clone(), commands),
        Err(reason) => Err(reason),
    };
    if let Err(reason) = result {
        report_failure(&job, &reason);
        end_run();
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

struct Progress {
    summary: String,
    id: Cell<u32>,
    line: RefCell<String>,
    is_update_queued: Cell<bool>,
    has_ended: Cell<bool>,
}

impl Progress {
    fn new(summary: String) -> Self {
        Progress {
            summary,
            id: Cell::new(0),
            line: RefCell::default(),
            is_update_queued: Cell::new(false),
            has_ended: Cell::new(false),
        }
    }

    fn show(self: &Rc<Self>) {
        let body = glib::markup_escape_text(&self.line.borrow());
        let progress = self.clone();
        notify::send_then(
            &Notification {
                app: APP,
                summary: &self.summary,
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
            if progress.has_ended.get() {
                return;
            }
            set_status(Status::Busy(progress.line.borrow().clone()));
            if progress.id.get() != 0 {
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

struct PluginSession {
    input: RefCell<Option<ChildStdin>>,
    session_line: RefCell<String>,
    has_session: Cell<bool>,
    has_failed: Cell<bool>,
    socket: RefCell<String>,
    token: RefCell<Vec<u8>>,
    progress: Rc<Progress>,
    job: PluginJob,
    commands: RefCell<VecDeque<&'static [&'static str]>>,
    running_count: Cell<u32>,
}

impl PluginSession {
    fn end_child(&self) {
        let running_count = self.running_count.get().saturating_sub(1);
        self.running_count.set(running_count);
        if running_count > 0 {
            return;
        }
        self.progress.end();
        let job = self.job.clone();
        let (has_session, has_failed) = (self.has_session.get(), self.has_failed.get());
        let reason = match self.session_line.borrow().as_str() {
            "" if !self.has_session.get() => tr("The authentication was canceled or failed."),
            "" => self.progress.line.borrow().clone(),
            session_line => session_line.to_owned(),
        };
        glib::spawn_future_local(async move {
            finish(&job, &reason, has_session, has_failed).await;
            gsettings::sync_button_layout();
            end_run();
        });
    }
}

fn start_session(
    job: PluginJob,
    commands: VecDeque<&'static [&'static str]>,
) -> Result<(), String> {
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
    let session = Rc::new(PluginSession {
        input: RefCell::new(Some(stdin)),
        session_line: RefCell::default(),
        has_session: Cell::new(false),
        has_failed: Cell::new(false),
        socket: RefCell::default(),
        token: RefCell::default(),
        progress: Rc::new(Progress::new(job.progress_summary())),
        job,
        commands: RefCell::new(commands),
        running_count: Cell::new(1),
    });
    let mut decoder = Decoder::default();
    let on_bytes = {
        let session = session.clone();
        move |bytes: Option<&[u8]>| {
            for message in decode_events(&mut decoder, bytes) {
                match message {
                    Ok(Event::Ready {
                        version,
                        socket,
                        token,
                    }) => {
                        session.has_session.set(true);
                        if version == VERSION {
                            session.socket.replace(socket);
                            session.token.replace(token);
                            start_next_command(&session);
                        } else {
                            session.has_failed.set(true);
                            session.session_line.replace(format!(
                                "{ERROR_PREFIX}{HELPER} speaks protocol {version}, the shell {VERSION}"
                            ));
                            close_input(&session.input);
                        }
                        if session.input.borrow().is_some() {
                            session.progress.show();
                        }
                    }
                    Ok(Event::Line(line)) => {
                        let line = visible_text(&line);
                        if line.starts_with(ERROR_PREFIX) {
                            session.session_line.replace(line.clone());
                        }
                        session.progress.push_line(&line);
                    }
                    _ => {}
                }
            }
        }
    };
    watch_output(stdout, glib::Pid(child.id() as i32), on_bytes, move |_| {
        session.input.take();
        session.end_child();
    });
    Ok(())
}

fn start_next_command(session: &Rc<PluginSession>) {
    if session.input.borrow().is_none() {
        if !session.commands.borrow().is_empty() {
            session.has_failed.set(true);
        }
        return;
    }
    let command = session.commands.borrow_mut().pop_front();
    let result = match command {
        Some(command) => start_command(session, command),
        None => {
            close_input(&session.input);
            return;
        }
    };
    if let Err(error) = result {
        session.has_failed.set(true);
        session
            .session_line
            .replace(format!("{ERROR_PREFIX}{error}"));
        close_input(&session.input);
    }
}

fn start_command(
    session: &Rc<PluginSession>,
    arguments: &'static [&'static str],
) -> Result<(), String> {
    let command_text = format!("hyprpm {}", arguments.join(" "));
    let line: Vec<&str> = ["/usr/bin/sh", "-c", MERGED_OUTPUT, "sh", HYPRPM]
        .into_iter()
        .chain(arguments.iter().copied())
        .collect();
    let mut command = process::own_scope(None, &line);
    command
        .env("PATH", hyprpm::tool_dir())
        .env(SOCKET_VARIABLE, &*session.socket.borrow())
        .env(TOKEN_VARIABLE, hex(&session.token.borrow()))
        .stdout(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("{command_text}: {error}"))?;
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("{command_text}: no output pipe"));
    };
    send_request(&session.input, Request::Allow(child.id()));
    session.session_line.take();
    session.running_count.set(session.running_count.get() + 1);
    session.progress.push_line(&command_text);
    let on_bytes = process::split_lines({
        let progress = session.progress.clone();
        move |line: Option<&str>| {
            if let Some(line) = line {
                progress.push_line(&visible_text(line));
            }
        }
    });
    let session = session.clone();
    watch_output(
        stdout,
        glib::Pid(child.id() as i32),
        on_bytes,
        move |wait_status| {
            let is_success = exit_code(wait_status) == Some(0);
            session.has_failed.set(!is_success);
            if can_continue_after(arguments, is_success) {
                start_next_command(&session);
            } else {
                close_input(&session.input);
            }
            session.end_child();
        },
    );
    Ok(())
}

fn can_continue_after(arguments: &[&str], is_success: bool) -> bool {
    is_success || arguments == UPDATE_COMMAND
}

fn judge_session(has_session: bool, has_failed: bool, outcome: RebuildOutcome) -> RebuildOutcome {
    match outcome {
        _ if !has_session => RebuildOutcome::Failure,
        RebuildOutcome::Loadable if has_failed => RebuildOutcome::Failure,
        outcome => outcome,
    }
}

async fn finish(job: &PluginJob, reason: &str, has_session: bool, has_failed: bool) {
    let outcome = gio::spawn_blocking(hyprpm::check_rebuild)
        .await
        .unwrap_or(RebuildOutcome::Failure);
    let outcome = judge_session(has_session, has_failed, outcome);
    let body = match outcome {
        RebuildOutcome::Failure => {
            report_failure(job, reason);
            return;
        }
        RebuildOutcome::Loadable => String::new(),
        RebuildOutcome::LoadableAfterLogin => {
            tr("Hyprland was updated. The title bars come back after you log in again.")
        }
    };
    let is_loadable = outcome == RebuildOutcome::Loadable;
    let _ = gio::spawn_blocking(move || {
        if is_loadable {
            let _ = process::quiet(&[HYPRPM, "reload"]).status();
        }
        hypr::request("dismissnotify");
    })
    .await;
    let summary = job.success_summary();
    notify::send(&Notification {
        app: APP,
        summary: &summary,
        body: &body,
        icon: ICON,
        urgency: Urgency::Low,
        ..Default::default()
    });
    set_status(Status::Success(if body.is_empty() {
        summary
    } else {
        body
    }));
}

fn report_failure(job: &PluginJob, reason: &str) {
    let reason = reason.trim_start_matches(ERROR_PREFIX);
    set_status(Status::Failure(match reason {
        "" => job.failure_summary(),
        reason => reason.to_owned(),
    }));
    notify_failure(job, reason);
}

fn notify_failure(job: &PluginJob, reason: &str) {
    if let PluginJob::Rebuild { abi_hash } = job {
        let is_reported = REPORTED_ABI_HASH.with_borrow_mut(|reported| {
            reported.replace(abi_hash.clone()).as_deref() == Some(abi_hash.as_str())
        });
        if is_reported {
            return;
        }
    }
    let advice = tr("Try again from the title bar settings.");
    let body = if reason.is_empty() {
        advice
    } else {
        format!("{}\n{advice}", glib::markup_escape_text(reason))
    };
    notify::send(&Notification {
        app: APP,
        summary: &job.failure_summary(),
        body: &body,
        icon: ICON,
        ..Default::default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failed_command_ends_the_session_unless_it_is_the_update() {
        let cases: [(&[&str], bool, bool); 6] = [
            (&UPDATE_COMMAND, true, true),
            (&UPDATE_COMMAND, false, true),
            (&ADD_COMMAND, true, true),
            (&ADD_COMMAND, false, false),
            (&ENABLE_COMMAND, true, true),
            (&ENABLE_COMMAND, false, false),
        ];
        for (arguments, is_success, expected) in cases {
            assert_eq!(
                can_continue_after(arguments, is_success),
                expected,
                "{arguments:?} {is_success}"
            );
        }
    }

    #[test]
    fn a_canceled_session_fails_and_a_failed_command_fails_unless_the_plugin_waits_for_a_login() {
        let cases = [
            (
                true,
                false,
                RebuildOutcome::Loadable,
                RebuildOutcome::Loadable,
            ),
            (
                true,
                true,
                RebuildOutcome::Loadable,
                RebuildOutcome::Failure,
            ),
            (
                true,
                true,
                RebuildOutcome::LoadableAfterLogin,
                RebuildOutcome::LoadableAfterLogin,
            ),
            (
                false,
                false,
                RebuildOutcome::Loadable,
                RebuildOutcome::Failure,
            ),
            (
                false,
                false,
                RebuildOutcome::LoadableAfterLogin,
                RebuildOutcome::Failure,
            ),
            (
                true,
                false,
                RebuildOutcome::Failure,
                RebuildOutcome::Failure,
            ),
        ];
        for (has_session, has_failed, outcome, expected) in cases {
            assert_eq!(
                judge_session(has_session, has_failed, outcome),
                expected,
                "{has_session} {has_failed}"
            );
        }
    }

    #[test]
    fn an_install_adds_the_repository_only_when_hyprpm_lacks_it() {
        let rebuild_job = PluginJob::Rebuild {
            abi_hash: "new".to_owned(),
        };
        let cases: [(&PluginJob, bool, &[&[&str]]); 6] = [
            (
                &PluginJob::Install,
                false,
                &[
                    &["update"],
                    &["add", "https://github.com/The1fEst/proscenio"],
                    &["enable", "proscenio_hyprland"],
                ],
            ),
            (
                &PluginJob::Install,
                true,
                &[&["update"], &["enable", "proscenio_hyprland"]],
            ),
            (&PluginJob::Update, false, &[&["update"]]),
            (&PluginJob::Update, true, &[&["update"]]),
            (&rebuild_job, false, &[&["update"]]),
            (&rebuild_job, true, &[&["update"]]),
        ];
        for (job, has_repository, expected) in cases {
            assert_eq!(
                Vec::from(plan_commands(job, has_repository)),
                expected,
                "{job:?} {has_repository}"
            );
        }
    }
}
