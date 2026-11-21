use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Osd {
    Volume,
    Brightness,
}

#[derive(Clone)]
pub struct States {
    pub is_bar_open: Rc<Cell<bool>>,
    pub is_super_down: Rc<Cell<bool>>,
    pub is_sidebar_open: Rc<Cell<bool>>,
    pub is_screen_locked: Rc<Cell<bool>>,
    pub is_osk_open: Rc<Cell<bool>>,
    covered_monitors: Rc<RefCell<HashSet<String>>>,
    open_sidebar_count: Rc<Cell<usize>>,
    listeners: Rc<Listeners>,
    osd_close_listeners: Rc<Listeners<Osd>>,
}

impl States {
    pub fn new() -> Self {
        States {
            is_bar_open: Rc::new(Cell::new(true)),
            is_super_down: Rc::new(Cell::new(false)),
            is_sidebar_open: Rc::new(Cell::new(false)),
            is_screen_locked: Rc::new(Cell::new(false)),
            is_osk_open: Rc::new(Cell::new(false)),
            covered_monitors: Rc::default(),
            open_sidebar_count: Rc::default(),
            listeners: Rc::default(),
            osd_close_listeners: Rc::default(),
        }
    }

    pub fn on_osd_close(&self, listener: impl Fn(Osd) + 'static) -> Subscription {
        self.osd_close_listeners
            .add_with(move |kind| listener(*kind))
    }

    pub fn close_osd(&self, kind: Osd) {
        self.osd_close_listeners.notify_with(&kind);
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn set_bar_open(&self, is_open: bool) {
        if self.is_bar_open.replace(is_open) != is_open {
            self.announce();
        }
    }

    pub fn set_super_down(&self, is_down: bool) {
        if self.is_super_down.replace(is_down) != is_down {
            self.announce();
        }
    }

    pub fn add_open_sidebar(&self) {
        self.set_open_sidebar_count(self.open_sidebar_count.get() + 1);
    }

    pub fn remove_open_sidebar(&self) {
        self.set_open_sidebar_count(self.open_sidebar_count.get().saturating_sub(1));
    }

    fn set_open_sidebar_count(&self, count: usize) {
        self.open_sidebar_count.set(count);
        let is_open = count > 0;
        if self.is_sidebar_open.replace(is_open) != is_open {
            self.announce();
        }
    }

    pub fn set_screen_locked(&self, is_locked: bool) {
        if self.is_screen_locked.replace(is_locked) != is_locked {
            self.announce();
        }
    }

    pub fn is_monitor_covered(&self, monitor: &str) -> bool {
        self.covered_monitors.borrow().contains(monitor)
    }

    pub fn set_monitor_covered(&self, monitor: &str, is_covered: bool) {
        let has_changed = if is_covered {
            self.covered_monitors
                .borrow_mut()
                .insert(monitor.to_owned())
        } else {
            self.covered_monitors.borrow_mut().remove(monitor)
        };
        if has_changed {
            self.announce();
        }
    }

    pub fn set_osk_open(&self, is_open: bool) {
        if self.is_osk_open.replace(is_open) != is_open {
            self.announce();
        }
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sidebar_counts_as_open_until_every_open_one_closes() {
        let states = States::new();
        states.add_open_sidebar();
        states.add_open_sidebar();
        states.remove_open_sidebar();
        assert!(states.is_sidebar_open.get());
        states.remove_open_sidebar();
        assert!(!states.is_sidebar_open.get());
    }

    #[test]
    fn a_monitor_counts_as_covered_only_between_its_reports() {
        let states = States::new();
        let announcement_count = Rc::new(Cell::new(0));
        let _subscription = states.subscribe({
            let announcement_count = announcement_count.clone();
            move || announcement_count.set(announcement_count.get() + 1)
        });
        states.set_monitor_covered("DP-1", true);
        states.set_monitor_covered("DP-1", true);
        assert!(states.is_monitor_covered("DP-1"));
        assert!(!states.is_monitor_covered("DP-2"));
        states.set_monitor_covered("DP-1", false);
        assert!(!states.is_monitor_covered("DP-1"));
        assert_eq!(announcement_count.get(), 2);
    }
}
