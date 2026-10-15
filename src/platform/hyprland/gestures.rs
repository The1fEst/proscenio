use std::path::PathBuf;

use crate::platform::hyprland::hyprconfig::{self, Area};

pub const UNSET: &str = "unset";

#[derive(Clone, Debug, PartialEq)]
pub struct Gesture {
    pub fingers: u32,
    pub direction: String,
    pub action: String,
    pub mods: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GestureEntry {
    pub gesture: Gesture,
    pub is_default: bool,
}

pub fn defaults_path() -> PathBuf {
    gtk4::glib::user_config_dir().join("hypr/hyprland/general.lua")
}

pub fn parse_direction(text: &str) -> Option<&'static str> {
    Some(match text.to_lowercase().as_str() {
        "swipe" => "swipe",
        "left" | "l" => "left",
        "right" | "r" => "right",
        "up" | "u" | "top" | "t" => "up",
        "down" | "d" | "bottom" | "b" => "down",
        "horizontal" | "horiz" => "horizontal",
        "vertical" | "vert" => "vertical",
        "pinch" => "pinch",
        "pinchin" | "zoomin" => "pinchin",
        "pinchout" | "zoomout" => "pinchout",
        _ => return None,
    })
}

fn axis(direction: &str) -> &str {
    match direction {
        "up" | "down" | "vertical" => "vertical",
        "left" | "right" | "horizontal" => "horizontal",
        "pinch" | "pinchin" | "pinchout" => "pinch",
        other => other,
    }
}

fn skip_space(text: &str) -> &str {
    text.trim_start_matches([' ', '\t', '\n', '\r'])
}

fn field<'a>(block: &'a str, key: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(at) = block[from..].find(key).map(|at| at + from) {
        from = at + key.len();
        let before = block[..at].chars().next_back();
        if before.is_some_and(|character| character.is_alphanumeric() || character == '_') {
            continue;
        }
        let rest = skip_space(&block[from..]);
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        return Some(skip_space(rest));
    }
    None
}

fn unquote(value: &str) -> Option<&str> {
    let quote = value
        .chars()
        .next()
        .filter(|quote| *quote == '"' || *quote == '\'')?;
    let rest = &value[1..];
    Some(&rest[..rest.find(quote)?])
}

fn parse_block(block: &str) -> Option<Gesture> {
    let fingers = field(block, "fingers")?;
    let digits = &fingers[..fingers
        .find(|character: char| !character.is_ascii_digit())
        .unwrap_or(fingers.len())];
    let action = field(block, "action")?;
    let action = if action.starts_with("function") || action.starts_with('{') {
        String::new()
    } else {
        unquote(action)?.to_owned()
    };
    Some(Gesture {
        fingers: digits.parse().ok()?,
        direction: parse_direction(unquote(field(block, "direction")?)?)?.to_owned(),
        action,
        mods: field(block, "mods")
            .and_then(unquote)
            .unwrap_or_default()
            .to_owned(),
    })
}

fn call_end(text: &str, open: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut index = open;
    while index < bytes.len() {
        match bytes[index] {
            b'(' | b'{' => depth += 1,
            b')' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index + 1);
                }
            }
            quote @ (b'"' | b'\'') => {
                index += 1;
                while index < bytes.len() && bytes[index] != quote {
                    index += if bytes[index] == b'\\' { 2 } else { 1 };
                }
            }
            b'-' if bytes.get(index + 1) == Some(&b'-') => {
                index = text[index..]
                    .find('\n')
                    .map_or(bytes.len(), |end| index + end);
            }
            _ => {}
        }
        index += 1;
    }
    None
}

pub fn parse(text: &str) -> Vec<Gesture> {
    let mut gestures = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed_line = line.trim_start();
        if !trimmed_line.starts_with("hl.gesture") {
            continue;
        }
        let open = start + (line.len() - trimmed_line.len()) + "hl.gesture".len();
        let Some(end) = call_end(text, open) else {
            continue;
        };
        if let Some(gesture) = parse_block(&text[open..end]) {
            gestures.push(gesture);
        }
    }
    gestures
}

fn is_same_slot(gesture: &Gesture, other: &Gesture) -> bool {
    gesture.fingers == other.fingers
        && gesture.direction == other.direction
        && gesture.mods == other.mods
}

pub fn parse_entries(defaults: &str, settings: &str) -> Vec<GestureEntry> {
    let mut entries: Vec<GestureEntry> = parse(defaults)
        .into_iter()
        .filter(|gesture| gesture.action != UNSET)
        .map(|gesture| GestureEntry {
            gesture,
            is_default: true,
        })
        .collect();
    for gesture in parse(settings) {
        if gesture.action == UNSET {
            if let Some(index) = entries
                .iter()
                .position(|entry| is_same_slot(&entry.gesture, &gesture))
            {
                entries.remove(index);
            }
        } else {
            entries.push(GestureEntry {
                gesture,
                is_default: false,
            });
        }
    }
    entries
}

pub fn find_conflict<'a>(entries: &'a [GestureEntry], gesture: &Gesture) -> Option<&'a Gesture> {
    let gesture_axis = axis(&gesture.direction);
    entries.iter().map(|entry| &entry.gesture).find(|existing| {
        existing.fingers == gesture.fingers
            && existing.mods == gesture.mods
            && (existing.direction == gesture_axis
                || existing.direction == gesture.direction
                || (matches!(gesture_axis, "vertical" | "horizontal")
                    && existing.direction == "swipe"))
    })
}

pub fn line_for(gesture: &Gesture) -> String {
    let mods = if gesture.mods.is_empty() {
        String::new()
    } else {
        format!(", mods = \"{}\"", gesture.mods)
    };
    format!(
        "hl.gesture({{ fingers = {}, direction = \"{}\"{mods}, action = \"{}\" }})",
        gesture.fingers, gesture.direction, gesture.action
    )
}

fn without_line(text: &str, line: &str) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let index = lines.iter().position(|existing| existing.trim() == line)?;
    let mut remaining_lines = lines;
    remaining_lines.remove(index);
    let mut new_text = remaining_lines.join("\n");
    new_text.push('\n');
    Some(new_text)
}

fn with_line_appended(text: &str, line: &str) -> String {
    format!("{}\n{line}\n", text.trim_end_matches('\n'))
}

pub fn with_gesture_removed(settings: &str, entry: &GestureEntry) -> String {
    if !entry.is_default
        && let Some(text) = without_line(settings, &line_for(&entry.gesture))
    {
        return text;
    }
    let unset_gesture = Gesture {
        action: UNSET.to_owned(),
        ..entry.gesture.clone()
    };
    with_line_appended(settings, &line_for(&unset_gesture))
}

pub fn with_gesture_added(settings: &str, defaults: &str, gesture: &Gesture) -> String {
    let unset_line = line_for(&Gesture {
        action: UNSET.to_owned(),
        ..gesture.clone()
    });
    if parse(defaults).contains(gesture)
        && let Some(text) = without_line(settings, &unset_line)
    {
        return text;
    }
    with_line_appended(settings, &line_for(gesture))
}

pub fn read() -> (String, std::io::Result<String>) {
    (
        std::fs::read_to_string(defaults_path()).unwrap_or_default(),
        hyprconfig::read_for_edit(Area::Mouse),
    )
}

pub fn write(settings: &str) -> std::io::Result<()> {
    hyprconfig::write(Area::Mouse, settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULTS: &str = "hl.monitor({ output = \"\" })\n\nhl.gesture({\n\tfingers = 3,\n\tdirection = \"swipe\",\n\taction = \"move\",\n})\n-- hl.gesture({ fingers = 5, direction = \"up\", action = \"close\" })\nhl.gesture({\n\tfingers = 4,\n\tdirection = \"up\",\n\taction = function()\n\t\thl.dispatch(hl.dsp.global(\"quickshell:overviewWorkspacesToggle\"))\n\tend,\n})\nhl.gesture({ fingers = 4, direction = \"horiz\", action = \"workspace\" })\n";

    fn gesture(fingers: u32, direction: &str, action: &str) -> Gesture {
        Gesture {
            fingers,
            direction: direction.to_owned(),
            action: action.to_owned(),
            mods: String::new(),
        }
    }

    #[test]
    fn gestures_are_read_across_lines_with_functions_and_aliases() {
        assert_eq!(
            parse(DEFAULTS),
            [
                gesture(3, "swipe", "move"),
                gesture(4, "up", ""),
                gesture(4, "horizontal", "workspace"),
            ]
        );
    }

    #[test]
    fn settings_unset_defaults_and_add_their_own() {
        let settings = "hl.config({ general = { gaps_in = 4 } })\nhl.gesture({ fingers = 3, direction = \"swipe\", action = \"unset\" })\nhl.gesture({ fingers = 3, direction = \"pinchout\", action = \"fullscreen\" })\n";
        let entries = parse_entries(DEFAULTS, settings);
        let entry_pairs: Vec<(Gesture, bool)> = entries
            .iter()
            .map(|entry| (entry.gesture.clone(), entry.is_default))
            .collect();
        assert_eq!(
            entry_pairs,
            [
                (gesture(4, "up", ""), true),
                (gesture(4, "horizontal", "workspace"), true),
                (gesture(3, "pinchout", "fullscreen"), false),
            ]
        );
    }

    #[test]
    fn a_gesture_on_a_taken_axis_conflicts_as_hyprland_decides() {
        let entries = parse_entries(DEFAULTS, "");
        assert_eq!(
            find_conflict(&entries, &gesture(3, "left", "close")),
            Some(&gesture(3, "swipe", "move"))
        );
        assert_eq!(
            find_conflict(&entries, &gesture(4, "right", "close")),
            Some(&gesture(4, "horizontal", "workspace"))
        );
        assert_eq!(find_conflict(&entries, &gesture(4, "down", "close")), None);
        assert_eq!(find_conflict(&entries, &gesture(3, "pinch", "close")), None);
    }

    #[test]
    fn removing_and_adding_back_a_default_round_trips() {
        let entries = parse_entries(DEFAULTS, "");
        let first_entry = entries[0].clone();
        let settings_without_default = with_gesture_removed("", &first_entry);
        assert_eq!(
            settings_without_default,
            "\nhl.gesture({ fingers = 3, direction = \"swipe\", action = \"unset\" })\n"
        );
        assert_eq!(parse_entries(DEFAULTS, &settings_without_default).len(), 2);
        let settings_restored =
            with_gesture_added(&settings_without_default, DEFAULTS, &first_entry.gesture);
        assert_eq!(parse_entries(DEFAULTS, &settings_restored), entries);

        let own_gesture = gesture(5, "down", "close");
        let settings_with_own = with_gesture_added("", DEFAULTS, &own_gesture);
        let own_entries = parse_entries(DEFAULTS, &settings_with_own);
        assert_eq!(
            with_gesture_removed(&settings_with_own, own_entries.last().unwrap()),
            "\n"
        );
    }
}
