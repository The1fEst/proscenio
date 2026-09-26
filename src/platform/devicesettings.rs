use crate::platform::hyprconfig::{render, settings_path};

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub device: String,
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Set(String, String, String),
    Unset(String, String),
}

fn skip(text: &str) -> &str {
    text.trim_start_matches(char::is_whitespace)
}

pub fn parse_line(line: &str) -> Option<Setting> {
    let rest = skip(line.trim().strip_prefix("hl.device({")?);
    let rest = skip(skip(rest.strip_prefix("name")?).strip_prefix('=')?);
    let rest = rest.strip_prefix('"')?;
    let quote = rest.find('"')?;
    let device = &rest[..quote];
    let rest = skip(skip(&rest[quote + 1..]).strip_prefix(',')?);
    let key_end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    if key_end == 0 {
        return None;
    }
    let key = &rest[..key_end];
    let rest = skip(skip(&rest[key_end..]).strip_prefix('=')?);
    let close = rest.find('}')?;
    if rest[close..].trim_end() != "})" {
        return None;
    }
    Some(Setting {
        device: device.to_owned(),
        key: key.to_owned(),
        value: rest[..close].trim_end().trim_matches('"').to_owned(),
    })
}

pub fn parse(text: &str) -> Vec<Setting> {
    text.lines().filter_map(parse_line).collect()
}

fn line_for(device: &str, key: &str, value: &str) -> String {
    format!(
        "hl.device({{ name = \"{device}\", {key} = {} }})",
        render(value)
    )
}

fn is_setting(line: &str, device: &str, key: &str) -> bool {
    parse_line(line).is_some_and(|found| found.device == device && found.key == key)
}

pub fn with_changes(text: &str, changes: &[Change]) -> String {
    let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
    for change in changes {
        let (device, key) = match change {
            Change::Set(device, key, _) | Change::Unset(device, key) => (device, key),
        };
        lines.retain(|line| !is_setting(line, device, key));
        if let Change::Set(device, key, value) = change {
            lines.push(line_for(device, key, value));
        }
    }
    format!("{}\n", lines.join("\n").trim_end_matches('\n'))
}

fn contents() -> String {
    std::fs::read_to_string(settings_path()).unwrap_or_default()
}

pub fn read() -> Vec<Setting> {
    parse(&contents())
}

pub fn apply(changes: &[Change]) -> std::io::Result<()> {
    std::fs::write(settings_path(), with_changes(&contents(), changes))
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "-- mine\nhl.device({ name = \"logitech-mouse\", sensitivity = -0.3 })\nhl.device({ name = \"logitech-mouse\", accel_profile = \"flat\" })\n  hl.device({name=\"at-kb\",kb_layout=\"us,ru\"})\nhl.device({ name = \"other\", natural_scroll = true, sensitivity = 1 })\n";

    fn setting(device: &str, key: &str, value: &str) -> Setting {
        Setting {
            device: device.to_owned(),
            key: key.to_owned(),
            value: value.to_owned(),
        }
    }

    #[test]
    fn a_device_setting_is_one_line_with_one_key() {
        assert_eq!(
            parse(FILE),
            [
                setting("logitech-mouse", "sensitivity", "-0.3"),
                setting("logitech-mouse", "accel_profile", "flat"),
                setting("at-kb", "kb_layout", "us,ru"),
                setting("other", "natural_scroll", "true, sensitivity = 1"),
            ]
        );
    }

    #[test]
    fn changes_apply_in_order_to_one_reading() {
        let text = |device: &str, key: &str, value: &str| {
            (device.to_owned(), key.to_owned(), value.to_owned())
        };
        let set = |(device, key, value)| Change::Set(device, key, value);
        assert_eq!(
            with_changes(
                FILE,
                &[
                    set(text("logitech-mouse", "sensitivity", "0.5")),
                    Change::Unset("logitech-mouse".to_owned(), "accel_profile".to_owned()),
                    set(text("at-kb", "kb_options", "grp:alt_shift_toggle")),
                ]
            ),
            "-- mine\n  hl.device({name=\"at-kb\",kb_layout=\"us,ru\"})\nhl.device({ name = \"other\", natural_scroll = true, sensitivity = 1 })\nhl.device({ name = \"logitech-mouse\", sensitivity = 0.5 })\nhl.device({ name = \"at-kb\", kb_options = \"grp:alt_shift_toggle\" })\n"
        );
        assert_eq!(
            with_changes(
                FILE,
                &[Change::Unset("at-kb".to_owned(), "kb_layout".to_owned())]
            ),
            "-- mine\nhl.device({ name = \"logitech-mouse\", sensitivity = -0.3 })\nhl.device({ name = \"logitech-mouse\", accel_profile = \"flat\" })\nhl.device({ name = \"other\", natural_scroll = true, sensitivity = 1 })\n"
        );
    }
}
