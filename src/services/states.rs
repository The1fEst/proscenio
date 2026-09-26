use std::cell::Cell;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Osd {
    Volume,
    Brightness,
}

#[derive(Clone)]
pub struct States {
    pub bar_open: Rc<Cell<bool>>,
    pub super_down: Rc<Cell<bool>>,
    pub sidebar_open: Rc<Cell<bool>>,
    pub screen_locked: Rc<Cell<bool>>,
    pub osk_open: Rc<Cell<bool>>,
    listeners: Rc<Listeners>,
    osd_closers: Rc<Listeners<Osd>>,
}

impl States {
    pub fn new() -> Self {
        States {
            bar_open: Rc::new(Cell::new(true)),
            super_down: Rc::new(Cell::new(false)),
            sidebar_open: Rc::new(Cell::new(false)),
            screen_locked: Rc::new(Cell::new(false)),
            osk_open: Rc::new(Cell::new(false)),
            listeners: Rc::default(),
            osd_closers: Rc::default(),
        }
    }

    pub fn on_osd_close(&self, listener: impl Fn(Osd) + 'static) -> Subscription {
        self.osd_closers.add_with(move |kind| listener(*kind))
    }

    pub fn close_osd(&self, kind: Osd) {
        self.osd_closers.notify_with(&kind);
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn set_bar_open(&self, open: bool) {
        if self.bar_open.replace(open) != open {
            self.announce();
        }
    }

    pub fn set_super_down(&self, down: bool) {
        if self.super_down.replace(down) != down {
            self.announce();
        }
    }

    pub fn set_sidebar_open(&self, open: bool) {
        if self.sidebar_open.replace(open) != open {
            self.announce();
        }
    }

    pub fn set_screen_locked(&self, locked: bool) {
        if self.screen_locked.replace(locked) != locked {
            self.announce();
        }
    }

    pub fn set_osk_open(&self, open: bool) {
        if self.osk_open.replace(open) != open {
            self.announce();
        }
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}
