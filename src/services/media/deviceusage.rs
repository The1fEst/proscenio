use gtk4::{gio, glib};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hyprland::hypr::{self, Events};
use crate::platform::system::desktop;
use crate::platform::wayland::capture;
use crate::services::hyprland::hyprstate::HyprState;
use crate::services::hyprland::screencast::{
    self, Kind, SHARING_DELAY, SHARING_GRACE, Sessions, Target,
};
use crate::services::media::audio::{Audio, Recorder};
use crate::services::media::cameras::{self, CameraUse};
use crate::services::media::privacy::{self, Application, VideoKind, VideoUser};

const OWN_CAPTURE_PERIOD: Duration = Duration::from_secs(2);
const MAX_VIDEO_RETRIES: u32 = 3;
const SCREENCOPY_RECORDERS: [&str; 2] = ["wf-recorder", "wl-screenrec"];
const PROC_ROOT: &str = "/proc";
const VIDEO_DEVICES_ROOT: &str = "/sys/class/video4linux";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceUse {
    pub application: Application,
    pub device: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Usage {
    pub microphones: Vec<DeviceUse>,
    pub screens: Vec<Target>,
    pub screen_applications: Vec<Application>,
    pub cameras: Vec<DeviceUse>,
}

struct Inner {
    audio: Option<Audio>,
    hypr: HyprState,
    usage: RefCell<Usage>,
    listeners: Listeners,
    sessions: RefCell<Sessions>,
    screens: RefCell<Vec<Target>>,
    screens_timer: RefCell<Option<glib::SourceId>>,
    confirmation_timer: RefCell<Option<glib::SourceId>>,
    has_grace_expired: Cell<bool>,
    is_verifying: Cell<bool>,
    is_recorder_running: Cell<bool>,
    cameras: RefCell<Vec<DeviceUse>>,
    brokered_devices: RefCell<Vec<String>>,
    video_users: RefCell<Vec<VideoUser>>,
    video_read_count: Cell<u64>,
    video_retry_count: Cell<u32>,
    applications: RefCell<HashMap<String, Application>>,
    is_scanning: Cell<bool>,
}

#[derive(Clone)]
pub struct DeviceUsage {
    inner: Rc<Inner>,
}

impl DeviceUsage {
    pub fn new(events: &Events, hypr: &HyprState, audio: Option<Audio>) -> Self {
        let usage = DeviceUsage {
            inner: Rc::new(Inner {
                audio,
                hypr: hypr.clone(),
                usage: RefCell::default(),
                listeners: Listeners::default(),
                sessions: RefCell::default(),
                screens: RefCell::default(),
                screens_timer: RefCell::default(),
                confirmation_timer: RefCell::default(),
                has_grace_expired: Cell::new(false),
                is_verifying: Cell::new(false),
                is_recorder_running: Cell::new(false),
                cameras: RefCell::default(),
                brokered_devices: RefCell::default(),
                video_users: RefCell::default(),
                video_read_count: Cell::new(0),
                video_retry_count: Cell::new(0),
                applications: RefCell::default(),
                is_scanning: Cell::new(false),
            }),
        };
        events
            .subscribe({
                let usage = usage.clone();
                move |event, data| {
                    if event == "screencastv2" {
                        usage.on_screencast(data);
                    }
                }
            })
            .detach();
        if let Some(audio) = &usage.inner.audio {
            audio
                .on_recorders_change({
                    let usage = usage.clone();
                    move || usage.update()
                })
                .detach();
        }
        usage
    }

    pub fn usage(&self) -> Usage {
        self.inner.usage.borrow().clone()
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.inner.listeners.add(listener)
    }

    pub fn refresh(&self) {
        self.scan_cameras();
        if !self.inner.screens.borrow().is_empty()
            || !self.inner.brokered_devices.borrow().is_empty()
        {
            self.inner.video_retry_count.set(0);
            self.read_video_users();
        }
    }

    pub fn scan_cameras(&self) {
        if self.inner.is_scanning.replace(true) {
            return;
        }
        let usage = self.clone();
        glib::spawn_future_local(async move {
            let scan = gio::spawn_blocking(|| {
                cameras::scan(Path::new(PROC_ROOT), Path::new(VIDEO_DEVICES_ROOT))
            })
            .await;
            usage.inner.is_scanning.set(false);
            if let Ok(scan) = scan {
                usage.on_scan(scan);
            }
        });
    }

    fn on_scan(&self, scan: cameras::Scan) {
        let cameras: Vec<DeviceUse> = scan
            .uses
            .into_iter()
            .map(|CameraUse { executable, device }| DeviceUse {
                application: self.describe(&executable),
                device,
            })
            .collect();
        let is_static_share =
            self.inner.has_grace_expired.get() && !self.inner.screens.borrow().is_empty();
        let has_new_brokered_devices = *self.inner.brokered_devices.borrow()
            != scan.brokered_devices
            && !scan.brokered_devices.is_empty();
        self.inner.cameras.replace(cameras);
        self.inner.brokered_devices.replace(scan.brokered_devices);
        if has_new_brokered_devices {
            self.inner.video_retry_count.set(0);
            self.read_video_users();
        } else if self.lacks_video_users() && self.inner.video_retry_count.get() < MAX_VIDEO_RETRIES
        {
            self.inner
                .video_retry_count
                .set(self.inner.video_retry_count.get() + 1);
            self.read_video_users();
        } else if is_static_share {
            self.read_video_users();
        }
        if is_static_share {
            self.scan_recorders();
        }
        self.update();
    }

    fn on_screencast(&self, data: &str) {
        let Some(session) = screencast::parse_session(data) else {
            return;
        };
        if session.is_active {
            let own_count = self.inner.sessions.borrow().own_count(&session.target);
            let is_own = self.is_own_session(&session.target, own_count);
            self.inner
                .sessions
                .borrow_mut()
                .start(session.target, is_own, Instant::now());
        } else {
            self.inner.sessions.borrow_mut().stop(&session.target);
        }
        self.show_screens();
    }

    fn is_own_session(&self, target: &Target, own_count: usize) -> bool {
        if target.kind != Kind::Window {
            return false;
        }
        let requested_windows = capture::list_requested_windows(OWN_CAPTURE_PERIOD);
        if requested_windows.is_empty() {
            return false;
        }
        let snapshot = self.inner.hypr.snapshot();
        let titles = read_window_titles(snapshot.clients.as_deref().unwrap_or_default());
        let (mut requested_count, titled_count) =
            count_windows(&target.name, &requested_windows, &titles);
        if titled_count == 0 {
            let Some(Value::Array(clients)) = hypr::json("clients") else {
                return false;
            };
            requested_count = count_windows(
                &target.name,
                &requested_windows,
                &read_window_titles(&clients),
            )
            .0;
        }
        own_count < requested_count
    }

    fn show_screens(&self) {
        let now = Instant::now();
        let shown_targets = self.inner.screens.borrow().clone();
        let (confirmed_targets, wait) = {
            let sessions = self.inner.sessions.borrow();
            (
                sessions.confirmed_targets(now, SHARING_DELAY, &shown_targets),
                sessions.next_confirmation(now, SHARING_DELAY, &shown_targets),
            )
        };
        if let Some(source) = self.inner.confirmation_timer.borrow_mut().take() {
            source.remove();
        }
        if let Some(wait) = wait {
            let usage = self.clone();
            let source = glib::timeout_add_local_once(wait + Duration::from_millis(1), move || {
                usage.inner.confirmation_timer.borrow_mut().take();
                usage.show_screens();
            });
            self.inner.confirmation_timer.replace(Some(source));
        }
        if !confirmed_targets.is_empty() {
            if let Some(source) = self.inner.screens_timer.borrow_mut().take() {
                source.remove();
            }
            self.inner.has_grace_expired.set(false);
            if shown_targets != confirmed_targets {
                self.inner.screens.replace(confirmed_targets);
                self.inner.video_retry_count.set(0);
                self.read_video_users();
                self.scan_recorders();
                self.update();
            }
            return;
        }
        if shown_targets.is_empty() {
            return;
        }
        if !self.inner.has_grace_expired.get() {
            if self.inner.screens_timer.borrow().is_none() && !self.inner.is_verifying.get() {
                let usage = self.clone();
                let source = glib::timeout_add_local_once(SHARING_GRACE, move || {
                    usage.inner.screens_timer.borrow_mut().take();
                    usage.verify_share();
                });
                self.inner.screens_timer.replace(Some(source));
            }
            return;
        }
        if !self.is_share_alive() {
            self.inner.has_grace_expired.set(false);
            self.inner.screens.replace(Vec::new());
            self.update();
        }
    }

    fn is_share_alive(&self) -> bool {
        self.inner.is_recorder_running.get()
            || self
                .inner
                .video_users
                .borrow()
                .iter()
                .any(|user| user.kind == VideoKind::Screen)
    }

    fn verify_share(&self) {
        self.inner.is_verifying.set(true);
        let usage = self.clone();
        glib::spawn_future_local(async move {
            let is_running =
                gio::spawn_blocking(|| is_recorder_running(Path::new(PROC_ROOT))).await;
            if let Ok(is_running) = is_running {
                usage.inner.is_recorder_running.set(is_running);
            }
            let device_usage = usage.clone();
            usage.read_video_users_with(move |is_latest| {
                device_usage.inner.is_verifying.set(false);
                let shown_targets = device_usage.inner.screens.borrow().clone();
                let is_static = device_usage
                    .inner
                    .sessions
                    .borrow()
                    .confirmed_targets(Instant::now(), SHARING_DELAY, &shown_targets)
                    .is_empty();
                if is_latest && is_static && !shown_targets.is_empty() {
                    device_usage.inner.has_grace_expired.set(true);
                }
            });
        });
    }

    fn scan_recorders(&self) {
        let usage = self.clone();
        glib::spawn_future_local(async move {
            let is_running =
                gio::spawn_blocking(|| is_recorder_running(Path::new(PROC_ROOT))).await;
            if let Ok(is_running) = is_running {
                usage.inner.is_recorder_running.set(is_running);
                usage.show_screens();
            }
        });
    }

    fn lacks_video_users(&self) -> bool {
        let video_users = self.inner.video_users.borrow();
        let has_user = |kind| video_users.iter().any(|user| user.kind == kind);
        (!self.inner.screens.borrow().is_empty() && !has_user(VideoKind::Screen))
            || (!self.inner.brokered_devices.borrow().is_empty() && !has_user(VideoKind::Camera))
    }

    fn read_video_users(&self) {
        self.read_video_users_with(|_| {});
    }

    fn read_video_users_with(&self, on_read: impl FnOnce(bool) + 'static) {
        let read_count = self.inner.video_read_count.get() + 1;
        self.inner.video_read_count.set(read_count);
        let usage = self.clone();
        privacy::read_video_users(move |users| {
            let is_latest = usage.inner.video_read_count.get() == read_count;
            if is_latest && let Some(users) = users {
                usage.inner.video_users.replace(users);
                usage.update();
            }
            on_read(is_latest);
            usage.show_screens();
        });
    }

    fn describe(&self, executable: &str) -> Application {
        if let Some(application) = self.inner.applications.borrow().get(executable) {
            return application.clone();
        }
        let application = match desktop::find(executable) {
            Some(entry) => Application {
                name: entry.name(),
                icon: entry.icon(),
            },
            None => Application {
                name: executable.to_owned(),
                icon: executable.to_owned(),
            },
        };
        self.inner
            .applications
            .borrow_mut()
            .insert(executable.to_owned(), application.clone());
        application
    }

    fn update(&self) {
        if self.inner.screens.borrow().is_empty() && self.inner.brokered_devices.borrow().is_empty()
        {
            self.inner.video_users.borrow_mut().clear();
        }
        let recorders = self
            .inner
            .audio
            .as_ref()
            .map(Audio::recorders)
            .unwrap_or_default();
        let usage = assemble(
            &recorders,
            &self.inner.screens.borrow(),
            &self.inner.cameras.borrow(),
            !self.inner.brokered_devices.borrow().is_empty(),
            &self.inner.video_users.borrow(),
        );
        if *self.inner.usage.borrow() == usage {
            return;
        }
        self.inner.usage.replace(usage);
        self.inner.listeners.notify();
    }
}

fn is_recorder_comm(comm: &str) -> bool {
    SCREENCOPY_RECORDERS.contains(&comm)
}

fn is_recorder_running(proc_root: &Path) -> bool {
    let Ok(processes) = std::fs::read_dir(proc_root) else {
        return false;
    };
    processes.flatten().any(|process| {
        std::fs::read_to_string(process.path().join("comm"))
            .is_ok_and(|comm| is_recorder_comm(comm.trim_end()))
    })
}

fn read_window_titles(clients: &[Value]) -> Vec<(u32, String)> {
    clients
        .iter()
        .filter_map(|client| {
            let address = client["address"].as_str()?.trim_start_matches("0x");
            let handle = u64::from_str_radix(address, 16).ok()? as u32;
            Some((handle, client["title"].as_str()?.to_owned()))
        })
        .collect()
}

fn count_windows(
    title: &str,
    requested_windows: &[u32],
    titles: &[(u32, String)],
) -> (usize, usize) {
    let titled_windows = titles
        .iter()
        .filter(|(_, window_title)| window_title == title);
    let requested_count = titled_windows
        .clone()
        .filter(|(handle, _)| requested_windows.contains(handle))
        .count();
    (requested_count, titled_windows.count())
}

fn assemble(
    recorders: &[Recorder],
    screens: &[Target],
    cameras: &[DeviceUse],
    has_brokered_cameras: bool,
    video_users: &[VideoUser],
) -> Usage {
    let mut usage = Usage {
        microphones: recorders
            .iter()
            .map(|recorder| DeviceUse {
                application: Application {
                    name: recorder.name.clone(),
                    icon: recorder.icon.clone(),
                },
                device: recorder.source.clone(),
            })
            .collect(),
        screens: screens.to_vec(),
        cameras: cameras.to_vec(),
        ..Usage::default()
    };
    for user in video_users {
        match user.kind {
            VideoKind::Screen if !screens.is_empty() => {
                if !usage.screen_applications.contains(&user.application) {
                    usage.screen_applications.push(user.application.clone());
                }
            }
            VideoKind::Camera if has_brokered_cameras => {
                let camera = DeviceUse {
                    application: user.application.clone(),
                    device: user.device.clone(),
                };
                if !usage.cameras.contains(&camera) {
                    usage.cameras.push(camera);
                }
            }
            _ => {}
        }
    }
    usage
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn application(name: &str) -> Application {
        Application {
            name: name.to_owned(),
            icon: String::new(),
        }
    }

    fn device_use(name: &str, device: &str) -> DeviceUse {
        DeviceUse {
            application: application(name),
            device: device.to_owned(),
        }
    }

    fn video_user(kind: VideoKind, name: &str, device: &str) -> VideoUser {
        VideoUser {
            kind,
            application: application(name),
            device: device.to_owned(),
        }
    }

    fn monitor() -> Target {
        Target {
            kind: Kind::Monitor,
            name: "DP-2".to_owned(),
        }
    }

    #[test]
    fn a_client_names_its_export_handle_and_title() {
        let clients = [
            json!({"address": "0x5581a0c3d1f0", "title": "Dolphin"}),
            json!({"address": "0xzz", "title": "Broken"}),
            json!({"address": "0x10", "class": "no title"}),
        ];
        assert_eq!(
            read_window_titles(&clients),
            [(0xa0c3d1f0, "Dolphin".to_owned())]
        );
    }

    #[test]
    fn windows_with_a_title_are_counted_with_the_ones_requested() {
        let titles = [
            (1, "Terminal".to_owned()),
            (2, "Browser".to_owned()),
            (3, "Browser".to_owned()),
        ];
        let cases = [
            ("Terminal", vec![1], (1, 1)),
            ("Terminal", vec![2], (0, 1)),
            ("Browser", vec![3, 2], (2, 2)),
            ("Browser", vec![1, 3], (1, 2)),
            ("Terminal", vec![], (0, 1)),
            ("Renamed since the last snapshot", vec![1], (0, 0)),
        ];
        for (title, requested_windows, expected) in cases {
            assert_eq!(
                count_windows(title, &requested_windows, &titles),
                expected,
                "{title} {requested_windows:?}"
            );
        }
    }

    #[test]
    fn a_screencopy_recorder_is_known_by_its_exact_command_name() {
        for comm in ["wf-recorder", "wl-screenrec"] {
            assert!(is_recorder_comm(comm), "{comm}");
        }
        for comm in ["wf-recorder-gui", "wayvnc", "obs", "grim", ""] {
            assert!(!is_recorder_comm(comm), "{comm}");
        }
    }

    #[test]
    fn a_recorder_is_running_when_a_process_has_its_name() {
        let root = std::env::temp_dir().join(format!("proscenio-recorders-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let write = |pid: &str, comm: &str| {
            std::fs::create_dir_all(root.join(pid)).unwrap();
            std::fs::write(root.join(pid).join("comm"), format!("{comm}\n")).unwrap();
        };
        write("10", "kitty");
        write("11", "grim");
        assert!(!is_recorder_running(&root));
        write("12", "wf-recorder");
        assert!(is_recorder_running(&root));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn nothing_in_use_is_idle() {
        assert_eq!(assemble(&[], &[], &[], false, &[]), Usage::default());
        assert_eq!(assemble(&[], &[], &[], true, &[]), Usage::default());
    }

    #[test]
    fn a_recorder_becomes_a_microphone_use_of_its_source() {
        let recorder = Recorder {
            name: "TeamSpeak".to_owned(),
            icon: "teamspeak".to_owned(),
            source: "Scarlett Mic".to_owned(),
        };
        let usage = assemble(&[recorder], &[], &[], false, &[]);
        assert_eq!(
            usage.microphones,
            [DeviceUse {
                application: Application {
                    name: "TeamSpeak".to_owned(),
                    icon: "teamspeak".to_owned(),
                },
                device: "Scarlett Mic".to_owned(),
            }]
        );
    }

    #[test]
    fn a_shared_screen_lists_the_applications_reading_it_and_nothing_else() {
        let users = [
            video_user(VideoKind::Screen, "OBS", ""),
            video_user(VideoKind::Screen, "OBS", ""),
            video_user(VideoKind::Camera, "Zoom", "Integrated Camera"),
        ];
        let usage = assemble(&[], &[monitor()], &[], false, &users);
        assert_eq!(usage.screens, [monitor()]);
        assert_eq!(usage.screen_applications, [application("OBS")]);
        assert_eq!(usage.cameras, []);
    }

    #[test]
    fn video_readers_name_no_screen_application_while_no_screen_is_shared() {
        let users = [video_user(VideoKind::Screen, "OBS", "")];
        assert_eq!(assemble(&[], &[], &[], false, &users), Usage::default());
        assert_eq!(
            assemble(&[], &[], &[], false, &users).screen_applications,
            []
        );
    }

    #[test]
    fn a_camera_held_by_the_sound_server_is_used_by_the_applications_streaming_it() {
        let direct_cameras = [device_use("obs", "Phone Camera")];
        let users = [
            video_user(VideoKind::Camera, "Zoom", "Integrated Camera"),
            video_user(VideoKind::Camera, "Zoom", "Integrated Camera"),
            video_user(VideoKind::Screen, "OBS", ""),
        ];
        assert_eq!(
            assemble(&[], &[], &direct_cameras, false, &users).cameras,
            direct_cameras
        );
        assert_eq!(
            assemble(&[], &[], &direct_cameras, true, &users).cameras,
            [
                device_use("obs", "Phone Camera"),
                device_use("Zoom", "Integrated Camera")
            ]
        );
    }
}
