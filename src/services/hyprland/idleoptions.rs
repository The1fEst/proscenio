use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::core::process;
use crate::platform::hyprland::hypridle;

const WRITE_DELAY: Duration = Duration::from_millis(50);

pub struct IdleOptions {
    timeouts: RefCell<HashMap<String, u64>>,
    general: RefCell<HashMap<String, String>>,
    pending_timeouts: RefCell<Vec<(String, i64)>>,
    pending_general: RefCell<Vec<(String, Option<String>)>>,
    write_timer: Cell<Option<glib::SourceId>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl IdleOptions {
    pub fn new() -> Rc<Self> {
        Rc::new(IdleOptions {
            timeouts: RefCell::new(hypridle::read()),
            general: RefCell::new(hypridle::read_general()),
            pending_timeouts: RefCell::new(Vec::new()),
            pending_general: RefCell::new(Vec::new()),
            write_timer: Cell::new(None),
            listeners: RefCell::new(Vec::new()),
        })
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn seconds(&self, timeout_name: &str) -> u64 {
        self.timeouts
            .borrow()
            .get(timeout_name)
            .copied()
            .unwrap_or(0)
    }

    pub fn general(&self, key: &str) -> Option<String> {
        self.general.borrow().get(key).cloned()
    }

    pub fn set(self: &Rc<Self>, timeout_name: &str, seconds: i64) {
        {
            let mut pending_timeouts = self.pending_timeouts.borrow_mut();
            match pending_timeouts
                .iter_mut()
                .find(|(pending_name, _)| pending_name == timeout_name)
            {
                Some(slot) => slot.1 = seconds,
                None => pending_timeouts.push((timeout_name.to_owned(), seconds)),
            }
        }
        self.schedule();
    }

    pub fn set_general(self: &Rc<Self>, key: &str, value: Option<&str>) {
        {
            let mut pending_general = self.pending_general.borrow_mut();
            let value = value.map(str::to_owned);
            match pending_general
                .iter_mut()
                .find(|(pending_key, _)| pending_key == key)
            {
                Some(slot) => slot.1 = value,
                None => pending_general.push((key.to_owned(), value)),
            }
        }
        self.schedule();
    }

    fn schedule(self: &Rc<Self>) {
        if let Some(source) = self.write_timer.take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        self.write_timer
            .set(Some(glib::timeout_add_local_once(WRITE_DELAY, move || {
                let Some(options) = weak.upgrade() else {
                    return;
                };
                options.write_timer.set(None);
                let Some(has_written) = options.persist() else {
                    return;
                };
                glib::spawn_future_local(async move {
                    if has_written {
                        restart_hypridle().await;
                    }
                    options.timeouts.replace(hypridle::read());
                    options.general.replace(hypridle::read_general());
                    for listener in options.listeners.borrow().iter() {
                        listener();
                    }
                });
            })));
    }

    fn persist(&self) -> Option<bool> {
        let timeout_changes = self.pending_timeouts.take();
        let general_changes = self.pending_general.take();
        if timeout_changes.is_empty() && general_changes.is_empty() {
            return None;
        }
        let result = hypridle::write(&timeout_changes, &general_changes);
        if let Err(error) = &result {
            eprintln!(
                "idleoptions: {} could not be written: {error}",
                hypridle::config_path().display()
            );
        }
        Some(matches!(result, Ok(true)))
    }
}

impl Drop for IdleOptions {
    fn drop(&mut self) {
        if let Some(source) = self.write_timer.take() {
            source.remove();
        }
        if self.persist() == Some(true) {
            glib::spawn_future_local(restart_hypridle());
        }
    }
}

async fn restart_hypridle() {
    let command = ["systemctl", "--user", "restart", "hypridle.service"];
    process::finish(process::quiet(&command)).await;
}
