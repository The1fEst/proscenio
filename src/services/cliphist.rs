use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::fuzzy::{self, Prepared};
use crate::core::levenshtein;
use crate::core::listeners::{Listeners, Subscription};
use crate::core::process::detach;

const BINARY: &str = "cliphist";
const SLOPPY_ENTRIES: usize = 100;
const PASTE_DELAY: f64 = 0.05;
const PRESS_PASTE: &str = "ydotool key -d 1 29:1 47:1 47:0 29:0";

#[derive(Clone)]
pub struct Cliphist {
    entries: Rc<RefCell<Vec<String>>>,
    prepared: Rc<RefCell<Vec<Prepared>>>,
    listeners: Rc<Listeners>,
    pending: Rc<RefCell<Option<glib::SourceId>>>,
    loaded: Rc<Cell<bool>>,
}

impl Cliphist {
    pub fn new() -> Self {
        let cliphist = Cliphist {
            entries: Rc::new(RefCell::new(Vec::new())),
            prepared: Rc::new(RefCell::new(Vec::new())),
            listeners: Rc::default(),
            pending: Rc::new(RefCell::new(None)),
            loaded: Rc::new(Cell::new(false)),
        };
        if let Some(display) = gtk4::gdk::Display::default() {
            let again = cliphist.clone();
            display.clipboard().connect_changed(move |_| {
                if again.loaded.get() {
                    again.refresh_soon();
                }
            });
        }
        cliphist
    }

    pub fn load(&self) {
        self.loaded.set(true);
        self.refresh();
    }

    pub fn release(&self) {
        self.loaded.set(false);
        if let Some(source) = self.pending.borrow_mut().take() {
            source.remove();
        }
        self.entries.replace(Vec::new());
        self.prepared.replace(Vec::new());
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn fuzzy_query(&self, search: &str) -> Vec<String> {
        let entries = self.entries.borrow();
        if search.trim().is_empty() {
            return entries.clone();
        }
        let found = if levenshtein::sloppy() {
            let texts = entries.iter().take(SLOPPY_ENTRIES).map(String::as_str);
            levenshtein::rank(texts, search, levenshtein::text_match_score)
        } else {
            let prepared = self.prepared.borrow();
            let references: Vec<&Prepared> = prepared.iter().collect();
            fuzzy::go(search, &references)
        };
        found
            .into_iter()
            .filter_map(|index| entries.get(index).cloned())
            .collect()
    }

    fn refresh_soon(&self) {
        if let Some(source) = self.pending.borrow_mut().take() {
            source.remove();
        }
        let again = self.clone();
        let delay = Duration::from_millis(crate::core::config::current().race_condition_delay);
        let source = glib::timeout_add_local_once(delay, move || {
            again.pending.borrow_mut().take();
            again.refresh();
        });
        self.pending.replace(Some(source));
    }

    pub fn refresh(&self) {
        let again = self.clone();
        glib::spawn_future_local(async move {
            let listed = gio::spawn_blocking(|| {
                let output = std::process::Command::new(BINARY)
                    .arg("list")
                    .stderr(std::process::Stdio::null())
                    .output()
                    .ok()
                    .filter(|output| output.status.success())?;
                let stdout = String::from_utf8(output.stdout).ok()?;
                let entries: Vec<String> = stdout.lines().map(str::to_owned).collect();
                let prepared: Vec<Prepared> = entries
                    .iter()
                    .map(|entry| fuzzy::prepare(without_first_word(entry)))
                    .collect();
                Some((entries, prepared))
            })
            .await;
            let Ok(Some((entries, prepared))) = listed else {
                return;
            };
            if !again.loaded.get() {
                return;
            }
            again.entries.replace(entries);
            again.prepared.replace(prepared);
            again.listeners.notify();
        });
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
        self.after(&format!("echo '{}' | {BINARY} delete", escape(entry)));
    }

    pub fn wipe(&self) {
        self.after(&format!("{BINARY} wipe"));
    }

    fn after(&self, line: &str) {
        let Ok(process) = gio::Subprocess::newv(
            &[
                std::ffi::OsStr::new("bash"),
                std::ffi::OsStr::new("-c"),
                std::ffi::OsStr::new(line),
            ],
            gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
        ) else {
            return;
        };
        let again = self.clone();
        process.wait_async(gio::Cancellable::NONE, move |_| again.refresh());
    }

    pub fn superpaste(&self, count: usize, images: bool) {
        let chosen: Vec<String> = self
            .entries
            .borrow()
            .iter()
            .filter(|entry| !images || is_image(entry))
            .take(count)
            .cloned()
            .collect();
        let steps: Vec<String> = chosen
            .iter()
            .rev()
            .map(|entry| {
                format!(
                    "printf '%s' '{}' | {BINARY} decode | wl-copy && sleep {PASTE_DELAY} && {PRESS_PASTE}",
                    escape(entry)
                )
            })
            .collect();
        detach(&[
            "bash",
            "-c",
            &steps.join(&format!(" && sleep {PASTE_DELAY} && ")),
        ]);
    }
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
    let Some(at) = inner.find("binary data") else {
        return false;
    };
    has_size(&inner[at + "binary data".len()..])
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
    let at = bytes.iter().enumerate().position(|(index, &byte)| {
        byte == b'x'
            && index > 0
            && bytes[index - 1].is_ascii_digit()
            && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
    })?;
    let start = bytes[..at]
        .iter()
        .rposition(|byte| !byte.is_ascii_digit())
        .map_or(0, |index| index + 1);
    let end = bytes[at + 1..]
        .iter()
        .position(|byte| !byte.is_ascii_digit())
        .map_or(bytes.len(), |index| at + 1 + index);
    Some((
        entry[start..at].parse().ok()?,
        entry[at + 1..end].parse().ok()?,
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
        Some(at) => trimmed[at..].trim_start(),
        None => entry,
    }
}

pub fn escape(text: &str) -> String {
    text.replace('\'', "'\\''")
}

#[cfg(test)]
mod tests {
    use super::{clean, image_size, is_image, without_first_word};

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
