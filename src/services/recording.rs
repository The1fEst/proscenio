use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;

const CHECK: Duration = Duration::from_millis(250);
const CHECKS: i32 = 20;
const APP_NAME: &str = "Recorder";

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let mut region = None;
    let mut sound = false;
    let mut fullscreen = false;
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--region" => {
                let Some(value) = rest.next() else {
                    notify("Recording cancelled", "No region specified for --region");
                    return glib::ExitCode::FAILURE;
                };
                region = Some(value.clone());
            }
            "--sound" => sound = true,
            "--fullscreen" => fullscreen = true,
            _ => {}
        }
    }

    if process::run(&["pgrep", "wf-recorder"]) {
        notify("Recording Stopped", "Stopped");
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
    if fullscreen {
        command.extend([
            "-o".to_owned(),
            crate::platform::hypr::focused_monitor().unwrap_or_default(),
        ]);
    } else {
        let region = match region {
            Some(region) => region,
            None => match process::output(&["slurp"]) {
                Some(region) => region,
                None => {
                    notify("Recording cancelled", "Selection was cancelled");
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
    notify("Starting recording", &name);
    let mut arguments = vec![
        "wf-recorder".to_owned(),
        "--pixel-format".to_owned(),
        "yuv420p".to_owned(),
        "-f".to_owned(),
        folder.join(&name).to_string_lossy().into_owned(),
        "-t".to_owned(),
    ];
    arguments.extend(command);
    if sound && let Some(sink) = process::output(&["pactl", "get-default-sink"]) {
        arguments.push(format!("--audio={sink}.monitor"));
    }
    process::run(&arguments);
    glib::ExitCode::SUCCESS
}

fn notify(summary: &str, body: &str) {
    process::detach(&["notify-send", summary, body, "-a", APP_NAME]);
}

#[derive(Clone)]
pub struct Recording {
    seen: Rc<Cell<bool>>,
    expecting: Rc<Cell<bool>>,
    pending_checks: Rc<Cell<i32>>,
    checking: Rc<Cell<bool>>,
    pub seconds: Rc<Cell<u32>>,
    was_active: Rc<Cell<bool>>,
    ticker: Rc<RefCell<Option<glib::SourceId>>>,
    listeners: Rc<Listeners>,
}

impl Recording {
    pub fn new() -> Self {
        let recording = Recording {
            seen: Rc::new(Cell::new(false)),
            expecting: Rc::new(Cell::new(false)),
            pending_checks: Rc::new(Cell::new(0)),
            checking: Rc::new(Cell::new(false)),
            seconds: Rc::new(Cell::new(0)),
            was_active: Rc::new(Cell::new(false)),
            ticker: Rc::new(RefCell::new(None)),
            listeners: Rc::default(),
        };
        recording.refresh();
        recording.tick();
        recording
    }

    pub fn active(&self) -> bool {
        self.seen.get() || self.expecting.get()
    }

    pub fn elapsed(&self) -> String {
        let seconds = self.seconds.get();
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn watch(&self) {
        self.expecting.set(true);
        self.start_checks();
        self.changed();
    }

    pub fn stop(&self) {
        if !self.active() {
            return;
        }
        crate::core::process::detach_subcommand(&["record"]);
        self.expecting.set(false);
        self.seen.set(false);
        self.start_checks();
        self.changed();
    }

    fn start_checks(&self) {
        let running = self.pending_checks.get() > 0;
        self.pending_checks.set(CHECKS);
        if running {
            return;
        }
        let recording = self.clone();
        glib::timeout_add_local(CHECK, move || {
            let left = recording.pending_checks.get() - 1;
            recording.pending_checks.set(left);
            if left == 0 {
                recording.expecting.set(false);
                recording.changed();
            }
            recording.refresh();
            if left > 0 {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            }
        });
    }

    fn tick(&self) {
        if let Some(previous) = self.ticker.borrow_mut().take() {
            previous.remove();
        }
        if !self.active() {
            return;
        }
        let recording = self.clone();
        let id = glib::timeout_add_local(Duration::from_secs(1), move || {
            recording.refresh();
            if recording.active() {
                recording.seconds.set(recording.seconds.get() + 1);
                recording.changed();
            }
            glib::ControlFlow::Continue
        });
        self.ticker.replace(Some(id));
    }

    pub fn check_idle(&self) {
        if !self.active() {
            self.refresh();
        }
    }

    fn refresh(&self) {
        if self.checking.replace(true) {
            return;
        }
        let recording = self.clone();
        glib::spawn_future_local(async move {
            let found = match gio::Subprocess::newv(
                &[
                    std::ffi::OsStr::new("pgrep"),
                    std::ffi::OsStr::new("wf-recorder"),
                ],
                gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
            ) {
                Ok(process) => process.wait_future().await.is_ok() && process.exit_status() == 0,
                Err(_) => false,
            };
            recording.checking.set(false);
            recording.seen.set(found);
            if found {
                recording.expecting.set(false);
            }
            recording.changed();
        });
    }

    fn changed(&self) {
        let active = self.active();
        if self.was_active.replace(active) != active {
            self.seconds.set(0);
            self.tick();
        }
        self.listeners.notify();
    }
}
