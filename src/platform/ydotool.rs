use std::cell::Cell;
use std::rc::Rc;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::process;

pub const SHIFT_KEYS: [u16; 2] = [42, 54];
const KEYCODES: u16 = 249;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Shift {
    #[default]
    Off,
    On,
    Locked,
}

#[derive(Clone, Default)]
pub struct Ydotool {
    shift: Rc<Cell<Shift>>,
    listeners: Rc<Listeners>,
}

impl Ydotool {
    pub fn shift(&self) -> Shift {
        self.shift.get()
    }

    pub fn set_shift(&self, shift: Shift) {
        if self.shift.replace(shift) != shift {
            self.listeners.notify();
        }
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn press(&self, keycode: u16) {
        send(&[format!("{keycode}:1")]);
    }

    pub fn release(&self, keycode: u16) {
        send(&[format!("{keycode}:0")]);
    }

    pub fn release_shift_keys(&self) {
        send(&SHIFT_KEYS.map(|keycode| format!("{keycode}:0")));
        self.set_shift(Shift::Off);
    }

    pub fn release_all_keys(&self) {
        let keys: Vec<String> = (0..KEYCODES)
            .map(|keycode| format!("{keycode}:0"))
            .collect();
        send(&keys);
        self.set_shift(Shift::Off);
    }
}

fn send(keys: &[String]) {
    let mut command = vec!["ydotool", "key", "--key-delay", "0"];
    command.extend(keys.iter().map(String::as_str));
    process::detach(&command);
}
