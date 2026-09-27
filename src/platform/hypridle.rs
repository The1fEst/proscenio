use std::collections::HashMap;
use std::path::PathBuf;

const SCREEN_OFF: &str = "hyprctl dispatch 'hl.dsp.dpms({ action = \"disable\" })'";
const SCREEN_ON: &str = "hyprctl dispatch 'hl.dsp.dpms({ action = \"enable\" })'";

pub fn config_path() -> PathBuf {
    gtk4::glib::user_config_dir().join("hypr/hypridle.conf")
}

pub fn read() -> HashMap<String, u64> {
    timeouts(&std::fs::read_to_string(config_path()).unwrap_or_default())
}

pub fn write(pairs: &[(String, i64)]) -> std::io::Result<()> {
    let path = config_path();
    let mut text = std::fs::read_to_string(&path)?;
    for (what, seconds) in pairs {
        text = set_timeout(&text, what, *seconds);
    }
    std::fs::write(path, text)
}

pub fn read_general() -> HashMap<String, String> {
    general(&std::fs::read_to_string(config_path()).unwrap_or_default())
}

pub fn write_general(changes: &[(String, Option<String>)]) -> std::io::Result<()> {
    let path = config_path();
    let mut text = std::fs::read_to_string(&path)?;
    for (key, value) in changes {
        text = set_general(&text, key, value.as_deref());
    }
    std::fs::write(path, text)
}

fn general_body(text: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim() == "general {" {
            let start = offset + line.len();
            let end = start + text[start..].find("\n}")?;
            return Some((start, end));
        }
        offset += line.len();
    }
    None
}

fn key_of(line: &str) -> Option<&str> {
    line.split_once('=').map(|(key, _)| key.trim())
}

pub fn general(text: &str) -> HashMap<String, String> {
    let Some((start, end)) = general_body(text) else {
        return HashMap::new();
    };
    text[start..end]
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            Some((key.trim().to_owned(), value.trim().to_owned()))
        })
        .collect()
}

pub fn set_general(text: &str, key: &str, value: Option<&str>) -> String {
    let Some((start, end)) = general_body(text) else {
        return match value {
            Some(value) => format!("general {{\n    {key} = {value}\n}}\n\n{text}"),
            None => text.to_owned(),
        };
    };
    let mut offset = start;
    for line in text[start..end].split_inclusive('\n') {
        let line_end = offset + line.len();
        if key_of(line) == Some(key) {
            let last = !line.ends_with('\n');
            return match value {
                Some(value) => format!(
                    "{}    {key} = {value}{}{}",
                    &text[..offset],
                    if last { "" } else { "\n" },
                    &text[line_end..]
                ),
                None if last => format!(
                    "{}{}",
                    text[..offset].trim_end_matches('\n'),
                    &text[line_end..]
                ),
                None => format!("{}{}", &text[..offset], &text[line_end..]),
            };
        }
        offset = line_end;
    }
    match value {
        Some(value) => format!("{}\n    {key} = {value}{}", &text[..end], &text[end..]),
        None => text.to_owned(),
    }
}

fn skip_whitespace(text: &str, at: usize) -> usize {
    text[at..]
        .char_indices()
        .find(|(_, character)| !character.is_whitespace())
        .map_or(text.len(), |(offset, _)| at + offset)
}

fn listeners(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(at) = text[from..].find("listener").map(|at| at + from) {
        let brace = skip_whitespace(text, at + "listener".len());
        if !text[brace..].starts_with('{') {
            from = at + 1;
            continue;
        }
        let Some(close) = text[brace..].find("\n}").map(|offset| brace + offset + 2) else {
            break;
        };
        let mut start = at;
        while start > from && text.as_bytes()[start - 1] == b'\n' {
            start -= 1;
        }
        found.push((start, close));
        from = close;
    }
    found
}

fn what_of(block: &str) -> Option<&'static str> {
    let mut fired = Vec::new();
    let mut from = 0;
    while let Some(at) = block[from..].find("on-timeout").map(|at| at + from) {
        let equals = skip_whitespace(block, at + "on-timeout".len());
        if block[equals..].starts_with('=') {
            let value = skip_whitespace(block, equals + 1);
            let end = block[value..]
                .find('\n')
                .map_or(block.len(), |offset| value + offset);
            fired.push(block[value..end].to_owned());
        }
        from = at + 1;
    }
    let fired = fired.join(" ").to_lowercase();
    if fired.contains("dpms") {
        Some("screen")
    } else if fired.contains("lock") {
        Some("lock")
    } else if fired.contains("suspend") {
        Some("suspend")
    } else {
        None
    }
}

fn timeout_span(block: &str) -> Option<(usize, usize, u64)> {
    let mut from = 0;
    while let Some(at) = block[from..].find("timeout").map(|at| at + from) {
        let equals = skip_whitespace(block, at + "timeout".len());
        if block[equals..].starts_with('=') {
            let digits = skip_whitespace(block, equals + 1);
            let end = block[digits..]
                .find(|character: char| !character.is_ascii_digit())
                .map_or(block.len(), |offset| digits + offset);
            if end > digits
                && let Ok(seconds) = block[digits..end].parse()
            {
                return Some((digits, end, seconds));
            }
        }
        from = at + 1;
    }
    None
}

fn by_what(text: &str) -> HashMap<&'static str, (usize, usize)> {
    let mut found = HashMap::new();
    for (start, end) in listeners(text) {
        if let Some(what) = what_of(&text[start..end]) {
            found.entry(what).or_insert((start, end));
        }
    }
    found
}

pub fn timeouts(text: &str) -> HashMap<String, u64> {
    by_what(text)
        .into_iter()
        .filter_map(|(what, (start, end))| {
            timeout_span(&text[start..end]).map(|(_, _, seconds)| (what.to_owned(), seconds))
        })
        .collect()
}

fn block_for(what: &str, seconds: i64, text: &str) -> String {
    match what {
        "screen" => format!(
            "listener {{\n    timeout = {seconds}\n    on-timeout = {SCREEN_OFF}\n    on-resume = {SCREEN_ON}\n}}"
        ),
        "lock" => {
            let command = if text.contains("$lock_cmd") {
                "$lock_cmd"
            } else {
                "loginctl lock-session"
            };
            format!("listener {{\n    timeout = {seconds}\n    on-timeout = {command}\n}}")
        }
        _ => {
            let command = if text.contains("$suspend_cmd") {
                "$suspend_cmd"
            } else {
                "systemctl suspend || loginctl suspend"
            };
            format!("listener {{\n    timeout = {seconds}\n    on-timeout = {command}\n}}")
        }
    }
}

pub fn set_timeout(text: &str, what: &str, seconds: i64) -> String {
    let Some(&(start, end)) = by_what(text).get(what) else {
        if seconds <= 0 {
            return text.to_owned();
        }
        return format!(
            "{}\n\n{}\n",
            text.trim_end_matches('\n'),
            block_for(what, seconds, text)
        );
    };
    if seconds <= 0 {
        return format!("{}{}", &text[..start], &text[end..]);
    }
    let block = &text[start..end];
    let changed = match timeout_span(block) {
        Some((digits, digits_end, _)) => {
            let line_end = block[digits_end..]
                .find('\n')
                .map_or(block.len(), |offset| digits_end + offset);
            format!("{}{seconds}{}", &block[..digits], &block[line_end..])
        }
        None => block.to_owned(),
    };
    format!("{}{changed}{}", &text[..start], &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "$lock_cmd = loginctl lock-session\n\ngeneral {\n    lock_cmd = $lock_cmd\n}\n\nlistener {\n    timeout = 1800 # 30mins\n    on-timeout = loginctl lock-session\n}\n\nlistener {\n    timeout = 2400\n    on-timeout = hyprctl dispatch 'hl.dsp.dpms({ action = \"disable\" })'\n    on-resume = hyprctl dispatch 'hl.dsp.dpms({ action = \"enable\" })'\n}\n";

    #[test]
    fn timeouts_are_read_by_what_their_listener_does() {
        let found = timeouts(FILE);
        assert_eq!(found.get("lock"), Some(&1800));
        assert_eq!(found.get("screen"), Some(&2400));
        assert_eq!(found.get("suspend"), None);
    }

    #[test]
    fn a_timeout_is_changed_removed_or_added() {
        assert_eq!(
            set_timeout(FILE, "lock", 600),
            FILE.replace("timeout = 1800 # 30mins", "timeout = 600")
        );
        assert_eq!(
            set_timeout(FILE, "screen", 0),
            "$lock_cmd = loginctl lock-session\n\ngeneral {\n    lock_cmd = $lock_cmd\n}\n\nlistener {\n    timeout = 1800 # 30mins\n    on-timeout = loginctl lock-session\n}\n"
        );
        assert_eq!(
            set_timeout(FILE, "suspend", 2700),
            format!(
                "{}\n\nlistener {{\n    timeout = 2700\n    on-timeout = systemctl suspend || loginctl suspend\n}}\n",
                FILE.trim_end_matches('\n')
            )
        );
    }

    #[test]
    fn general_keys_are_added_changed_and_removed_inside_the_block() {
        let added = set_general(FILE, "before_sleep_cmd", Some("loginctl lock-session"));
        assert!(added.contains(
            "general {\n    lock_cmd = $lock_cmd\n    before_sleep_cmd = loginctl lock-session\n}\n"
        ));
        assert_eq!(
            general(&added).get("before_sleep_cmd").map(String::as_str),
            Some("loginctl lock-session")
        );
        let changed = set_general(&added, "lock_cmd", Some("hyprlock"));
        assert!(changed.contains("general {\n    lock_cmd = hyprlock\n    before_sleep_cmd"));
        assert_eq!(set_general(&added, "before_sleep_cmd", None), FILE);
        assert_eq!(
            set_general(FILE, "lock_cmd", None),
            FILE.replace("general {\n    lock_cmd = $lock_cmd\n}", "general {\n}")
        );
    }
}
