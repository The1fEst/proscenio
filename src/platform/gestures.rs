use std::path::PathBuf;

use crate::platform::hyprconfig::{self, Area};

pub const UNSET: &str = "unset";

#[derive(Clone, Debug, PartialEq)]
pub struct Gesture {
    pub fingers: u32,
    pub direction: String,
    pub action: String,
    pub mods: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Listed {
    pub gesture: Gesture,
    pub default: bool,
}

pub fn defaults_path() -> PathBuf {
    gtk4::glib::user_config_dir().join("hypr/hyprland/general.lua")
}

pub fn direction(text: &str) -> Option<&'static str> {
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

fn quoted(value: &str) -> Option<&str> {
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
        quoted(action)?.to_owned()
    };
    Some(Gesture {
        fingers: digits.parse().ok()?,
        direction: direction(quoted(field(block, "direction")?)?)?.to_owned(),
        action,
        mods: field(block, "mods")
            .and_then(quoted)
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
    let mut found = Vec::new();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        let start = offset;
        offset += line.len();
        let trimmed = line.trim_start();
        if !trimmed.starts_with("hl.gesture") {
            continue;
        }
        let open = start + (line.len() - trimmed.len()) + "hl.gesture".len();
        let Some(end) = call_end(text, open) else {
            continue;
        };
        if let Some(gesture) = parse_block(&text[open..end]) {
            found.push(gesture);
        }
    }
    found
}

fn same_slot(one: &Gesture, other: &Gesture) -> bool {
    one.fingers == other.fingers && one.direction == other.direction && one.mods == other.mods
}

pub fn effective(defaults: &str, settings: &str) -> Vec<Listed> {
    let mut list: Vec<Listed> = parse(defaults)
        .into_iter()
        .filter(|gesture| gesture.action != UNSET)
        .map(|gesture| Listed {
            gesture,
            default: true,
        })
        .collect();
    for gesture in parse(settings) {
        if gesture.action == UNSET {
            if let Some(index) = list
                .iter()
                .position(|listed| same_slot(&listed.gesture, &gesture))
            {
                list.remove(index);
            }
        } else {
            list.push(Listed {
                gesture,
                default: false,
            });
        }
    }
    list
}

pub fn conflict<'a>(list: &'a [Listed], wanted: &Gesture) -> Option<&'a Gesture> {
    let wanted_axis = axis(&wanted.direction);
    list.iter().map(|listed| &listed.gesture).find(|known| {
        known.fingers == wanted.fingers
            && known.mods == wanted.mods
            && (known.direction == wanted_axis
                || known.direction == wanted.direction
                || (matches!(wanted_axis, "vertical" | "horizontal") && known.direction == "swipe"))
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
    let index = lines.iter().position(|known| known.trim() == line)?;
    let mut kept = lines;
    kept.remove(index);
    let mut joined = kept.join("\n");
    joined.push('\n');
    Some(joined)
}

fn appended(text: &str, line: &str) -> String {
    format!("{}\n{line}\n", text.trim_end_matches('\n'))
}

pub fn removed(settings: &str, listed: &Listed) -> String {
    if !listed.default
        && let Some(text) = without_line(settings, &line_for(&listed.gesture))
    {
        return text;
    }
    let unset = Gesture {
        action: UNSET.to_owned(),
        ..listed.gesture.clone()
    };
    appended(settings, &line_for(&unset))
}

pub fn added(settings: &str, defaults: &str, gesture: &Gesture) -> String {
    let unset = line_for(&Gesture {
        action: UNSET.to_owned(),
        ..gesture.clone()
    });
    if parse(defaults).contains(gesture)
        && let Some(text) = without_line(settings, &unset)
    {
        return text;
    }
    appended(settings, &line_for(gesture))
}

pub fn read() -> (String, String) {
    (
        std::fs::read_to_string(defaults_path()).unwrap_or_default(),
        hyprconfig::read(Area::Mouse),
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
        let list = effective(DEFAULTS, settings);
        let shown: Vec<(Gesture, bool)> = list
            .iter()
            .map(|listed| (listed.gesture.clone(), listed.default))
            .collect();
        assert_eq!(
            shown,
            [
                (gesture(4, "up", ""), true),
                (gesture(4, "horizontal", "workspace"), true),
                (gesture(3, "pinchout", "fullscreen"), false),
            ]
        );
    }

    #[test]
    fn a_gesture_on_a_taken_axis_conflicts_as_hyprland_decides() {
        let list = effective(DEFAULTS, "");
        assert_eq!(
            conflict(&list, &gesture(3, "left", "close")),
            Some(&gesture(3, "swipe", "move"))
        );
        assert_eq!(
            conflict(&list, &gesture(4, "right", "close")),
            Some(&gesture(4, "horizontal", "workspace"))
        );
        assert_eq!(conflict(&list, &gesture(4, "down", "close")), None);
        assert_eq!(conflict(&list, &gesture(3, "pinch", "close")), None);
    }

    #[test]
    fn removing_and_adding_back_a_default_round_trips() {
        let list = effective(DEFAULTS, "");
        let moved = list[0].clone();
        let without = removed("", &moved);
        assert_eq!(
            without,
            "\nhl.gesture({ fingers = 3, direction = \"swipe\", action = \"unset\" })\n"
        );
        assert_eq!(effective(DEFAULTS, &without).len(), 2);
        let back = added(&without, DEFAULTS, &moved.gesture);
        assert_eq!(effective(DEFAULTS, &back), list);

        let own = gesture(5, "down", "close");
        let with_own = added("", DEFAULTS, &own);
        let listed = effective(DEFAULTS, &with_own);
        assert_eq!(removed(&with_own, listed.last().unwrap()), "\n");
    }
}
