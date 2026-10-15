use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;
use crate::platform::system::desktop;

const AVAILABILITY_CHECK_COMMAND: &str =
    "command -v easyeffects || flatpak info com.github.wwmm.easyeffects > /dev/null 2>&1";
const RUNNING_CHECK_COMMAND: &str =
    "pidof easyeffects || flatpak ps | grep com.github.wwmm.easyeffects > /dev/null 2>&1";
const ENABLE_COMMAND: &str = "easyeffects --hide-window --service-mode || \
                      flatpak run com.github.wwmm.easyeffects --hide-window --service-mode";
const DISABLE_COMMAND: &str = "pkill easyeffects || flatpak pkill com.github.wwmm.easyeffects";
const CONFIGURE_COMMAND: &str = "flatpak run com.github.wwmm.easyeffects || easyeffects";

#[derive(Clone)]
pub struct EasyEffects {
    pub is_available: Rc<Cell<bool>>,
    pub is_active: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
}

impl EasyEffects {
    pub fn new() -> Self {
        let service = EasyEffects {
            is_available: Rc::new(Cell::new(false)),
            is_active: Rc::new(Cell::new(false)),
            listeners: Rc::default(),
        };
        service.probe(AVAILABILITY_CHECK_COMMAND, {
            let is_available = service.is_available.clone();
            move |is_ok| is_available.set(is_ok)
        });
        service.probe_running();
        service
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn toggle(&self) {
        let service = self.clone();
        if self.is_active.get() {
            glib::spawn_future_local(async move {
                match process::finish(process::quiet(&["bash", "-c", DISABLE_COMMAND])).await {
                    Some(true) => {
                        service.is_active.set(false);
                        service.announce();
                    }
                    _ => service.probe_running(),
                }
            });
            return;
        }
        self.is_active.set(true);
        self.announce();
        glib::spawn_future_local(async move {
            process::finish(process::own_scope(None, &["bash", "-c", ENABLE_COMMAND])).await;
            service.probe_running();
        });
    }

    fn probe_running(&self) {
        self.probe(RUNNING_CHECK_COMMAND, {
            let is_active = self.is_active.clone();
            move |is_ok| is_active.set(is_ok)
        });
    }

    pub fn configure(&self) {
        desktop::run_shell(CONFIGURE_COMMAND);
    }

    fn announce(&self) {
        self.listeners.notify();
    }

    fn probe(&self, command: &'static str, on_result: impl Fn(bool) + 'static) {
        let service = self.clone();
        glib::spawn_future_local(async move {
            let Some(is_success) = process::finish(process::quiet(&["bash", "-c", command])).await
            else {
                return;
            };
            on_result(is_success);
            service.announce();
        });
    }
}
