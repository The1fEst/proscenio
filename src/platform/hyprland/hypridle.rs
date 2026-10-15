use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::platform::hyprland::hyprconfig;

const SCREEN_OFF: &str = "hyprctl dispatch 'hl.dsp.dpms({ action = \"disable\" })'";
const SCREEN_ON: &str = "hyprctl dispatch 'hl.dsp.dpms({ action = \"enable\" })'";
const SHELL_LOCK: &str = "hyprctl dispatch 'hl.dsp.global(\"proscenio:lock\")'";
const LOCKERS: [&str; 5] = ["hyprlock", "swaylock", "gtklock", "waylock", "$lock_cmd"];
const LAUNCHERS: [&str; 7] = ["sh", "bash", "dash", "-c", "exec", "nohup", "setsid"];

pub fn config_path() -> PathBuf {
    gtk4::glib::user_config_dir().join("hypr/hypridle.conf")
}

pub fn read() -> HashMap<String, u64> {
    timeouts(&std::fs::read_to_string(config_path()).unwrap_or_default())
}

pub fn write(
    timeouts: &[(String, i64)],
    general_changes: &[(String, Option<String>)],
) -> std::io::Result<bool> {
    write_to(&config_path(), timeouts, general_changes)
}

fn write_to(
    path: &Path,
    timeouts: &[(String, i64)],
    general_changes: &[(String, Option<String>)],
) -> std::io::Result<bool> {
    let (mut text, is_missing) = match std::fs::read_to_string(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (String::new(), true),
        result => (result?, false),
    };
    for (kind, seconds) in timeouts {
        text = set_timeout(&text, kind, *seconds);
    }
    for (key, value) in general_changes {
        text = set_general(&text, key, value.as_deref());
    }
    if is_missing && text.is_empty() {
        return Ok(false);
    }
    if is_missing && !general(&text).contains_key("lock_cmd") {
        text = set_general(&text, "lock_cmd", Some(SHELL_LOCK));
    }
    hyprconfig::replace_file(path, &text)?;
    Ok(true)
}

pub fn read_general() -> HashMap<String, String> {
    general(&std::fs::read_to_string(config_path()).unwrap_or_default())
}

fn general_body(text: &str) -> Option<(usize, usize)> {
    let mut offset = 0;
    let mut start = None;
    for line in text.split_inclusive('\n') {
        match start {
            None if line.trim() == "general {" => start = Some(offset + line.len()),
            Some(start) if line.trim_start().starts_with('}') => {
                let end = offset - 1;
                return Some((start.min(end), end));
            }
            _ => {}
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
            Some(value) if text.is_empty() => format!("general {{\n    {key} = {value}\n}}\n"),
            Some(value) => format!("general {{\n    {key} = {value}\n}}\n\n{text}"),
            None => text.to_owned(),
        };
    };
    let mut offset = start;
    for line in text[start..end].split_inclusive('\n') {
        let line_end = offset + line.len();
        if key_of(line) == Some(key) {
            let is_last = !line.ends_with('\n');
            return match value {
                Some(value) => format!(
                    "{}    {key} = {value}{}{}",
                    &text[..offset],
                    if is_last { "" } else { "\n" },
                    &text[line_end..]
                ),
                None if is_last => format!(
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
    let mut spans = Vec::new();
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
        spans.push((start, close));
        from = close;
    }
    spans
}

pub fn locks(command: &str) -> bool {
    command.split(['|', '&', ';']).any(|part| {
        let words: Vec<&str> = part
            .split(|character: char| character.is_whitespace() || "'\"(){}`".contains(character))
            .filter(|word| !word.is_empty())
            .skip_while(|word| LAUNCHERS.contains(word))
            .collect();
        let Some(program) = words.first() else {
            return false;
        };
        LOCKERS.contains(&program.rsplit('/').next().unwrap_or(program))
            || (*program == "loginctl" && words.iter().any(|word| word.starts_with("lock-session")))
            || words.contains(&"proscenio:lock")
            || words
                .windows(3)
                .any(|call| call == ["call", "lock", "activate"])
    })
}

fn classify_listener(block: &str) -> Option<&'static str> {
    let mut commands = Vec::new();
    let mut from = 0;
    while let Some(at) = block[from..].find("on-timeout").map(|at| at + from) {
        let equals = skip_whitespace(block, at + "on-timeout".len());
        if block[equals..].starts_with('=') {
            let value = skip_whitespace(block, equals + 1);
            let end = block[value..]
                .find('\n')
                .map_or(block.len(), |offset| value + offset);
            commands.push(block[value..end].to_owned());
        }
        from = at + 1;
    }
    let has_lock_command = commands.iter().any(|command| locks(command));
    let command_text = commands.join(" ").to_lowercase();
    if command_text.contains("dpms") {
        Some("screen")
    } else if has_lock_command {
        Some("lock")
    } else if command_text.contains("suspend") {
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

fn index_listeners(text: &str) -> HashMap<&'static str, (usize, usize)> {
    let mut spans = HashMap::new();
    for (start, end) in listeners(text) {
        if let Some(kind) = classify_listener(&text[start..end]) {
            spans.entry(kind).or_insert((start, end));
        }
    }
    spans
}

pub fn timeouts(text: &str) -> HashMap<String, u64> {
    index_listeners(text)
        .into_iter()
        .filter_map(|(kind, (start, end))| {
            timeout_span(&text[start..end]).map(|(_, _, seconds)| (kind.to_owned(), seconds))
        })
        .collect()
}

fn block_for(kind: &str, seconds: i64, text: &str) -> String {
    match kind {
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

pub fn set_timeout(text: &str, kind: &str, seconds: i64) -> String {
    let Some(&(start, end)) = index_listeners(text).get(kind) else {
        if seconds <= 0 {
            return text.to_owned();
        }
        if text.is_empty() {
            return format!("{}\n", block_for(kind, seconds, text));
        }
        return format!(
            "{}\n\n{}\n",
            text.trim_end_matches('\n'),
            block_for(kind, seconds, text)
        );
    };
    if seconds <= 0 {
        return format!("{}{}", &text[..start], &text[end..]);
    }
    let block = &text[start..end];
    let new_block = match timeout_span(block) {
        Some((digits, digits_end, _)) => {
            let line_end = block[digits_end..]
                .find('\n')
                .map_or(block.len(), |offset| digits_end + offset);
            format!("{}{seconds}{}", &block[..digits], &block[line_end..])
        }
        None => block.to_owned(),
    };
    format!("{}{new_block}{}", &text[..start], &text[end..])
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = "$lock_cmd = loginctl lock-session\n\ngeneral {\n    lock_cmd = $lock_cmd\n}\n\nlistener {\n    timeout = 1800 # 30mins\n    on-timeout = loginctl lock-session\n}\n\nlistener {\n    timeout = 2400\n    on-timeout = hyprctl dispatch 'hl.dsp.dpms({ action = \"disable\" })'\n    on-resume = hyprctl dispatch 'hl.dsp.dpms({ action = \"enable\" })'\n}\n";

    #[test]
    fn a_missing_file_is_created_with_the_written_keys() {
        let root = std::env::temp_dir().join(format!("proscenio-hypridle-{}", std::process::id()));
        let path = root.join("hypr/hypridle.conf");
        let _ = std::fs::remove_dir_all(&root);
        let result = write_to(
            &path,
            &[("lock".to_owned(), 600)],
            &[("lock_cmd".to_owned(), Some("hyprlock".to_owned()))],
        );
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&root);
        result.unwrap();
        assert_eq!(
            text,
            "general {\n    lock_cmd = hyprlock\n}\n\nlistener {\n    timeout = 600\n    on-timeout = loginctl lock-session\n}\n"
        );
    }

    #[test]
    fn a_created_file_without_a_lock_command_gets_the_shell_lock() {
        let root =
            std::env::temp_dir().join(format!("proscenio-hypridle-lock-{}", std::process::id()));
        let path = root.join("hypridle.conf");
        let _ = std::fs::remove_dir_all(&root);
        let result = write_to(&path, &[("lock".to_owned(), 600)], &[]);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&root);
        assert!(result.unwrap());
        assert_eq!(
            text,
            "general {\n    lock_cmd = hyprctl dispatch 'hl.dsp.global(\"proscenio:lock\")'\n}\n\nlistener {\n    timeout = 600\n    on-timeout = loginctl lock-session\n}\n"
        );
    }

    #[test]
    fn a_write_that_changes_nothing_does_not_create_the_file() {
        let root =
            std::env::temp_dir().join(format!("proscenio-hypridle-noop-{}", std::process::id()));
        let path = root.join("hypridle.conf");
        let _ = std::fs::remove_dir_all(&root);
        let result = write_to(
            &path,
            &[("lock".to_owned(), 0)],
            &[("lock_cmd".to_owned(), None)],
        );
        let has_file = path.exists();
        let _ = std::fs::remove_dir_all(&root);
        assert!(!result.unwrap());
        assert!(!has_file);
    }

    #[test]
    fn timeouts_are_read_by_what_their_listener_does() {
        let timeouts_by_kind = timeouts(FILE);
        assert_eq!(timeouts_by_kind.get("lock"), Some(&1800));
        assert_eq!(timeouts_by_kind.get("screen"), Some(&2400));
        assert_eq!(timeouts_by_kind.get("suspend"), None);
    }

    #[test]
    fn only_a_lock_command_makes_a_lock_listener() {
        let text = "listener {\n    timeout = 60\n    on-timeout = rfkill block bluetooth\n}\n\nlistener {\n    timeout = 90\n    on-timeout = hyprctl dispatch 'hl.dsp.global(\"proscenio:lock\")'\n}\n";
        let timeouts_by_kind = timeouts(text);
        assert_eq!(timeouts_by_kind.get("lock"), Some(&90));
        assert_eq!(timeouts_by_kind.len(), 1);
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
        let added_text = set_general(FILE, "before_sleep_cmd", Some("loginctl lock-session"));
        assert!(added_text.contains(
            "general {\n    lock_cmd = $lock_cmd\n    before_sleep_cmd = loginctl lock-session\n}\n"
        ));
        assert_eq!(
            general(&added_text)
                .get("before_sleep_cmd")
                .map(String::as_str),
            Some("loginctl lock-session")
        );
        let changed_text = set_general(&added_text, "lock_cmd", Some("hyprlock"));
        assert!(changed_text.contains("general {\n    lock_cmd = hyprlock\n    before_sleep_cmd"));
        assert_eq!(set_general(&added_text, "before_sleep_cmd", None), FILE);
        assert_eq!(
            set_general(FILE, "lock_cmd", None),
            FILE.replace("general {\n    lock_cmd = $lock_cmd\n}", "general {\n}")
        );
    }

    #[test]
    fn a_key_goes_into_an_empty_general_block() {
        let text = "general {\n}\n\nlistener {\n    timeout = 60\n    on-timeout = loginctl lock-session\n}\n";
        assert!(general(text).is_empty());
        assert_eq!(
            set_general(text, "before_sleep_cmd", Some("loginctl lock-session")),
            "general {\n    before_sleep_cmd = loginctl lock-session\n}\n\nlistener {\n    timeout = 60\n    on-timeout = loginctl lock-session\n}\n"
        );
        let indented_config = text.replacen("general {\n}", "general {\n    lock_cmd = x\n  }", 1);
        assert_eq!(general(&indented_config).len(), 1);
    }
}
