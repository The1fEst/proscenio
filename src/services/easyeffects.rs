use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::{self, detach};
use crate::platform::desktop;

const AVAILABLE: &str =
    "command -v easyeffects || flatpak info com.github.wwmm.easyeffects > /dev/null 2>&1";
const RUNNING: &str =
    "pidof easyeffects || flatpak ps | grep com.github.wwmm.easyeffects > /dev/null 2>&1";
const ENABLE: &str = "easyeffects --hide-window --service-mode || \
                      flatpak run com.github.wwmm.easyeffects --hide-window --service-mode";
const DISABLE: &str = "pkill easyeffects || flatpak pkill com.github.wwmm.easyeffects";
const CONFIGURE: &str = "flatpak run com.github.wwmm.easyeffects || easyeffects";

#[derive(Clone)]
pub struct EasyEffects {
    pub available: Rc<Cell<bool>>,
    pub active: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
}

impl EasyEffects {
    pub fn new() -> Self {
        let service = EasyEffects {
            available: Rc::new(Cell::new(false)),
            active: Rc::new(Cell::new(false)),
            listeners: Rc::default(),
        };
        service.probe(AVAILABLE, {
            let available = service.available.clone();
            move |ok| available.set(ok)
        });
        service.probe(RUNNING, {
            let active = service.active.clone();
            move |ok| active.set(ok)
        });
        service
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn toggle(&self) {
        let on = !self.active.get();
        self.active.set(on);
        if on {
            desktop::shell(ENABLE);
        } else {
            detach(&["bash", "-c", DISABLE]);
        }
        self.announce();
    }

    pub fn configure(&self) {
        desktop::shell(CONFIGURE);
    }

    fn announce(&self) {
        self.listeners.notify();
    }

    fn probe(&self, line: &'static str, keep: impl Fn(bool) + 'static) {
        let service = self.clone();
        glib::spawn_future_local(async move {
            let Some(success) = process::finish(process::quiet(&["bash", "-c", line])).await else {
                return;
            };
            keep(success);
            service.announce();
        });
    }
}
