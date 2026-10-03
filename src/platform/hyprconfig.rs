use serde_json::{Map, Value};
use std::path::PathBuf;

use crate::platform::hypr;

const KINDS: [&str; 6] = ["int", "float", "bool", "str", "css", "vec2"];
const LEGACY_HEADER: [&str; 2] = [
    "-- Written by the settings app.",
    "-- before the files in custom/,",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    Appearance,
    Displays,
    Multitasking,
    Keyboard,
    Accessibility,
    Mouse,
    Devices,
    Apps,
    Binds,
    Other,
}

impl Area {
    pub fn name(self) -> &'static str {
        match self {
            Area::Appearance => "appearance",
            Area::Displays => "displays",
            Area::Multitasking => "multitasking",
            Area::Keyboard => "keyboard",
            Area::Accessibility => "accessibility",
            Area::Mouse => "mouse",
            Area::Devices => "devices",
            Area::Apps => "apps",
            Area::Binds => "binds",
            Area::Other => "other",
        }
    }
}

pub struct Lines {
    pub area: Area,
    pub lines: &'static [&'static str],
}

fn config_dir() -> PathBuf {
    gtk4::glib::user_config_dir().join("hypr")
}

pub fn path(area: Area) -> PathBuf {
    config_dir()
        .join("settings")
        .join(format!("{}.lua", area.name()))
}

pub fn read(area: Area) -> String {
    std::fs::read_to_string(path(area)).unwrap_or_default()
}

pub fn write(area: Area, text: &str) -> std::io::Result<()> {
    let path = path(area);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, text)
}

pub fn read_options(names: &[&str]) -> Map<String, Value> {
    let mut state = Map::new();
    for name in names {
        let Some(reply) = hypr::json(&format!("getoption {name}")) else {
            continue;
        };
        if let Some(value) = KINDS.iter().find_map(|kind| reply.get(*kind)) {
            state.insert((*name).to_owned(), value.clone());
        }
    }
    state
}

pub fn write_options(area: Area, pairs: &[(String, String)]) -> std::io::Result<()> {
    let mut text = read(area);
    for (option, value) in pairs {
        text = set_option(&text, option, value);
    }
    write(area, &text)
}

pub fn render(value: &str) -> String {
    if value == "true" || value == "false" || value.parse::<f64>().is_ok() {
        value.to_owned()
    } else {
        format!("\"{value}\"")
    }
}

fn line_for(option: &str, value: &str) -> String {
    let keys: Vec<String> = option.split(':').map(|key| key.replace('-', "_")).collect();
    let mut body = format!("{} = {}", keys[keys.len() - 1], render(value));
    for key in keys[..keys.len() - 1].iter().rev() {
        body = format!("{key} = {{ {body} }}");
    }
    format!("hl.config({{ {body} }})")
}

fn prefix_for(option: &str) -> String {
    let line = line_for(option, "");
    let cut = line.rfind('=').unwrap_or(line.len());
    format!("{}=", &line[..cut])
}

pub fn set_option(text: &str, option: &str, value: &str) -> String {
    let prefix = prefix_for(option);
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.starts_with(&prefix) {
            let end = offset + line.trim_end_matches('\n').len();
            return format!(
                "{}{}{}",
                &text[..offset],
                line_for(option, value),
                &text[end..]
            );
        }
        offset += line.len();
    }
    format!(
        "{}\n{}\n",
        text.trim_end_matches('\n'),
        line_for(option, value)
    )
}

pub const SMART_GAPS: Lines = Lines {
    area: Area::Multitasking,
    lines: &[
        "hl.workspace_rule({ workspace = \"w[tv1]\", gaps_out = 0, gaps_in = 0 })",
        "hl.workspace_rule({ workspace = \"f[1]\", gaps_out = 0, gaps_in = 0 })",
        "hl.window_rule({ name = \"no-gaps-wtv1\", match = { float = false, workspace = \"w[tv1]\" }, border_size = 0, rounding = 0 })",
        "hl.window_rule({ name = \"no-gaps-f1\", match = { float = false, workspace = \"f[1]\" }, border_size = 0, rounding = 0 })",
    ],
};

pub const UNDIMMED_FULLSCREEN: Lines = Lines {
    area: Area::Appearance,
    lines: &[
        "hl.window_rule({ name = \"no-dim-fullscreen\", match = { fullscreen = true }, no_dim = true })",
    ],
};

pub const OPAQUE_FULLSCREEN: Lines = Lines {
    area: Area::Appearance,
    lines: &[
        "hl.window_rule({ name = \"opaque-fullscreen\", match = { fullscreen = true }, opacity = \"1 override 1 override\" })",
    ],
};

pub struct Border {
    pub field: &'static str,
    pub color: &'static str,
    pub default: u8,
}

pub const ACTIVE_BORDER: Border = Border {
    field: "active_border",
    color: "active",
    default: 0x77,
};

pub const INACTIVE_BORDER: Border = Border {
    field: "inactive_border",
    color: "inactive",
    default: 0x33,
};

fn border_prefix(border: &Border) -> String {
    format!(
        "if border_colors then hl.config({{ general = {{ col = {{ {} = \"rgba(\" .. border_colors.{} .. \"",
        border.field, border.color
    )
}

fn border_alpha_in(text: &str, border: &Border) -> u8 {
    let prefix = border_prefix(border);
    text.lines()
        .find_map(|line| line.strip_prefix(&prefix))
        .and_then(|rest| rest.get(..2))
        .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        .unwrap_or(border.default)
}

fn with_border_alpha(text: &str, border: &Border, alpha: u8) -> String {
    let prefix = border_prefix(border);
    let line = format!("{prefix}{alpha:02X})\" }} }} }}) end");
    let mut found = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|existing| {
            if existing.starts_with(&prefix) {
                found = true;
                line.clone()
            } else {
                existing.to_owned()
            }
        })
        .collect();
    if !found {
        lines.push(line);
    }
    let mut joined = lines.join("\n");
    joined.push('\n');
    joined
}

pub fn border_alpha(border: &Border) -> u8 {
    border_alpha_in(&read(Area::Appearance), border)
}

pub fn set_border_alpha(border: &Border, alpha: u8) -> std::io::Result<()> {
    write(
        Area::Appearance,
        &with_border_alpha(&read(Area::Appearance), border, alpha),
    )
}

fn has_lines(text: &str, lines: &[&str]) -> bool {
    lines
        .iter()
        .all(|wanted| text.lines().any(|line| line.trim() == *wanted))
}

fn with_lines(text: &str, lines: &[&str], on: bool) -> String {
    let mut kept: Vec<&str> = text
        .lines()
        .filter(|line| !lines.contains(&line.trim()))
        .collect();
    while kept.last().is_some_and(|line| line.trim().is_empty()) {
        kept.pop();
    }
    if on {
        kept.extend_from_slice(lines);
    }
    let mut joined = kept.join("\n");
    joined.push('\n');
    joined
}

pub fn lines_present(set: &Lines) -> bool {
    has_lines(&read(set.area), set.lines)
}

pub fn set_lines(set: &Lines, on: bool) -> std::io::Result<()> {
    write(set.area, &with_lines(&read(set.area), set.lines, on))
}

fn statements(text: &str) -> Vec<String> {
    let mut statements: Vec<String> = Vec::new();
    for line in text.lines() {
        let continues = line.starts_with(char::is_whitespace)
            || line.starts_with('}')
            || line.starts_with(')')
            || line.starts_with("end");
        match statements.last_mut() {
            Some(last) if continues => {
                last.push('\n');
                last.push_str(line);
            }
            _ if line.trim().is_empty() => {}
            _ => statements.push(line.to_owned()),
        }
    }
    statements
}

fn option_of(statement: &str) -> Option<String> {
    let mut rest = statement.strip_prefix("hl.config({")?;
    let mut keys = Vec::new();
    loop {
        rest = rest.trim_start();
        let end = rest.find(|char: char| !(char.is_alphanumeric() || char == '_'))?;
        keys.push(&rest[..end]);
        rest = rest[end..].trim_start().strip_prefix('=')?.trim_start();
        match rest.strip_prefix('{') {
            Some(inner) => rest = inner,
            None => return Some(keys.join(":")),
        }
    }
}

fn area_of(statement: &str, option_area: &impl Fn(&str) -> Option<Area>) -> Option<Area> {
    let first = statement.lines().next().unwrap_or("").trim();
    if LEGACY_HEADER.iter().any(|header| first.starts_with(header)) {
        return None;
    }
    let among = |set: &Lines| set.lines.contains(&first);
    Some(if let Some(option) = option_of(first) {
        option_area(&option).unwrap_or(Area::Other)
    } else if first.starts_with("hl.monitor(")
        || first.starts_with("hl.env(\"WAYLANDDRV_PRIMARY_MONITOR\"")
    {
        Area::Displays
    } else if first.starts_with("hl.env(") || first.starts_with("hl.on(\"hyprland.start\"") {
        Area::Appearance
    } else if among(&SMART_GAPS) {
        Area::Multitasking
    } else if among(&UNDIMMED_FULLSCREEN) || among(&OPAQUE_FULLSCREEN) {
        Area::Appearance
    } else if first.starts_with("hl.window_rule(") {
        Area::Apps
    } else if first.starts_with("hl.device(") {
        Area::Devices
    } else if first.starts_with("hl.gesture(") {
        Area::Mouse
    } else if first.starts_with("shortcut(") {
        Area::Binds
    } else {
        Area::Other
    })
}

fn split(text: &str, option_area: &impl Fn(&str) -> Option<Area>) -> Vec<(Area, String)> {
    let mut parts: Vec<(Area, String)> = Vec::new();
    for statement in statements(text) {
        let Some(area) = area_of(&statement, option_area) else {
            continue;
        };
        match parts.iter_mut().find(|(known, _)| *known == area) {
            Some((_, part)) => {
                part.push_str(&statement);
                part.push('\n');
            }
            None => parts.push((area, format!("{statement}\n"))),
        }
    }
    parts
}

pub fn split_legacy(option_area: impl Fn(&str) -> Option<Area>) -> std::io::Result<()> {
    let legacy = config_dir().join("settings.lua");
    let Ok(text) = std::fs::read_to_string(&legacy) else {
        return Ok(());
    };
    for (area, part) in split(&text, &option_area) {
        let mut combined = read(area);
        if !combined.is_empty() && !combined.ends_with('\n') {
            combined.push('\n');
        }
        combined.push_str(&part);
        write(area, &combined)?;
    }
    std::fs::rename(&legacy, config_dir().join("settings.lua.bak"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_gaps_are_four_lines_added_and_removed_together() {
        let text = "-- mine\nhl.config({ general = { gaps_in = 4 } })\n";
        let lines = SMART_GAPS.lines;
        let on = with_lines(text, lines, true);
        assert!(has_lines(&on, lines));
        assert!(on.starts_with(text));
        assert_eq!(with_lines(&on, lines, true), on);
        assert_eq!(with_lines(&on, lines, false), text);
        assert!(!has_lines(&format!("{text}{}\n", lines[0]), lines));
    }

    #[test]
    fn a_border_alpha_is_one_guarded_line_over_the_generated_color() {
        let text = "hl.config({ general = { gaps_in = 4 } })\n";
        assert_eq!(border_alpha_in(text, &ACTIVE_BORDER), 0x77);
        let set = with_border_alpha(text, &ACTIVE_BORDER, 0xCC);
        assert_eq!(
            set,
            "hl.config({ general = { gaps_in = 4 } })\nif border_colors then hl.config({ general = { col = { active_border = \"rgba(\" .. border_colors.active .. \"CC)\" } } }) end\n"
        );
        assert_eq!(border_alpha_in(&set, &ACTIVE_BORDER), 0xCC);
        assert_eq!(border_alpha_in(&set, &INACTIVE_BORDER), 0x33);
        let again = with_border_alpha(&set, &ACTIVE_BORDER, 0xFF);
        assert_eq!(again.lines().count(), 2);
        assert_eq!(border_alpha_in(&again, &ACTIVE_BORDER), 0xFF);
    }

    #[test]
    fn options_are_rewritten_on_their_own_line_or_appended() {
        let text = "-- mine\nhl.config({ general = { gaps_in = 4 } })\nhl.config({ decoration = { rounding = 10 } })\n";
        assert_eq!(
            set_option(text, "general:gaps_in", "7"),
            "-- mine\nhl.config({ general = { gaps_in = 7 } })\nhl.config({ decoration = { rounding = 10 } })\n"
        );
        assert_eq!(
            set_option(text, "input:touchpad:tap-to-click", "true"),
            "-- mine\nhl.config({ general = { gaps_in = 4 } })\nhl.config({ decoration = { rounding = 10 } })\nhl.config({ input = { touchpad = { tap_to_click = true } } })\n"
        );
        assert_eq!(
            set_option("", "general:layout", "master"),
            "\nhl.config({ general = { layout = \"master\" } })\n"
        );
    }

    #[test]
    fn the_single_settings_file_splits_into_its_areas() {
        let legacy = concat!(
            "-- Written by the settings app. Hyprland sources it after its own configuration and\n",
            "-- before the files in custom/, so anything set here can still be overridden there.\n",
            "hl.env(\"XCURSOR_SIZE\", \"36\")\n",
            "\n",
            "hl.on(\"hyprland.start\", function()\n",
            "\thl.exec_cmd(\"hyprctl setcursor Bibata 36\")\n",
            "end)\n",
            "\n",
            "hl.monitor({\n",
            "\toutput = \"DP-1\",\n",
            "\treserved_area = { top = 0, right = 0 },\n",
            "})\n",
            "hl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"DP-1\")\n",
            "hl.config({ general = { gaps_in = 2 } })\n",
            "hl.config({ decoration = { shadow = { range = 10 } } })\n",
            "hl.config({ misc = { mystery = 1 } })\n",
            "hl.window_rule({ name = \"no-dim-fullscreen\", match = { fullscreen = true }, no_dim = true })\n",
            "hl.window_rule({ match = { class = \"^(code)$\" }, workspace = \"special:magic\" })\n",
            "hl.device({ name = \"mouse\", sensitivity = 0.5 })\n",
            "hl.gesture({\n",
            "\tfingers = 3,\n",
            "\tdirection = \"swipe\",\n",
            "})\n",
            "shortcut(\"Window: Close\", \"SUPER + X\", false)\n",
        );
        let option_area = |option: &str| match option {
            "general:gaps_in" => Some(Area::Multitasking),
            "decoration:shadow:range" => Some(Area::Appearance),
            _ => None,
        };
        let parts = split(legacy, &option_area);
        let part = |area| {
            parts
                .iter()
                .find(|(known, _)| *known == area)
                .map(|(_, text)| text.as_str())
                .unwrap_or("")
        };
        assert_eq!(
            part(Area::Appearance),
            "hl.env(\"XCURSOR_SIZE\", \"36\")\nhl.on(\"hyprland.start\", function()\n\thl.exec_cmd(\"hyprctl setcursor Bibata 36\")\nend)\nhl.config({ decoration = { shadow = { range = 10 } } })\nhl.window_rule({ name = \"no-dim-fullscreen\", match = { fullscreen = true }, no_dim = true })\n"
        );
        assert_eq!(
            part(Area::Displays),
            "hl.monitor({\n\toutput = \"DP-1\",\n\treserved_area = { top = 0, right = 0 },\n})\nhl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"DP-1\")\n"
        );
        assert_eq!(
            part(Area::Multitasking),
            "hl.config({ general = { gaps_in = 2 } })\n"
        );
        assert_eq!(part(Area::Other), "hl.config({ misc = { mystery = 1 } })\n");
        assert_eq!(
            part(Area::Apps),
            "hl.window_rule({ match = { class = \"^(code)$\" }, workspace = \"special:magic\" })\n"
        );
        assert_eq!(
            part(Area::Devices),
            "hl.device({ name = \"mouse\", sensitivity = 0.5 })\n"
        );
        assert_eq!(
            part(Area::Mouse),
            "hl.gesture({\n\tfingers = 3,\n\tdirection = \"swipe\",\n})\n"
        );
        assert_eq!(
            part(Area::Binds),
            "shortcut(\"Window: Close\", \"SUPER + X\", false)\n"
        );
        assert_eq!(parts.len(), 8);
    }
}
