use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::{config, watch};

#[derive(Clone)]
pub struct Updates {
    pub count: Rc<Cell<i32>>,
    pub available: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
    following: Rc<RefCell<Option<watch::Watch>>>,
}

impl Updates {
    pub fn new() -> Self {
        let updates = Updates {
            count: Rc::new(Cell::new(0)),
            available: Rc::new(Cell::new(false)),
            listeners: Rc::default(),
            following: Rc::default(),
        };
        let checker = updates.clone();
        updates
            .following
            .replace(Some(watch::config("/updates/enableCheck", move || {
                if config::current().updates_enable_check {
                    checker.refresh();
                }
            })));
        updates
    }

    pub fn period() -> Duration {
        let config = config::current();
        if !config.updates_enable_check {
            return Duration::ZERO;
        }
        Duration::from_secs((config.updates_interval as u64).max(1) * 60)
    }

    pub fn advised(&self) -> bool {
        self.available.get() && self.count.get() > config::current().updates_advise
    }

    pub fn strongly_advised(&self) -> bool {
        self.available.get() && self.count.get() > config::current().updates_strongly_advise
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn refresh(&self) {
        let updates = self.clone();
        glib::spawn_future_local(async move {
            if glib::find_program_in_path("checkupdates").is_none() {
                return;
            }
            updates.available.set(true);
            let Some(output) = run("checkupdates").await else {
                return;
            };
            updates
                .count
                .set(output.lines().filter(|line| !line.is_empty()).count() as i32);
            updates.listeners.notify();
        });
    }
}

async fn run(program: &str) -> Option<String> {
    let process = gio::Subprocess::newv(
        &[std::ffi::OsStr::new(program)],
        gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE,
    )
    .ok()?;
    let (stdout, _) = process.communicate_utf8_future(None).await.ok()?;
    stdout.map(Into::into)
}
