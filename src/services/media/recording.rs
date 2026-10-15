use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::dbus::notify::{self, Notification};

const CHECK_INTERVAL: Duration = Duration::from_millis(250);
const CHECK_COUNT: i32 = 20;
const APP_NAME: &str = "Recorder";

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let mut region = None;
    let mut should_record_sound = false;
    let mut should_record_fullscreen = false;
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--region" => {
                let Some(value) = rest.next() else {
                    notify(
                        &tr("Recording cancelled"),
                        &tr("No region specified for --region"),
                    );
                    return glib::ExitCode::FAILURE;
                };
                region = Some(value.clone());
            }
            "--sound" => should_record_sound = true,
            "--fullscreen" => should_record_fullscreen = true,
            _ => {}
        }
    }

    if process::run(&["pgrep", "wf-recorder"]) {
        notify(&tr("Recording Stopped"), &tr("Stopped"));
        process::run(&["pkill", "wf-recorder"]);
        return glib::ExitCode::SUCCESS;
    }

    let folder = crate::core::config::value("/screenRecord/savePath")
        .and_then(|value| {
            value
                .as_str()
                .map(|path| std::path::PathBuf::from(crate::core::paths::expand_home(path)))
        })
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| glib::user_special_dir(glib::UserDirectory::Videos))
        .unwrap_or_else(|| glib::home_dir().join("Videos"));
    let _ = std::fs::create_dir_all(&folder);

    let mut command = Vec::new();
    if should_record_fullscreen {
        command.extend([
            "-o".to_owned(),
            crate::platform::hyprland::hypr::focused_monitor().unwrap_or_default(),
        ]);
    } else {
        let region = match region {
            Some(region) => region,
            None => match process::output(&["slurp"]) {
                Some(region) => region,
                None => {
                    notify(&tr("Recording cancelled"), &tr("Selection was cancelled"));
                    return glib::ExitCode::FAILURE;
                }
            },
        };
        command.extend(["--geometry".to_owned(), region]);
    }

    let name = format!(
        "recording_{}.mp4",
        glib::DateTime::now_local()
            .ok()
            .and_then(|now| now.format("%Y-%m-%d_%H.%M.%S").ok())
            .unwrap_or_default()
    );
    notify(&tr("Starting recording"), &name);
    let mut arguments = vec![
        "wf-recorder".to_owned(),
        "--pixel-format".to_owned(),
        "yuv420p".to_owned(),
        "-f".to_owned(),
        folder.join(&name).to_string_lossy().into_owned(),
        "-t".to_owned(),
    ];
    arguments.extend(command);
    if should_record_sound && let Some(sink) = process::output(&["pactl", "get-default-sink"]) {
        arguments.push(format!("--audio={sink}.monitor"));
    }
    process::run(&arguments);
    glib::ExitCode::SUCCESS
}

fn list_process_ids() -> Vec<u32> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .collect()
}

fn runs_executable(pid: u32, executable: &str) -> bool {
    std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|path| {
        let path = path.to_string_lossy();
        path.strip_suffix(" (deleted)").unwrap_or(&path) == executable
    })
}

struct ProcessScope {
    executable: String,
    environment_marker: Option<String>,
}

impl ProcessScope {
    fn for_shell() -> Self {
        ProcessScope {
            executable: process::executable(),
            environment_marker: None,
        }
    }

    fn contains(&self, pid: u32) -> bool {
        self.environment_marker.as_ref().is_none_or(|marker| {
            std::fs::read(format!("/proc/{pid}/environ")).is_ok_and(|environment| {
                environment
                    .split(|byte| *byte == 0)
                    .any(|entry| entry == marker.as_bytes())
            })
        })
    }
}

fn list_pending_selections(scope: &ProcessScope) -> Vec<u32> {
    list_process_ids()
        .into_iter()
        .filter(|pid| {
            std::fs::read_to_string(format!("/proc/{pid}/comm"))
                .is_ok_and(|comm| comm.trim_end() == "slurp")
                && scope.contains(*pid)
                && process::ancestors(*pid)
                    .get(1)
                    .is_some_and(|parent| runs_executable(*parent, &scope.executable))
        })
        .collect()
}

fn list_pending_records(scope: &ProcessScope) -> Vec<u32> {
    list_process_ids()
        .into_iter()
        .filter(|pid| {
            *pid != std::process::id()
                && runs_executable(*pid, &scope.executable)
                && std::fs::read(format!("/proc/{pid}/cmdline"))
                    .is_ok_and(|cmdline| cmdline.split(|byte| *byte == 0).nth(1) == Some(b"record"))
                && scope.contains(*pid)
        })
        .collect()
}

fn cancel_pending_start(scope: &ProcessScope) {
    let pids: Vec<String> = list_pending_selections(scope)
        .into_iter()
        .chain(list_pending_records(scope))
        .map(|pid| pid.to_string())
        .collect();
    if pids.is_empty() {
        return;
    }
    let mut command = vec!["kill"];
    command.extend(pids.iter().map(String::as_str));
    process::run(&command);
}

fn notify(summary: &str, body: &str) {
    notify::send_blocking(&Notification {
        app: APP_NAME,
        summary,
        body,
        ..Default::default()
    });
}

#[derive(Clone)]
pub struct Recording {
    is_recorder_running: Rc<Cell<bool>>,
    is_start_expected: Rc<Cell<bool>>,
    pending_checks: Rc<Cell<i32>>,
    pub elapsed_seconds: Rc<Cell<u32>>,
    was_active: Rc<Cell<bool>>,
    ticker: Rc<RefCell<Option<glib::SourceId>>>,
    listeners: Rc<Listeners>,
    process_scope: Rc<ProcessScope>,
}

impl Recording {
    pub fn new() -> Self {
        let recording = Recording {
            is_recorder_running: Rc::new(Cell::new(false)),
            is_start_expected: Rc::new(Cell::new(false)),
            pending_checks: Rc::new(Cell::new(0)),
            elapsed_seconds: Rc::new(Cell::new(0)),
            was_active: Rc::new(Cell::new(false)),
            ticker: Rc::new(RefCell::new(None)),
            listeners: Rc::default(),
            process_scope: Rc::new(ProcessScope::for_shell()),
        };
        recording.refresh();
        recording.tick();
        recording
    }

    pub fn is_active(&self) -> bool {
        self.is_recorder_running.get() || self.is_start_expected.get()
    }

    pub fn elapsed_text(&self) -> String {
        let elapsed_seconds = self.elapsed_seconds.get();
        format!("{}:{:02}", elapsed_seconds / 60, elapsed_seconds % 60)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn watch(&self) {
        self.is_start_expected.set(true);
        self.start_checks();
        self.update_activity();
    }

    pub fn stop(&self) {
        if !self.is_active() {
            return;
        }
        if process::is_running("wf-recorder") {
            crate::core::process::launch_subcommand(&["record"]);
        } else {
            cancel_pending_start(&self.process_scope);
        }
        self.is_start_expected.set(false);
        self.is_recorder_running.set(false);
        self.start_checks();
        self.update_activity();
    }

    fn start_checks(&self) {
        let is_checking = self.pending_checks.get() > 0;
        self.pending_checks.set(CHECK_COUNT);
        if is_checking {
            return;
        }
        let recording = self.clone();
        glib::timeout_add_local(CHECK_INTERVAL, move || recording.check_once());
    }

    fn check_once(&self) -> glib::ControlFlow {
        let checks_left = if !list_pending_selections(&self.process_scope).is_empty() {
            CHECK_COUNT
        } else {
            self.pending_checks.get() - 1
        };
        self.pending_checks.set(checks_left);
        if checks_left == 0 {
            self.is_start_expected.set(false);
        }
        self.refresh();
        if checks_left > 0 {
            glib::ControlFlow::Continue
        } else {
            glib::ControlFlow::Break
        }
    }

    fn tick(&self) {
        if let Some(previous) = self.ticker.borrow_mut().take() {
            previous.remove();
        }
        if !self.is_active() {
            return;
        }
        let recording = self.clone();
        let id = glib::timeout_add_local(Duration::from_secs(1), move || {
            recording.advance();
            glib::ControlFlow::Continue
        });
        self.ticker.replace(Some(id));
    }

    fn advance(&self) {
        let was_recorder_running = self.is_recorder_running.get();
        self.probe_recorder();
        if was_recorder_running && self.is_recorder_running.get() {
            self.elapsed_seconds.set(self.elapsed_seconds.get() + 1);
        }
        self.update_activity();
    }

    pub fn check_idle(&self) {
        if !self.is_active() {
            self.refresh();
        }
    }

    fn refresh(&self) {
        self.probe_recorder();
        self.update_activity();
    }

    fn probe_recorder(&self) {
        let is_running = process::is_running("wf-recorder");
        let was_running = self.is_recorder_running.replace(is_running);
        if is_running {
            self.is_start_expected.set(false);
        }
        if is_running && !was_running {
            self.tick();
        }
    }

    fn update_activity(&self) {
        let is_active = self.is_active();
        if self.was_active.replace(is_active) != is_active {
            self.elapsed_seconds.set(0);
            self.tick();
        }
        self.listeners.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MARKER_VARIABLE: &str = "PROSCENIO_TEST_MARKER";

    fn make_marker(test: &str) -> String {
        format!("{}-{test}", std::process::id())
    }

    fn make_scope(marker: &str) -> ProcessScope {
        ProcessScope {
            executable: process::executable(),
            environment_marker: Some(format!("{MARKER_VARIABLE}={marker}")),
        }
    }

    fn expect_start(pending_checks: i32) -> (Recording, Rc<Cell<u32>>, Subscription) {
        expect_start_in(pending_checks, make_scope(&make_marker("idle")))
    }

    fn expect_start_in(
        pending_checks: i32,
        scope: ProcessScope,
    ) -> (Recording, Rc<Cell<u32>>, Subscription) {
        let recording = Recording {
            is_recorder_running: Rc::new(Cell::new(false)),
            is_start_expected: Rc::new(Cell::new(true)),
            pending_checks: Rc::new(Cell::new(pending_checks)),
            elapsed_seconds: Rc::new(Cell::new(0)),
            was_active: Rc::new(Cell::new(true)),
            ticker: Rc::default(),
            listeners: Rc::default(),
            process_scope: Rc::new(scope),
        };
        let announcements = Rc::new(Cell::new(0));
        let subscription = recording.subscribe({
            let announcements = announcements.clone();
            move || announcements.set(announcements.get() + 1)
        });
        (recording, announcements, subscription)
    }

    #[test]
    fn a_second_of_recording_is_announced_once() {
        let (recording, announcements, _subscription) = expect_start(0);
        recording.advance();
        assert_eq!(announcements.get(), 1);
    }

    #[test]
    fn the_clock_waits_for_the_recorder_to_start() {
        let (recording, _announcements, _subscription) = expect_start(0);
        recording.advance();
        assert_eq!(recording.elapsed_text(), "0:00");
    }

    #[test]
    fn the_last_check_for_a_start_is_announced_once() {
        let (recording, announcements, _subscription) = expect_start(1);
        assert_eq!(recording.check_once(), glib::ControlFlow::Break);
        assert_eq!(announcements.get(), 1);
    }

    struct FakeSlurp {
        child: std::process::Child,
        folder: std::path::PathBuf,
    }

    impl FakeSlurp {
        fn start(marker: &str) -> Self {
            let folder = std::env::temp_dir().join(format!("proscenio-slurp-{marker}"));
            let _ = std::fs::remove_dir_all(&folder);
            std::fs::create_dir_all(&folder).unwrap();
            let program = folder.join("slurp");
            std::os::unix::fs::symlink("/usr/bin/sleep", &program).unwrap();
            let child = std::process::Command::new(&program)
                .arg("60")
                .env(MARKER_VARIABLE, marker)
                .spawn()
                .unwrap();
            wait_for_exec(child.id(), marker);
            FakeSlurp { child, folder }
        }
    }

    impl Drop for FakeSlurp {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
            let _ = std::fs::remove_dir_all(&self.folder);
        }
    }

    #[test]
    fn a_start_is_expected_for_as_long_as_the_region_is_picked() {
        let marker = make_marker("long-pick");
        let _slurp = FakeSlurp::start(&marker);
        let (recording, _announcements, _subscription) = expect_start_in(1, make_scope(&marker));
        assert_eq!(recording.check_once(), glib::ControlFlow::Continue);
        assert!(recording.is_active());
    }

    #[test]
    fn stopping_while_the_region_is_picked_ends_the_pick() {
        let marker = make_marker("stopped-pick");
        let mut slurp = FakeSlurp::start(&marker);
        cancel_pending_start(&make_scope(&marker));
        assert_eq!(wait_for_signal(&mut slurp.child), Some(15));
    }

    const EXIT_TIMEOUT: Duration = Duration::from_secs(5);

    fn wait_for_exec(pid: u32, marker: &str) {
        let scope = make_scope(marker);
        let deadline = std::time::Instant::now() + EXIT_TIMEOUT;
        while !scope.contains(pid) {
            assert!(
                std::time::Instant::now() < deadline,
                "the fake did not start within {EXIT_TIMEOUT:?}"
            );
            std::thread::yield_now();
        }
    }

    fn wait_for_signal(child: &mut std::process::Child) -> Option<i32> {
        use std::os::unix::process::ExitStatusExt;
        let deadline = std::time::Instant::now() + EXIT_TIMEOUT;
        while std::time::Instant::now() < deadline {
            if let Some(status) = child.try_wait().unwrap() {
                return status.signal();
            }
            std::thread::yield_now();
        }
        panic!("the child still runs after {EXIT_TIMEOUT:?}");
    }

    #[test]
    #[ignore]
    fn run_fake_record() {
        if std::env::var_os(MARKER_VARIABLE).is_some() {
            let _ = std::io::stdin().read_line(&mut String::new());
        }
    }

    struct FakeRecord {
        child: std::process::Child,
    }

    impl FakeRecord {
        fn start(marker: &str) -> Self {
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "record",
                    "--exact",
                    "--ignored",
                    "services::media::recording::tests::run_fake_record",
                ])
                .env(MARKER_VARIABLE, marker)
                .stdin(std::process::Stdio::piped())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap();
            wait_for_exec(child.id(), marker);
            FakeRecord { child }
        }
    }

    impl Drop for FakeRecord {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn stopping_before_the_recorder_starts_ends_the_pending_record() {
        let marker = make_marker("pending-record");
        let mut record = FakeRecord::start(&marker);
        cancel_pending_start(&make_scope(&marker));
        assert_eq!(wait_for_signal(&mut record.child), Some(15));
    }
}
