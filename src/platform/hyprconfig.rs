use serde_json::{Map, Value};
use std::path::PathBuf;

use crate::platform::hypr;

const KINDS: [&str; 6] = ["int", "float", "bool", "str", "css", "vec2"];

pub fn settings_path() -> PathBuf {
    gtk4::glib::user_config_dir().join("hypr/settings.lua")
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

pub fn write_options(pairs: &[(String, String)]) -> std::io::Result<()> {
    let path = settings_path();
    let mut text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    for (option, value) in pairs {
        text = set_option(&text, option, value);
    }
    std::fs::write(path, text)
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

pub const SMART_GAPS: &[&str] = &[
    "hl.workspace_rule({ workspace = \"w[tv1]\", gaps_out = 0, gaps_in = 0 })",
    "hl.workspace_rule({ workspace = \"f[1]\", gaps_out = 0, gaps_in = 0 })",
    "hl.window_rule({ name = \"no-gaps-wtv1\", match = { float = false, workspace = \"w[tv1]\" }, border_size = 0, rounding = 0 })",
    "hl.window_rule({ name = \"no-gaps-f1\", match = { float = false, workspace = \"f[1]\" }, border_size = 0, rounding = 0 })",
];

pub const UNDIMMED_FULLSCREEN: &[&str] = &[
    "hl.window_rule({ name = \"no-dim-fullscreen\", match = { fullscreen = true }, no_dim = true })",
];

pub const OPAQUE_FULLSCREEN: &[&str] = &[
    "hl.window_rule({ name = \"opaque-fullscreen\", match = { fullscreen = true }, opacity = \"1 override 1 override\" })",
];

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

pub fn lines_present(lines: &[&str]) -> bool {
    has_lines(
        &std::fs::read_to_string(settings_path()).unwrap_or_default(),
        lines,
    )
}

pub fn set_lines(lines: &[&str], on: bool) -> std::io::Result<()> {
    let path = settings_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    std::fs::write(path, with_lines(&text, lines, on))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smart_gaps_are_four_lines_added_and_removed_together() {
        let text = "-- mine\nhl.config({ general = { gaps_in = 4 } })\n";
        let on = with_lines(text, &SMART_GAPS, true);
        assert!(has_lines(&on, &SMART_GAPS));
        assert!(on.starts_with(text));
        assert_eq!(with_lines(&on, &SMART_GAPS, true), on);
        assert_eq!(with_lines(&on, &SMART_GAPS, false), text);
        assert!(!has_lines(
            &format!("{text}{}\n", SMART_GAPS[0]),
            &SMART_GAPS
        ));
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
}
