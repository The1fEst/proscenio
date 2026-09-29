use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::{self, detach};
use crate::platform::notify::{self, Notification};

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
                report(&tr(UNREGISTERED));
                return;
            }
            if run(&["warp-cli", "connect"]).await.is_none() {
                report(&tr(FAILED));
            }
        });
    }
}

fn report(body: &str) {
    notify::send(&Notification {
        app: "Shell",
        summary: &tr("Cloudflare WARP"),
        body,
        ..Default::default()
    });
}

async fn run(line: &[&str]) -> Option<String> {
    let mut command = process::command(line);
    command.stderr(std::process::Stdio::null());
    let output = process::capture(command).await?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}
