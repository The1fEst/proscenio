use gtk4::glib;
use serde_json::{Map, Value};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::platform::{hypr, hyprconfig};

const WRITE_DELAY: Duration = Duration::from_millis(50);
const EMPTY: &str = "[[EMPTY]]";

pub struct HyprOptions {
    names: Vec<&'static str>,
    options: RefCell<Map<String, Value>>,
    pending: RefCell<Vec<(String, String)>>,
    writing: Cell<Option<glib::SourceId>>,
    listeners: RefCell<Vec<Box<dyn Fn()>>>,
}

impl HyprOptions {
    pub fn new(names: &[&'static str]) -> Rc<Self> {
        let options = Rc::new(HyprOptions {
            names: names.to_vec(),
            options: RefCell::new(Map::new()),
            pending: RefCell::new(Vec::new()),
            writing: Cell::new(None),
            listeners: RefCell::new(Vec::new()),
        });
        options.reload();
        options
    }

    pub fn connect_changed(&self, listener: impl Fn() + 'static) {
        self.listeners.borrow_mut().push(Box::new(listener));
    }

    pub fn reload(&self) {
        self.options.replace(hyprconfig::read_options(&self.names));
        for listener in self.listeners.borrow().iter() {
            listener();
        }
    }

    fn raw(&self, name: &str) -> Option<Value> {
        self.options.borrow().get(name).cloned()
    }

    pub fn number(&self, name: &str) -> f64 {
        match self.raw(name) {
            Some(Value::String(text)) => leading_number(&text),
            Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0),
            Some(Value::Bool(flag)) => f64::from(u8::from(flag)),
            _ => 0.0,
        }
    }

    pub fn number_or(&self, name: &str, fallback: f64) -> f64 {
        if self.raw(name).is_none() {
            fallback
        } else {
            self.number(name)
        }
    }

    pub fn flag(&self, name: &str) -> bool {
        self.raw(name) == Some(Value::Bool(true))
    }

    pub fn text(&self, name: &str) -> String {
        let text = match self.raw(name) {
            None => return String::new(),
            Some(Value::String(text)) => text,
            Some(other) => other.to_string(),
        };
        if text == EMPTY { String::new() } else { text }
    }

    pub fn set(self: &Rc<Self>, name: &str, value: &str) {
        {
            let mut pending = self.pending.borrow_mut();
            match pending.iter_mut().find(|(known, _)| known == name) {
                Some(slot) => slot.1 = value.to_owned(),
                None => pending.push((name.to_owned(), value.to_owned())),
            }
        }
        if let Some(source) = self.writing.take() {
            source.remove();
        }
        let weak = Rc::downgrade(self);
        self.writing
            .set(Some(glib::timeout_add_local_once(WRITE_DELAY, move || {
                if let Some(options) = weak.upgrade() {
                    options.writing.set(None);
                    if options.persist() {
                        options.reload();
                    }
                }
            })));
    }

    fn persist(&self) -> bool {
        let pairs = self.pending.take();
        if pairs.is_empty() {
            return false;
        }
        let _ = hyprconfig::write_options(&pairs);
        hypr::request("reload");
        true
    }
}

impl Drop for HyprOptions {
    fn drop(&mut self) {
        if let Some(source) = self.writing.take() {
            source.remove();
        }
        self.persist();
    }
}

fn leading_number(text: &str) -> f64 {
    let trimmed = text.trim_start();
    let end = trimmed
        .char_indices()
        .take_while(|(index, character)| {
            character.is_ascii_digit()
                || *character == '.'
                || (*index == 0 && (*character == '-' || *character == '+'))
        })
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    trimmed[..end].parse().unwrap_or(f64::NAN)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_text_value_reads_as_the_number_it_starts_with() {
        assert_eq!(leading_number("4 4 4 4"), 4.0);
        assert_eq!(leading_number("-1.5px"), -1.5);
        assert!(leading_number("none").is_nan());
    }
}
