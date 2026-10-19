use serde_json::Value;
use std::collections::HashMap;

use crate::platform::hyprland::hypr;
use crate::platform::hyprland::hyprconfig::{self, Area, lua_string, read_lua_string};

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
    pub is_mouse: bool,
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
    let mut keys_by_arg: Option<HashMap<String, String>> = None;
    let mut shortcuts: Vec<Shortcut> = Vec::new();
    for bind in &binds {
        let field = |name: &str| bind.get(name).and_then(Value::as_str).unwrap_or("");
        let description = field("description");
        if description.is_empty() || !field("submap").is_empty() {
            continue;
        }
        let mut key = field("key").to_owned();
        if key.is_empty() {
            let keys = keys_by_arg.get_or_insert_with(text_keys);
            key = keys.get(field("arg")).cloned().unwrap_or_default();
        }
        if key.is_empty() {
            continue;
        }
        let modmask = bind.get("modmask").and_then(Value::as_u64).unwrap_or(0) as u32;
        let keys = format_keys(modmask, &key);
        let is_mouse = bind.get("mouse").and_then(Value::as_bool).unwrap_or(false);
        let index = shortcuts
            .iter()
            .position(|shortcut| shortcut.description == description);
        let shortcut = match index {
            Some(index) => &mut shortcuts[index],
            None => {
                shortcuts.push(Shortcut {
                    description: description.to_owned(),
                    keys: [None, None],
                    is_mouse,
                });
                shortcuts.last_mut().unwrap()
            }
        };
        if shortcut
            .keys
            .iter()
            .flatten()
            .any(|existing_keys| are_same_keys(existing_keys, &keys))
        {
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

fn format_keys(modmask: u32, key: &str) -> String {
    let mut parts: Vec<&str> = MODIFIERS
        .iter()
        .filter(|(bit, _)| modmask & (1 << bit) != 0)
        .map(|(_, name)| *name)
        .collect();
    parts.push(key);
    parts.join(" + ")
}

fn canonicalize(keys: &str) -> (Vec<&'static str>, String) {
    let mut parts: Vec<&str> = keys.split('+').map(str::trim).collect();
    let key = parts.pop().unwrap_or("").to_lowercase();
    let modifier_tokens: Vec<String> = parts
        .iter()
        .map(|part| {
            let upper = part.to_uppercase();
            ALIASES
                .iter()
                .find(|(alias, _)| *alias == upper)
                .map_or(upper, |(_, name)| (*name).to_owned())
        })
        .collect();
    let modifiers = MODIFIERS
        .iter()
        .map(|(_, name)| *name)
        .filter(|name| modifier_tokens.iter().any(|modifier| modifier == name))
        .collect();
    (modifiers, key)
}

pub fn are_same_keys(a: &str, b: &str) -> bool {
    canonicalize(a) == canonicalize(b)
}

fn line(description: &str, keys: &[Option<String>; 2]) -> String {
    let to_lua_value =
        |keys: &Option<String>| keys.as_deref().map_or("false".to_owned(), lua_string);
    format!(
        "{CALL}{}, {}, {})",
        lua_string(description),
        to_lua_value(&keys[0]),
        to_lua_value(&keys[1])
    )
}

fn names(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix(CALL))
        .filter_map(|rest| read_lua_string(rest).map(|(name, _)| name))
        .collect()
}

fn with_shortcut(text: &str, description: &str, keys: Option<&[Option<String>; 2]>) -> String {
    let start = format!("{CALL}{}", lua_string(description));
    let mut remaining_lines: Vec<String> = text
        .lines()
        .filter(|line| !line.trim().starts_with(&format!("{start},")))
        .map(str::to_owned)
        .collect();
    while remaining_lines
        .last()
        .is_some_and(|line| line.trim().is_empty())
    {
        remaining_lines.pop();
    }
    if let Some(keys) = keys {
        remaining_lines.push(line(description, keys));
    }
    let mut new_text = remaining_lines.join("\n");
    new_text.push('\n');
    new_text
}

pub fn read_changed_descriptions() -> Vec<String> {
    names(&hyprconfig::read(Area::Binds))
}

pub fn store(changes: &[(String, Option<[Option<String>; 2]>)]) -> std::io::Result<()> {
    let mut text = hyprconfig::read_for_edit(Area::Binds)?;
    for (description, keys) in changes {
        text = with_shortcut(&text, description, keys.as_ref());
    }
    hyprconfig::write(Area::Binds, &text)?;
    hypr::request("reload");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::hyprland::workspacekeys::{self, BEGIN_LINE, END_LINE};

    #[test]
    fn editing_a_shortcut_keeps_the_workspace_keys_on_top_and_does_not_list_them_as_changed() {
        let keys_text = workspacekeys::with_keys("", 5, &[("DP-1".to_owned(), 1)]);
        let keys = [Some("SUPER + Z".to_owned()), None];
        let text = with_shortcut(&keys_text, "Workspace: Focus 1", Some(&keys));
        assert_eq!(
            text,
            format!("{keys_text}shortcut(\"Workspace: Focus 1\", \"SUPER + Z\", false)\n")
        );
        assert!(text.starts_with(BEGIN_LINE) && text.contains(&format!("\n{END_LINE}\n")));
        assert_eq!(names(&text), vec!["Workspace: Focus 1".to_owned()]);
        assert_eq!(
            workspacekeys::with_keys(&text, 5, &[("DP-1".to_owned(), 1)]),
            text
        );
        assert_eq!(with_shortcut(&text, "Workspace: Focus 1", None), keys_text);
    }

    #[test]
    fn combinations_match_whatever_the_modifier_order_and_key_case() {
        assert!(are_same_keys(
            "SHIFT + SUPER + ALT + Slash",
            "SUPER + ALT + SHIFT + slash"
        ));
        assert!(are_same_keys("CONTROL + super + q", "SUPER + CTRL + Q"));
        assert!(!are_same_keys("SUPER + Q", "SUPER + SHIFT + Q"));
        assert_eq!(format_keys(64 | 4 | 1, "Q"), "SUPER + CTRL + SHIFT + Q");
    }

    #[test]
    fn a_shortcut_line_is_replaced_removed_and_read_back_by_its_description() {
        let text = "hl.env(\"A\", \"b\")\n";
        let keys = [Some("SUPER + X".to_owned()), None];
        let text_with_shortcut = with_shortcut(text, "Window: Say \"hi\"", Some(&keys));
        assert_eq!(
            text_with_shortcut,
            "hl.env(\"A\", \"b\")\nshortcut(\"Window: Say \\\"hi\\\"\", \"SUPER + X\", false)\n"
        );
        assert_eq!(
            names(&text_with_shortcut),
            vec!["Window: Say \"hi\"".to_owned()]
        );
        let moved_keys = [None, Some("SUPER + Y".to_owned())];
        let replaced_text =
            with_shortcut(&text_with_shortcut, "Window: Say \"hi\"", Some(&moved_keys));
        assert_eq!(replaced_text.matches(CALL).count(), 1);
        assert!(replaced_text.contains("false, \"SUPER + Y\")"));
        assert_eq!(
            with_shortcut(&replaced_text, "Window: Say \"hi\"", None),
            text
        );
    }

    #[test]
    fn a_description_with_a_line_break_stays_on_one_line() {
        let keys = [Some("SUPER + X".to_owned()), None];
        let text_with_shortcut = with_shortcut("", "Two\nlines", Some(&keys));
        assert_eq!(
            text_with_shortcut,
            "shortcut(\"Two\\nlines\", \"SUPER + X\", false)\n"
        );
        assert_eq!(names(&text_with_shortcut), vec!["Two\nlines".to_owned()]);
        assert_eq!(with_shortcut(&text_with_shortcut, "Two\nlines", None), "\n");
    }
}
