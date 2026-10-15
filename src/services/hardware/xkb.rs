use gtk4::glib;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::platform::hyprland::hypr::{self, Events};

const BASE_LAYOUTS: &str = "/usr/share/X11/xkb/rules/base.lst";
const SETTLE_DELAY: Duration = Duration::from_millis(120);

#[derive(Clone)]
pub struct Xkb {
    pub layout_codes: Rc<RefCell<Vec<String>>>,
    pub current_name: Rc<RefCell<String>>,
    pub current_code: Rc<RefCell<String>>,
    cached_codes: Rc<RefCell<HashMap<String, String>>>,
    needs_refresh: Rc<Cell<bool>>,
    keyboards_disagree: Rc<Cell<bool>>,
    settle_timer: Rc<RefCell<Option<glib::SourceId>>>,
    listeners: Rc<Listeners>,
}

impl Xkb {
    pub fn new(events: &Events) -> Self {
        let xkb = Xkb {
            layout_codes: Rc::new(RefCell::new(Vec::new())),
            current_name: Rc::new(RefCell::new(String::new())),
            current_code: Rc::new(RefCell::new(String::new())),
            cached_codes: Rc::new(RefCell::new(HashMap::new())),
            needs_refresh: Rc::new(Cell::new(false)),
            keyboards_disagree: Rc::new(Cell::new(false)),
            settle_timer: Rc::new(RefCell::new(None)),
            listeners: Rc::default(),
        };
        xkb.fetch_layouts();

        events
            .subscribe({
                let xkb = xkb.clone();
                move |event, data| match event {
                    "activelayout" => {
                        if xkb.needs_refresh.replace(false) {
                            xkb.fetch_layouts();
                        }
                        if xkb.layout_codes.borrow().len() <= 1 {
                            return;
                        }
                        let name = data.split_once(',').map_or("", |(_, name)| name);
                        if name != *xkb.current_name.borrow() {
                            xkb.restart_settle();
                        }
                    }
                    "configreloaded" => xkb.needs_refresh.set(true),
                    _ => {}
                }
            })
            .detach();
        xkb
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn cycle_layout(&self) {
        let codes = self.layout_codes.borrow().clone();
        if codes.len() <= 1 {
            return;
        }
        let current_index = codes
            .iter()
            .position(|code| *code == *self.current_code.borrow());
        self.apply_layout_index(current_index.map_or(0, |index| (index + 1) % codes.len()) as i32);
    }

    fn apply_layout_index(&self, index: i32) {
        if index < 0 || index as usize >= self.layout_codes.borrow().len() {
            return;
        }
        crate::core::process::detach(&["hyprctl", "switchxkblayout", "all", &index.to_string()]);
    }

    fn align_keyboards(&self) {
        if !self.keyboards_disagree.replace(false) {
            return;
        }
        let index = self
            .layout_codes
            .borrow()
            .iter()
            .position(|code| *code == *self.current_code.borrow())
            .map_or(-1, |index| index as i32);
        self.apply_layout_index(index);
    }

    fn restart_settle(&self) {
        if let Some(source) = self.settle_timer.borrow_mut().take() {
            source.remove();
        }
        let xkb = self.clone();
        let id = glib::timeout_add_local_once(SETTLE_DELAY, move || {
            xkb.settle_timer.replace(None);
            xkb.fetch_layouts();
        });
        self.settle_timer.replace(Some(id));
    }

    fn fetch_layouts(&self) {
        let Some(devices) = hypr::json("devices") else {
            return;
        };
        let keyboards = devices
            .get("keyboards")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let Some(main_keyboard) = keyboards
            .iter()
            .find(|keyboard| keyboard.get("main").and_then(Value::as_bool) == Some(true))
            .or_else(|| keyboards.first())
        else {
            return;
        };
        self.layout_codes.replace(
            main_keyboard
                .get("layout")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .split(',')
                .map(str::to_owned)
                .collect(),
        );

        let keymap = |keyboard: &Value| {
            keyboard
                .get("active_keymap")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        let name = keymap(main_keyboard);
        self.keyboards_disagree
            .set(keyboards.iter().any(|keyboard| keymap(keyboard) != name));
        let has_name_changed = *self.current_name.borrow() != name;
        self.current_name.replace(name);
        if has_name_changed {
            self.update_layout_code();
        } else {
            self.align_keyboards();
        }
        self.announce();
    }

    fn update_layout_code(&self) {
        let name = self.current_name.borrow().clone();
        let cached_code = self.cached_codes.borrow().get(&name).cloned();
        let code = cached_code.or_else(|| {
            let code = lookup_code(&name)?;
            self.cached_codes
                .borrow_mut()
                .insert(name.clone(), code.clone());
            Some(code)
        });
        if let Some(code) = code {
            self.current_code.replace(code);
        }
        self.align_keyboards();
        self.announce();
    }

    fn announce(&self) {
        self.listeners.notify();
    }
}

fn lookup_code(description: &str) -> Option<String> {
    let text = std::fs::read_to_string(BASE_LAYOUTS).ok()?;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('!') {
            continue;
        }
        let mut words = trimmed.splitn(2, char::is_whitespace);
        let key = words.next()?;
        let rest = words.next().unwrap_or_default().trim_start();
        if rest == description {
            return Some(key.to_owned());
        }
        let mut variant_words = rest.splitn(2, char::is_whitespace);
        let layout = variant_words.next().unwrap_or_default();
        let variant_description = variant_words.next().unwrap_or_default().trim_start();
        if !layout.is_empty() && variant_description == description {
            return Some(format!("{layout}{key}"));
        }
    }
    None
}
