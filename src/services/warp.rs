use gtk4::gio;
use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::detach;

const FAILED: &str =
    "Connection failed. Please inspect manually with the <tt>warp-cli</tt> command";
const UNREGISTERED: &str =
    "Registration failed. Please inspect manually with the <tt>warp-cli</tt> command";

#[derive(Clone)]
pub struct Warp {
    pub available: Rc<Cell<bool>>,
    pub connected: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
}

impl Warp {
    pub fn new() -> Self {
        let warp = Warp {
            available: Rc::new(Cell::new(false)),
            connected: Rc::new(Cell::new(false)),
            listeners: Rc::default(),
        };
        warp.refresh();
        warp
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn toggle(&self) {
        let on = !self.connected.get();
        self.connected.set(on);
        detach(&["warp-cli", if on { "connect" } else { "disconnect" }]);
        self.announce();
    }

    fn announce(&self) {
        self.listeners.notify();
    }

    pub fn refresh(&self) {
        let warp = self.clone();
        glib::spawn_future_local(async move {
            let Some(status) = run(&["warp-cli", "status"]).await else {
                return;
            };
            if !status.is_empty() {
                warp.available.set(true);
            }
            if status.contains("Unable") {
                warp.register();
            } else if status.contains("Connected") {
                warp.connected.set(true);
            } else if status.contains("Disconnected") {
                warp.connected.set(false);
            }
            warp.announce();
        });
    }

    fn register(&self) {
        glib::spawn_future_local(async move {
            if run(&["warp-cli", "registration", "new"]).await.is_none() {
                report(UNREGISTERED);
                return;
            }
            if run(&["warp-cli", "connect"]).await.is_none() {
                report(FAILED);
            }
        });
    }
}

fn report(body: &str) {
    detach(&["notify-send", "Cloudflare WARP", body, "-a", "Shell"]);
}

async fn run(line: &[&str]) -> Option<String> {
    let arguments: Vec<&std::ffi::OsStr> = line.iter().map(std::ffi::OsStr::new).collect();
    let process = gio::Subprocess::newv(
        &arguments,
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    )
    .ok()?;
    let (stdout, _) = process.communicate_utf8_future(None).await.ok()?;
    if !process.has_exited() || process.exit_status() != 0 {
        return None;
    }
    Some(stdout.map(Into::into).unwrap_or_default())
}
