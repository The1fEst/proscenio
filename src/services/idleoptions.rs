use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::platform::hypridle;

const WRITE_DELAY: Duration = Duration::from_millis(50);

pub struct IdleOptions {
    timeouts: RefCell<HashMap<String, u64>>,
    general: RefCell<HashMap<String, String>>,
    pending: RefCell<Vec<(String, i64)>>,
    pending_general: RefCell<Vec<(String, Option<String>)>>,
    writing: Cell<Option<glib::SourceId>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl IdleOptions {
    pub fn new() -> Rc<Self> {
        Rc::new(IdleOptions {
            timeouts: RefCell::new(hypridle::read()),
            general: RefCell::new(hypridle::read_general()),
            pending: RefCell::new(Vec::new()),
            pending_general: RefCell::new(Vec::new()),
            writing: Cell::new(None),
            listeners: RefCell::new(Vec::new()),
        })
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn seconds(&self, what: &str) -> u64 {
        self.timeouts.borrow().get(what).copied().unwrap_or(0)
    }

    pub fn general(&self, key: &str) -> Option<String> {
        self.general.borrow().get(key).cloned()
    }

    pub fn set(self: &Rc<Self>, what: &str, seconds: i64) {
        {
            let mut pending = self.pending.borrow_mut();
            match pending.iter_mut().find(|(known, _)| known == what) {
                Some(slot) => slot.1 = seconds,
                None => pending.push((what.to_owned(), seconds)),
            }
        }
        self.schedule();
    }

    pub fn set_general(self: &Rc<Self>, key: &str, value: Option<&str>) {
        {
            let mut pending = self.pending_general.borrow_mut();
            let value = value.map(str::to_owned);
            match pending.iter_mut().find(|(known, _)| known == key) {
                Some(slot) => slot.1 = value,
                None => pending.push((key.to_owned(), value)),
            }
        }
        self.schedule();
    }

    fn schedule(self: &Rc<Self>) {
        if let Some(source) = self.writing.take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        self.writing
            .set(Some(glib::timeout_add_local_once(WRITE_DELAY, move || {
                let Some(options) = weak.upgrade() else {
                    return;
                };
                options.writing.set(None);
                if !options.persist() {
                    return;
                }
                glib::spawn_future_local(async move {
                    restart_hypridle().await;
                    options.timeouts.replace(hypridle::read());
                    options.general.replace(hypridle::read_general());
                    for listener in options.listeners.borrow().iter() {
                        listener();
                    }
                });
            })));
    }

    fn persist(&self) -> bool {
        let pairs = self.pending.take();
        let general = self.pending_general.take();
        if pairs.is_empty() && general.is_empty() {
            return false;
        }
        if !pairs.is_empty() {
            let _ = hypridle::write(&pairs);
        }
        if !general.is_empty() {
            let _ = hypridle::write_general(&general);
        }
        true
    }
}

impl Drop for IdleOptions {
    fn drop(&mut self) {
        if let Some(source) = self.writing.take() {
            source.remove();
        }
        if self.persist() {
            glib::spawn_future_local(restart_hypridle());
        }
    }
}

async fn restart_hypridle() {
    let command = ["systemctl", "--user", "restart", "hypridle.service"];
    if let Ok(process) = gio::Subprocess::newv(
        &command.map(std::ffi::OsStr::new),
        gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
    ) {
        let _ = process.wait_future().await;
    }
}
