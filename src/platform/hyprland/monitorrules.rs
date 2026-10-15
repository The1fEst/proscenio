use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::platform::hyprland::hyprconfig::{self, Area, read_lua_string};

const PRIMARY_VARIABLE: &str = "WAYLANDDRV_PRIMARY_MONITOR";
const BLOCK_START: &str = "hl.monitor({";
const BLOCK_END: &str = "})";

pub type Rule = HashMap<String, String>;

#[derive(Default, Clone, PartialEq, Debug)]
pub struct RuleSet {
    pub monitors: HashMap<String, Rule>,
    pub primary: String,
}

pub fn read() -> RuleSet {
    parse(&hyprconfig::read(Area::Displays))
}

pub fn write_rules(
    rules: &[(String, Vec<(String, String)>)],
    primary: Option<&str>,
) -> std::io::Result<()> {
    let text = apply_rules(&hyprconfig::read_for_edit(Area::Displays)?, rules, primary);
    hyprconfig::write(Area::Displays, &text)
}

fn apply_rules(
    text: &str,
    rules: &[(String, Vec<(String, String)>)],
    primary: Option<&str>,
) -> String {
    let text = primary.map_or_else(|| text.to_owned(), |output| with_primary(text, output));
    rules.iter().fold(text, |text, (output, pairs)| {
        with_rule(&text, output, pairs)
    })
}

fn render(value: &str) -> String {
    if value.starts_with('{') {
        value.to_owned()
    } else {
        hyprconfig::render(value)
    }
}

fn blocks(text: &str) -> Vec<(usize, usize)> {
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some(start) = text[from..].find(BLOCK_START).map(|at| at + from) {
        let body = start + BLOCK_START.len();
        let Some(end) = text[body..]
            .find(BLOCK_END)
            .map(|at| body + at + BLOCK_END.len())
        else {
            break;
        };
        spans.push((start, end));
        from = end;
    }
    spans
}

fn skip(text: &str, at: usize, is_allowed: impl Fn(char) -> bool) -> usize {
    text[at..]
        .char_indices()
        .find(|(_, character)| !is_allowed(*character))
        .map_or(text.len(), |(offset, _)| at + offset)
}

fn output_of(block: &str) -> Option<String> {
    let mut from = 0;
    while let Some(at) = block[from..].find("output").map(|at| at + from) {
        let after = skip(block, at + "output".len(), char::is_whitespace);
        if block[after..].starts_with('=') {
            let quote = skip(block, after + 1, char::is_whitespace);
            if block[quote..].starts_with('"')
                && let Some(close) = block[quote + 1..].find('"')
                && close > 0
            {
                return Some(block[quote + 1..quote + 1 + close].to_owned());
            }
        }
        from = at + 1;
    }
    None
}

fn value_end(block: &str, at: usize) -> usize {
    if let Some((_, rest)) = read_lua_string(&block[at..]) {
        return block.len() - rest.len();
    }
    if block[at..].starts_with('{')
        && let Some(close) = block[at..].find('}')
    {
        return at + close + 1;
    }
    block[at..]
        .find([',', '\n'])
        .map_or(block.len(), |offset| at + offset)
}

fn key_at(
    block: &str,
    newline: usize,
    spacing: fn(char) -> bool,
) -> Option<(usize, String, usize, usize)> {
    let name_start = skip(block, newline + 1, spacing);
    let name_end = skip(block, name_start, |c| c.is_alphanumeric() || c == '_');
    if name_end == name_start {
        return None;
    }
    let equals = skip(block, name_end, spacing);
    if !block[equals..].starts_with('=') {
        return None;
    }
    let value_start = skip(block, equals + 1, spacing);
    Some((
        name_start,
        block[name_start..name_end].to_owned(),
        value_start,
        value_end(block, value_start),
    ))
}

fn keys_of(block: &str) -> Rule {
    let mut keys = Rule::new();
    let mut from = 0;
    while let Some(newline) = block[from..].find('\n').map(|at| at + from) {
        match key_at(block, newline, |c| c == ' ' || c == '\t') {
            Some((_, name, start, end)) => {
                let value_text = block[start..end].trim();
                let value = read_lua_string(value_text).map_or_else(
                    || value_text.trim_matches('"').to_owned(),
                    |(value, _)| value,
                );
                keys.insert(name, value);
                from = end;
            }
            None => from = newline + 1,
        }
    }
    keys
}

pub fn parse(text: &str) -> RuleSet {
    let mut monitors = HashMap::new();
    for (start, end) in blocks(text) {
        let block = &text[start..end];
        if let Some(output) = output_of(block) {
            monitors.insert(output, keys_of(block));
        }
    }
    RuleSet {
        monitors,
        primary: primary_span(text)
            .map(|(start, end)| text[start..end].to_owned())
            .unwrap_or_default(),
    }
}

fn primary_span(text: &str) -> Option<(usize, usize)> {
    let mut from = 0;
    while let Some(at) = text[from..].find("hl.env(").map(|at| at + from) {
        let quote = skip(text, at + "hl.env(".len(), char::is_whitespace);
        let name = format!("\"{PRIMARY_VARIABLE}\"");
        if text[quote..].starts_with(&name) {
            let comma = skip(text, quote + name.len(), char::is_whitespace);
            if text[comma..].starts_with(',') {
                let open = skip(text, comma + 1, char::is_whitespace);
                if text[open..].starts_with('"')
                    && let Some(close) = text[open + 1..].find('"')
                {
                    return Some((open + 1, open + 1 + close));
                }
            }
        }
        from = at + 1;
    }
    None
}

pub fn with_primary(text: &str, output: &str) -> String {
    match primary_span(text) {
        Some((start, end)) => format!("{}{output}{}", &text[..start], &text[end..]),
        None => format!(
            "{}\nhl.env(\"{PRIMARY_VARIABLE}\", \"{output}\")\n",
            text.trim_end_matches('\n')
        ),
    }
}

fn set_key(block: &str, key: &str, value: &str) -> String {
    let mut from = 0;
    while let Some(newline) = block[from..].find('\n').map(|at| at + from) {
        if let Some((_, name, start, end)) = key_at(block, newline, char::is_whitespace)
            && name == key
        {
            let comma = if block[end..].starts_with(',') { 1 } else { 0 };
            return format!(
                "{}{},{}",
                &block[..start],
                render(value),
                &block[end + comma..]
            );
        }
        from = newline + 1;
    }
    match block.find("\n})") {
        Some(at) => format!(
            "{}\n\t{key} = {},{}",
            &block[..at],
            render(value),
            &block[at..]
        ),
        None => block.to_owned(),
    }
}

pub fn with_rule(text: &str, output: &str, pairs: &[(String, String)]) -> String {
    let target = blocks(text)
        .into_iter()
        .find(|(start, end)| output_of(&text[*start..*end]).as_deref() == Some(output));
    match target {
        Some((start, end)) => {
            let mut block = text[start..end].to_owned();
            for (key, value) in pairs {
                block = set_key(&block, key, value);
            }
            format!("{}{block}{}", &text[..start], &text[end..])
        }
        None => {
            let mut lines = vec![BLOCK_START.to_owned(), format!("\toutput = \"{output}\",")];
            lines.extend(
                pairs
                    .iter()
                    .map(|(key, value)| format!("\t{key} = {},", render(value))),
            );
            lines.push(BLOCK_END.to_owned());
            lines.push(String::new());
            let separator = if text.trim().is_empty() { "" } else { "\n\n" };
            format!(
                "{}{separator}{}",
                text.trim_end_matches('\n'),
                lines.join("\n")
            )
        }
    }
}

pub fn icc_profiles() -> Vec<String> {
    let home = gtk4::glib::home_dir();
    let directories = [
        home.join(".local/share/icc"),
        home.join(".color/icc"),
        PathBuf::from("/usr/local/share/color/icc"),
        PathBuf::from("/usr/share/color/icc"),
    ];
    let mut paths = Vec::new();
    for directory in &directories {
        walk(directory, &mut paths);
    }
    paths.sort();
    paths
}

fn walk(directory: &Path, paths: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, paths);
            continue;
        }
        let name = path.to_string_lossy().to_lowercase();
        if name.ends_with(".icc") || name.ends_with(".icm") {
            paths.push(path.to_string_lossy().into_owned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "hl.config({ general = { gaps_in = 4 } })\n\nhl.monitor({\n\toutput = \"DP-1\",\n\tmode = \"2560x1440@144.00\",\n\tscale = 1,\n\treserved_area = { top = 5, bottom = 0 },\n\tcm = \"srgb\",\n})\nhl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"DP-1\")\n";

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect()
    }

    #[test]
    fn a_rule_reads_back_what_each_block_asks_for() {
        let rules = parse(FILE);
        assert_eq!(rules.primary, "DP-1");
        let rule = &rules.monitors["DP-1"];
        assert_eq!(rule["mode"], "2560x1440@144.00");
        assert_eq!(rule["scale"], "1");
        assert_eq!(rule["reserved_area"], "{ top = 5, bottom = 0 }");
        assert_eq!(rule["cm"], "srgb");
    }

    #[test]
    fn keys_are_set_in_place_added_to_the_block_or_to_a_new_one() {
        assert_eq!(
            with_rule(
                FILE,
                "DP-1",
                &pairs(&[("scale", "1.25"), ("cm", "hdr"), ("bitdepth", "10")])
            ),
            "hl.config({ general = { gaps_in = 4 } })\n\nhl.monitor({\n\toutput = \"DP-1\",\n\tmode = \"2560x1440@144.00\",\n\tscale = 1.25,\n\treserved_area = { top = 5, bottom = 0 },\n\tcm = \"hdr\",\n\tbitdepth = 10,\n})\nhl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"DP-1\")\n"
        );
        assert_eq!(
            with_rule(FILE, "HDMI-A-1", &pairs(&[("position", "2560x0")])),
            "hl.config({ general = { gaps_in = 4 } })\n\nhl.monitor({\n\toutput = \"DP-1\",\n\tmode = \"2560x1440@144.00\",\n\tscale = 1,\n\treserved_area = { top = 5, bottom = 0 },\n\tcm = \"srgb\",\n})\nhl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"DP-1\")\n\nhl.monitor({\n\toutput = \"HDMI-A-1\",\n\tposition = \"2560x0\",\n})\n"
        );
        assert_eq!(
            with_primary(FILE, "HDMI-A-1"),
            FILE.replace("\"DP-1\")", "\"HDMI-A-1\")")
        );
    }

    #[test]
    fn rules_for_several_displays_and_the_primary_go_into_one_text() {
        let rules = [
            ("DP-1".to_owned(), pairs(&[("position", "0x0")])),
            ("HDMI-A-1".to_owned(), pairs(&[("position", "-1920x0")])),
        ];
        assert_eq!(
            apply_rules(FILE, &rules, Some("HDMI-A-1")),
            "hl.config({ general = { gaps_in = 4 } })\n\nhl.monitor({\n\toutput = \"DP-1\",\n\tmode = \"2560x1440@144.00\",\n\tscale = 1,\n\treserved_area = { top = 5, bottom = 0 },\n\tcm = \"srgb\",\n\tposition = \"0x0\",\n})\nhl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"HDMI-A-1\")\n\nhl.monitor({\n\toutput = \"HDMI-A-1\",\n\tposition = \"-1920x0\",\n})\n"
        );
        assert_eq!(apply_rules(FILE, &[], None), FILE);
    }

    #[test]
    fn a_display_is_turned_off_and_on_with_a_bare_boolean() {
        let disabled_text = with_rule(FILE, "DP-1", &pairs(&[("disabled", "true")]));
        assert!(disabled_text.contains("\tcm = \"srgb\",\n\tdisabled = true,\n})"));
        assert_eq!(parse(&disabled_text).monitors["DP-1"]["disabled"], "true");
        let enabled_text = with_rule(&disabled_text, "DP-1", &pairs(&[("disabled", "false")]));
        assert!(enabled_text.contains("\tdisabled = false,\n})"));
        assert!(!enabled_text.contains("disabled = true"));
    }

    #[test]
    fn quotes_and_commas_in_a_value_stay_inside_its_string() {
        let profile = "/icc/a \"b\",c\\d.icc";
        for text in [FILE, ""] {
            let config_with_rule = with_rule(text, "DP-1", &pairs(&[("icc", profile)]));
            assert!(config_with_rule.contains("\ticc = \"/icc/a \\\"b\\\",c\\\\d.icc\",\n"));
            assert_eq!(parse(&config_with_rule).monitors["DP-1"]["icc"], profile);
            let config_with_replaced_rule =
                with_rule(&config_with_rule, "DP-1", &pairs(&[("icc", "/icc/e.icc")]));
            assert_eq!(
                parse(&config_with_replaced_rule).monitors["DP-1"]["icc"],
                "/icc/e.icc"
            );
            assert!(!config_with_replaced_rule.contains("c\\\\d.icc"));
        }
    }
}
