use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::fuzzy::{self, SearchTarget};
use crate::core::levenshtein;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::{self, detach};

const BINARY: &str = "cliphist";
const SLOPPY_ENTRIES: usize = 100;
const PASTE_DELAY_SECONDS: f64 = 0.05;
const PRESS_PASTE: &str = "ydotool key -d 1 29:1 47:1 47:0 29:0";

#[derive(Clone)]
pub struct Cliphist {
    entries: Rc<RefCell<Vec<String>>>,
    search_targets: Rc<RefCell<Vec<SearchTarget>>>,
    listeners: Rc<Listeners>,
    refresh_timer: Rc<RefCell<Option<glib::SourceId>>>,
    is_loaded: Rc<Cell<bool>>,
    refresh_generation: Rc<Cell<u64>>,
    applied_generation: Rc<Cell<u64>>,
}

impl Cliphist {
    pub fn new() -> Self {
        let cliphist = Cliphist {
            entries: Rc::new(RefCell::new(Vec::new())),
            search_targets: Rc::new(RefCell::new(Vec::new())),
            listeners: Rc::default(),
            refresh_timer: Rc::new(RefCell::new(None)),
            is_loaded: Rc::new(Cell::new(false)),
            refresh_generation: Rc::default(),
            applied_generation: Rc::default(),
        };
        if let Some(display) = gtk4::gdk::Display::default() {
            display.clipboard().connect_changed({
                let cliphist = cliphist.clone();
                move |_| {
                    if cliphist.is_loaded.get() {
                        cliphist.refresh_soon();
                    }
                }
            });
        }
        cliphist
    }

    pub fn load(&self) {
        self.is_loaded.set(true);
        self.refresh();
    }

    pub fn release(&self) {
        self.is_loaded.set(false);
        if let Some(source) = self.refresh_timer.borrow_mut().take() {
            source.remove();
        }
        self.entries.replace(Vec::new());
        self.search_targets.replace(Vec::new());
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn fuzzy_query(&self, search: &str) -> Vec<String> {
        let entries = self.entries.borrow();
        if search.trim().is_empty() {
            return entries.clone();
        }
        let ranked_indices = if levenshtein::is_enabled() {
            let texts = entries.iter().take(SLOPPY_ENTRIES).map(String::as_str);
            levenshtein::rank(texts, search, levenshtein::text_match_score)
        } else {
            let search_targets = self.search_targets.borrow();
            let references: Vec<&SearchTarget> = search_targets.iter().collect();
            fuzzy::go(search, &references)
        };
        ranked_indices
            .into_iter()
            .filter_map(|index| entries.get(index).cloned())
            .collect()
    }

    fn refresh_soon(&self) {
        if let Some(source) = self.refresh_timer.borrow_mut().take() {
            source.remove();
        }
        let cliphist = self.clone();
        let delay = Duration::from_millis(crate::core::config::current().race_condition_delay);
        let source = glib::timeout_add_local_once(delay, move || {
            cliphist.refresh_timer.borrow_mut().take();
            cliphist.refresh();
        });
        self.refresh_timer.replace(Some(source));
    }

    pub fn refresh(&self) {
        let generation = self.start_refresh();
        let cliphist = self.clone();
        glib::spawn_future_local(async move {
            let list_result = gio::spawn_blocking(|| read_list(&[BINARY, "list"])).await;
            if let Ok(Some((entries, search_targets))) = list_result {
                cliphist.apply_list(generation, entries, search_targets);
            }
        });
    }

    fn start_refresh(&self) -> u64 {
        let generation = self.refresh_generation.get() + 1;
        self.refresh_generation.set(generation);
        generation
    }

    fn apply_list(&self, generation: u64, entries: Vec<String>, search_targets: Vec<SearchTarget>) {
        if generation <= self.applied_generation.get() || !self.is_loaded.get() {
            return;
        }
        self.applied_generation.set(generation);
        self.entries.replace(entries);
        self.search_targets.replace(search_targets);
        self.listeners.notify();
    }

    pub fn copy(&self, entry: &str) {
        detach(&[
            "bash",
            "-c",
            &format!(
                "printf '%s' '{}' | {BINARY} decode | wl-copy",
                escape(entry)
            ),
        ]);
    }

    pub fn delete_entry(&self, entry: &str) {
        self.run_then_refresh(&format!("echo '{}' | {BINARY} delete", escape(entry)));
    }

    pub fn wipe(&self) {
        self.run_then_refresh(&format!("{BINARY} wipe"));
    }

    fn run_then_refresh(&self, script: &str) {
        let command = process::quiet(&["bash", "-c", script]);
        let cliphist = self.clone();
        glib::spawn_future_local(async move {
            if process::finish(command).await.is_some() {
                cliphist.refresh();
            }
        });
    }

    pub fn superpaste(&self, count: usize, is_image_only: bool) {
        let entries: Vec<String> = self
            .entries
            .borrow()
            .iter()
            .filter(|entry| !is_image_only || is_image(entry))
            .take(count)
            .cloned()
            .collect();
        let steps: Vec<String> = entries
            .iter()
            .rev()
            .map(|entry| {
                format!(
                    "printf '%s' '{}' | {BINARY} decode | wl-copy && sleep {PASTE_DELAY_SECONDS} && {PRESS_PASTE}",
                    escape(entry)
                )
            })
            .collect();
        detach(&[
            "bash",
            "-c",
            &steps.join(&format!(" && sleep {PASTE_DELAY_SECONDS} && ")),
        ]);
    }
}

fn read_list(command: &[&str]) -> Option<(Vec<String>, Vec<SearchTarget>)> {
    let entries: Vec<String> = process::output(command)?
        .lines()
        .map(str::to_owned)
        .collect();
    let search_targets = entries
        .iter()
        .map(|entry| fuzzy::prepare(without_first_word(entry)))
        .collect();
    Some((entries, search_targets))
}

pub fn is_image(entry: &str) -> bool {
    let Some((number, rest)) = entry.split_once('\t') else {
        return false;
    };
    if number.is_empty() || !number.chars().all(|digit| digit.is_ascii_digit()) {
        return false;
    }
    let Some(inner) = rest
        .strip_prefix("[[")
        .and_then(|rest| rest.strip_suffix("]]"))
    else {
        return false;
    };
    let Some(index) = inner.find("binary data") else {
        return false;
    };
    has_size(&inner[index + "binary data".len()..])
}

fn has_size(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().any(|(index, &byte)| {
        byte == b'x'
            && index > 0
            && bytes[index - 1].is_ascii_digit()
            && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
    })
}

pub fn image_size(entry: &str) -> Option<(u32, u32)> {
    let bytes = entry.as_bytes();
    let x_index = bytes.iter().enumerate().position(|(index, &byte)| {
        byte == b'x'
            && index > 0
            && bytes[index - 1].is_ascii_digit()
            && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
    })?;
    let start = bytes[..x_index]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |index| index + 1);
    let end = bytes[x_index + 1..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map_or(bytes.len(), |index| x_index + 1 + index);
    Some((
        entry[start..x_index].parse().ok()?,
        entry[x_index + 1..end].parse().ok()?,
    ))
}

pub fn entry_number(entry: &str) -> Option<u64> {
    entry.split_once('\t')?.0.parse().ok()
}

pub fn clean(entry: &str) -> &str {
    match entry.split_once('\t') {
        Some((number, rest))
            if !number.is_empty() && number.chars().all(|digit| digit.is_ascii_digit()) =>
        {
            rest
        }
        _ => entry,
    }
}

pub fn without_first_word(entry: &str) -> &str {
    let trimmed = entry.trim_start();
    match trimmed.find(char::is_whitespace) {
        Some(index) => trimmed[index..].trim_start(),
        None => entry,
    }
}

pub fn escape(text: &str) -> String {
    text.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_list_read_before_a_newer_refresh_does_not_replace_the_newer_list() {
        let cliphist = Cliphist {
            entries: Rc::default(),
            search_targets: Rc::default(),
            listeners: Rc::default(),
            refresh_timer: Rc::default(),
            is_loaded: Rc::new(Cell::new(true)),
            refresh_generation: Rc::default(),
            applied_generation: Rc::default(),
        };
        let apply = |generation: u64, entry: &str| {
            let search_target = fuzzy::prepare(without_first_word(entry));
            cliphist.apply_list(generation, vec![entry.to_owned()], vec![search_target]);
        };
        let older = cliphist.start_refresh();
        let newer = cliphist.start_refresh();
        apply(newer, "2\tkept");
        apply(older, "1\tdeleted");
        assert_eq!(*cliphist.entries.borrow(), ["2\tkept"]);

        let older = cliphist.start_refresh();
        let newer = cliphist.start_refresh();
        apply(older, "3\tolder");
        assert_eq!(*cliphist.entries.borrow(), ["3\tolder"]);
        apply(newer, "4\tnewer");
        assert_eq!(*cliphist.entries.borrow(), ["4\tnewer"]);
    }

    #[test]
    fn an_entry_that_is_not_utf8_keeps_its_number_and_the_others() {
        let (entries, _) = read_list(&["printf", "2\\tplain\\n1\\tcaf\\351 latin1\\n"]).unwrap();
        assert_eq!(entries, ["2\tplain", "1\tcaf\u{FFFD} latin1"]);
        assert_eq!(entry_number(&entries[1]), Some(1));
    }

    #[test]
    fn recognises_image_entries_as_cliphist_lists_them() {
        assert!(is_image("12\t[[ binary data 34 KiB png 640x480 ]]"));
        assert!(!is_image("12\tplain text 640x480"));
        assert!(!is_image("[[ binary data 34 KiB png 640x480 ]]"));
        assert_eq!(
            image_size("12\t[[ binary data 34 KiB png 640x480 ]]"),
            Some((640, 480))
        );
    }

    #[test]
    fn strips_the_entry_number() {
        assert_eq!(clean("42\thello world"), "hello world");
        assert_eq!(without_first_word("42\thello world"), "hello world");
    }
}
