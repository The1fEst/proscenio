use serde_json::Value;
use std::collections::HashMap;

use crate::platform::{hypr, hyprconfig};

const MODIFIERS: [(u32, &str); 8] = [
    (6, "SUPER"),
    (2, "CTRL"),
    (3, "ALT"),
    (0, "SHIFT"),
    (1, "CAPS"),
    (4, "MOD2"),
    (5, "MOD3"),
    (7, "MOD5"),
];
const ALIASES: [(&str, &str); 5] = [
    ("CONTROL", "CTRL"),
    ("MOD1", "ALT"),
    ("MOD4", "SUPER"),
    ("WIN", "SUPER"),
    ("LOGO", "SUPER"),
];
const CALL: &str = "shortcut(";

#[derive(Clone, Debug, PartialEq)]
pub struct Shortcut {
    pub description: String,
    pub keys: [Option<String>; 2],
    pub mouse: bool,
}

impl Shortcut {
    pub fn category(&self) -> &str {
        self.description
            .find(':')
            .map_or("", |end| &self.description[..end])
    }

    pub fn label(&self) -> &str {
        self.description
            .find(':')
            .map_or(self.description.as_str(), |end| {
                self.description[end + 1..].trim()
            })
    }
}

pub fn load() -> Vec<Shortcut> {
    let binds = hypr::json("binds")
        .and_then(|binds| binds.as_array().cloned())
        .unwrap_or_default();
    let mut texts: Option<HashMap<String, String>> = None;
    let mut shortcuts: Vec<Shortcut> = Vec::new();
    for bind in &binds {
        let text = |name: &str| bind.get(name).and_then(Value::as_str).unwrap_or("");
        let description = text("description");
        if description.is_empty() || !text("submap").is_empty() {
            continue;
        }
        let mut key = text("key").to_owned();
        if key.is_empty() {
            let keys = texts.get_or_insert_with(text_keys);
            key = keys.get(text("arg")).cloned().unwrap_or_default();
        }
        if key.is_empty() {
            continue;
        }
        let modmask = bind.get("modmask").and_then(Value::as_u64).unwrap_or(0) as u32;
        let keys = written(modmask, &key);
        let mouse = bind.get("mouse").and_then(Value::as_bool).unwrap_or(false);
        let index = shortcuts
            .iter()
            .position(|shortcut| shortcut.description == description);
        let shortcut = match index {
            Some(index) => &mut shortcuts[index],
            None => {
                shortcuts.push(Shortcut {
                    description: description.to_owned(),
                    keys: [None, None],
                    mouse,
                });
                shortcuts.last_mut().unwrap()
            }
        };
        if shortcut.keys.iter().flatten().any(|held| same(held, &keys)) {
            continue;
        }
        if let Some(slot) = shortcut.keys.iter_mut().find(|slot| slot.is_none()) {
            *slot = Some(keys);
        }
    }
    shortcuts
}

fn text_keys() -> HashMap<String, String> {
    let mut keys = HashMap::new();
    let mut key = String::new();
    for line in hypr::request("binds").unwrap_or_default().lines() {
        let line = line.trim();
        if let Some(value) = line.strip_prefix("key: ") {
            key = value.rsplit('+').next().unwrap_or("").trim().to_owned();
        } else if let Some(value) = line.strip_prefix("arg: ") {
            keys.insert(value.to_owned(), std::mem::take(&mut key));
        }
    }
    keys
}

fn written(modmask: u32, key: &str) -> String {
    let mut parts: Vec<&str> = MODIFIERS
        .iter()
        .filter(|(bit, _)| modmask & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect();
    parts.push(key);
    parts.join(" + ")
}

fn canonical(keys: &str) -> (Vec<&'static str>, String) {
    let mut parts: Vec<&str> = keys.split('+').map(str::trim).collect();
    let key = parts.pop().unwrap_or("").to_lowercase();
    let held: Vec<String> = parts
        .iter()
        .map(|part| {
            let upper = part.to_uppercase();
            ALIASES
                .iter()
                .find(|(alias, _)| *alias == upper)
                .map_or(upper, |(_, name)| (*name).to_owned())
        })
        .collect();
    let mods = MODIFIERS
        .iter()
        .map(|(_, name)| *name)
        .filter(|name| held.iter().any(|held| held == name))
        .collect();
    (mods, key)
}

pub fn same(a: &str, b: &str) -> bool {
    canonical(a) == canonical(b)
}

fn quoted(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

fn line(description: &str, keys: &[Option<String>; 2]) -> String {
    let slot = |keys: &Option<String>| keys.as_deref().map_or("false".to_owned(), quoted);
    format!(
        "{CALL}{}, {}, {})",
        quoted(description),
        slot(&keys[0]),
        slot(&keys[1])
    )
}

fn names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(CALL))
        .filter_map(|rest| {
            let rest = rest.strip_prefix('"')?;
            let mut name = String::new();
            let mut chars = rest.chars();
            while let Some(char) = chars.next() {
                match char {
                    '\\' => name.push(chars.next()?),
                    '"' => return Some(name),
                    other => name.push(other),
                }
            }
            None
        })
        .collect()
}

fn with_shortcut(text: &str, description: &str, keys: Option<&[Option<String>; 2]>) -> String {
    let start = format!("{CALL}{}", quoted(description));
    let mut kept: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().starts_with(&format!("{start},")))
        .map(str::to_owned)
        .collect();
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    if let Some(keys) = keys {
        kept.push(line(description, keys));
    }
    let mut joined = kept.join("\n");
    joined.push('\n');
    joined
}

pub fn changed() -> Vec<String> {
    names(&std::fs::read_to_string(hyprconfig::settings_path()).unwrap_or_default())
}

pub fn store(changes: &[(String, Option<[Option<String>; 2]>)]) -> std::io::Result<()> {
    let path = hyprconfig::settings_path();
    let mut text = std::fs::read_to_string(&path).unwrap_or_default();
    for (description, keys) in changes {
        text = with_shortcut(&text, description, keys.as_ref());
    }
    std::fs::write(path, text)?;
    hypr::request("reload");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combinations_match_whatever_the_modifier_order_and_key_case() {
        assert!(same(
            "SHIFT + SUPER + ALT + Slash",
            "SUPER + ALT + SHIFT + slash"
        ));
        assert!(same("CONTROL + super + q", "SUPER + CTRL + Q"));
        assert!(!same("SUPER + Q", "SUPER + SHIFT + Q"));
        assert_eq!(written(64 | 4 | 1, "Q"), "SUPER + CTRL + SHIFT + Q");
    }

    #[test]
    fn a_shortcut_line_is_replaced_removed_and_read_back_by_its_description() {
        let text = "hl.env(\"A\", \"b\")\n";
        let keys = [Some("SUPER + X".to_owned()), None];
        let set = with_shortcut(text, "Window: Say \"hi\"", Some(&keys));
        assert_eq!(
            set,
            "hl.env(\"A\", \"b\")\nshortcut(\"Window: Say \\\"hi\\\"\", \"SUPER + X\", false)\n"
        );
        assert_eq!(names(&set), vec!["Window: Say \"hi\"".to_owned()]);
        let moved = [None, Some("SUPER + Y".to_owned())];
        let replaced = with_shortcut(&set, "Window: Say \"hi\"", Some(&moved));
        assert_eq!(replaced.matches(CALL).count(), 1);
        assert!(replaced.contains("false, \"SUPER + Y\")"));
        assert_eq!(with_shortcut(&replaced, "Window: Say \"hi\"", None), text);
    }
}
