use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::{self, detach};
use crate::platform::dbus::notify::{self, Notification};

const CONNECT_FAILURE_MESSAGE: &str =
    "Connection failed. Please inspect manually with the <tt>warp-cli</tt> command";
const REGISTRATION_FAILURE_MESSAGE: &str =
    "Registration failed. Please inspect manually with the <tt>warp-cli</tt> command";

#[derive(Clone)]
pub struct Warp {
    pub is_available: Rc<Cell<bool>>,
    pub is_connected: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
}

impl Warp {
    pub fn new() -> Self {
        let warp = Warp {
            is_available: Rc::new(Cell::new(false)),
            is_connected: Rc::new(Cell::new(false)),
            listeners: Rc::default(),
        };
        warp.refresh();
        warp
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn toggle(&self) {
        let should_connect = !self.is_connected.get();
        self.is_connected.set(should_connect);
        detach(&[
            "warp-cli",
            if should_connect {
                "connect"
            } else {
                "disconnect"
            },
        ]);
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
                warp.is_available.set(true);
            }
            if status.contains("Unable") {
                warp.register();
            } else if status.contains("Connected") {
                warp.is_connected.set(true);
            } else if status.contains("Disconnected") {
                warp.is_connected.set(false);
            }
            warp.announce();
        });
    }

    fn register(&self) {
        glib::spawn_future_local(async move {
            if run(&["warp-cli", "registration", "new"]).await.is_none() {
                report(&tr(REGISTRATION_FAILURE_MESSAGE));
                return;
            }
            if run(&["warp-cli", "connect"]).await.is_none() {
                report(&tr(CONNECT_FAILURE_MESSAGE));
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
