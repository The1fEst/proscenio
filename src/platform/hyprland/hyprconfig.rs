use serde_json::{Map, Value};
use std::iter::Peekable;
use std::path::{Path, PathBuf};
use std::str::CharIndices;

use crate::platform::hyprland::hypr;

const MAX_LINK_HOPS: usize = 40;
const KINDS: [&str; 6] = ["int", "float", "bool", "str", "css", "vec2"];

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

pub(super) fn config_dir() -> PathBuf {
    gtk4::glib::user_config_dir().join("hypr")
}

pub fn path(area: Area) -> PathBuf {
    settings_path(&format!("{}.lua", area.name()))
}

pub fn settings_path(name: &str) -> PathBuf {
    config_dir().join("settings").join(name)
}

pub fn read(area: Area) -> String {
    std::fs::read_to_string(path(area)).unwrap_or_default()
}

pub fn read_for_edit(area: Area) -> std::io::Result<String> {
    read_file_for_edit(&path(area))
}

pub fn read_file_for_edit(path: &Path) -> std::io::Result<String> {
    match std::fs::read_to_string(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        result => result,
    }
}

pub fn write(area: Area, text: &str) -> std::io::Result<()> {
    replace_file(&path(area), text)
}

pub fn replace_file(path: &Path, text: &str) -> std::io::Result<()> {
    let target = resolve_link(path);
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let permissions = match std::fs::metadata(&target) {
        Ok(metadata) if metadata.permissions().readonly() => {
            return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };
    let mut staging_path = target.clone().into_os_string();
    staging_path.push(".part");
    let staging_path = PathBuf::from(staging_path);
    match std::fs::remove_file(&staging_path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(error),
        _ => {}
    }
    let staging_file = match create_staging_file(&staging_path) {
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
            return std::fs::write(&target, text);
        }
        result => result?,
    };
    let result = write_synced(staging_file, text, permissions)
        .and_then(|()| std::fs::rename(&staging_path, &target));
    if result.is_err() {
        let _ = std::fs::remove_file(&staging_path);
        return result;
    }
    if let Some(parent) = target.parent()
        && let Ok(directory) = std::fs::File::open(parent)
    {
        let _ = directory.sync_all();
    }
    Ok(())
}

pub fn replace_if_changed(path: &Path, edit: impl FnOnce(&str) -> String) -> std::io::Result<bool> {
    let text = read_file_for_edit(path)?;
    let new_text = edit(&text);
    if new_text == text {
        return Ok(false);
    }
    replace_file(path, &new_text).map(|()| true)
}

fn resolve_link(path: &Path) -> PathBuf {
    if let Ok(target) = std::fs::canonicalize(path) {
        return target;
    }
    let mut current = path.to_path_buf();
    for _ in 0..MAX_LINK_HOPS {
        let Ok(next) = std::fs::read_link(&current) else {
            break;
        };
        current = match current.parent() {
            Some(parent) => parent.join(next),
            None => next,
        };
    }
    current
}

fn create_staging_file(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

fn write_synced(
    mut file: std::fs::File,
    text: &str,
    permissions: Option<std::fs::Permissions>,
) -> std::io::Result<()> {
    use std::io::Write;
    if let Some(permissions) = permissions {
        file.set_permissions(permissions)?;
    }
    file.write_all(text.as_bytes())?;
    file.sync_all()
}

pub fn read_options(names: &[&str]) -> Map<String, Value> {
    let mut options = Map::new();
    for name in names {
        let Some(reply) = hypr::json(&format!("getoption {name}")) else {
            continue;
        };
        if let Some(value) = KINDS.iter().find_map(|kind| reply.get(*kind)) {
            options.insert((*name).to_owned(), value.clone());
        }
    }
    options
}

pub fn write_options(area: Area, pairs: &[(String, String)]) -> std::io::Result<()> {
    let mut text = read_for_edit(area)?;
    for (option, value) in pairs {
        text = set_option(&text, option, value);
    }
    write(area, &text)
}

pub fn lua_string(text: &str) -> String {
    let mut literal = String::from("\"");
    for character in text.chars() {
        match character {
            '\\' => literal.push_str("\\\\"),
            '"' => literal.push_str("\\\""),
            '\n' => literal.push_str("\\n"),
            '\r' => literal.push_str("\\r"),
            other => literal.push(other),
        }
    }
    literal.push('"');
    literal
}

pub fn read_lua_string(text: &str) -> Option<(String, &str)> {
    let rest = text.strip_prefix('"')?;
    let mut characters = rest.char_indices().peekable();
    let mut value = String::new();
    while let Some((offset, character)) = characters.next() {
        if character == '"' {
            return Some((value, &rest[offset + 1..]));
        }
        if character != '\\' {
            value.push(character);
            continue;
        }
        let (_, escape) = characters.next()?;
        let read_digits =
            |characters: &mut Peekable<CharIndices>, radix: u32, max_count, mut number: u32| {
                for _ in 0..max_count {
                    let Some((_, digit)) =
                        characters.next_if(|(_, candidate)| candidate.is_digit(radix))
                    else {
                        break;
                    };
                    number = number * radix + digit.to_digit(radix).unwrap_or(0);
                }
                number
            };
        match escape {
            'a' => value.push('\u{7}'),
            'b' => value.push('\u{8}'),
            'f' => value.push('\u{c}'),
            'n' => value.push('\n'),
            'r' => value.push('\r'),
            't' => value.push('\t'),
            'v' => value.push('\u{b}'),
            'x' => value.push(char::from_u32(read_digits(&mut characters, 16, 2, 0))?),
            'u' => {
                characters.next_if(|(_, candidate)| *candidate == '{')?;
                value.push(char::from_u32(read_digits(&mut characters, 16, 8, 0))?);
                characters.next_if(|(_, candidate)| *candidate == '}')?;
            }
            'z' => {
                while characters
                    .next_if(|(_, candidate)| candidate.is_whitespace())
                    .is_some()
                {}
            }
            first_digit if first_digit.is_ascii_digit() => {
                let initial_value = first_digit.to_digit(10)?;
                value.push(char::from_u32(read_digits(
                    &mut characters,
                    10,
                    2,
                    initial_value,
                ))?);
            }
            other => value.push(other),
        }
    }
    None
}

pub fn render(value: &str) -> String {
    let is_number = !value.starts_with('+') && value.parse::<f64>().is_ok_and(f64::is_finite);
    if value == "true" || value == "false" || is_number {
        value.to_owned()
    } else {
        lua_string(value)
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

pub(super) fn prefix_for(option: &str) -> String {
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

pub const NO_DIM_FULLSCREEN: Lines = Lines {
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

#[derive(Debug, PartialEq)]
pub struct BorderColor {
    pub key: String,
    pub alpha: u8,
}

const BORDER_GUARD: &str = "if border_colors";
const SHADOW_MARKER: &str = "hl.config({ decoration = { shadow = { color = \"rgba(000000";
const SHADOW_DEFAULT_ALPHA: u8 = 0x20;

fn parse_alpha(text: &str) -> Option<u8> {
    proscenio_protocol::unhex(text.get(..2)?)?.first().copied()
}

fn with_line(text: &str, is_target: impl Fn(&str) -> bool, line: &str) -> String {
    let mut has_target_line = false;
    let mut lines: Vec<String> = text
        .lines()
        .map(|current_line| {
            if is_target(current_line) {
                has_target_line = true;
                line.to_owned()
            } else {
                current_line.to_owned()
            }
        })
        .collect();
    if !has_target_line {
        lines.push(line.to_owned());
    }
    let mut new_text = lines.join("\n");
    new_text.push('\n');
    new_text
}

fn border_marker(border: &Border) -> String {
    format!("col = {{ {} = \"rgba(\" .. border_colors.", border.field)
}

fn is_border_line(line: &str, marker: &str) -> bool {
    line.starts_with(BORDER_GUARD) && line.contains(marker)
}

fn border_color_in(text: &str, border: &Border) -> BorderColor {
    let marker = border_marker(border);
    text.lines()
        .filter(|line| is_border_line(line, &marker))
        .find_map(|line| {
            let (key, rest) = line.split_once(&marker)?.1.split_once(" .. \"")?;
            let alpha = parse_alpha(rest)?;
            Some(BorderColor {
                key: key.to_owned(),
                alpha,
            })
        })
        .unwrap_or_else(|| BorderColor {
            key: border.color.to_owned(),
            alpha: border.default,
        })
}

fn with_border_color(text: &str, border: &Border, color: &BorderColor) -> String {
    let marker = border_marker(border);
    let key = &color.key;
    let line = format!(
        "{BORDER_GUARD} and border_colors.{key} then hl.config({{ general = {{ {marker}{key} .. \"{:02X})\" }} }} }}) end",
        color.alpha
    );
    with_line(
        text,
        |current_line| is_border_line(current_line, &marker),
        &line,
    )
}

pub fn border_color(border: &Border) -> BorderColor {
    border_color_in(&read(Area::Appearance), border)
}

pub fn set_border_color(border: &Border, color: &BorderColor) -> std::io::Result<()> {
    write(
        Area::Appearance,
        &with_border_color(&read_for_edit(Area::Appearance)?, border, color),
    )
}

fn shadow_alpha_in(text: &str) -> u8 {
    text.lines()
        .find_map(|line| parse_alpha(line.strip_prefix(SHADOW_MARKER)?))
        .unwrap_or(SHADOW_DEFAULT_ALPHA)
}

fn with_shadow_alpha(text: &str, alpha: u8) -> String {
    let line = format!("{SHADOW_MARKER}{alpha:02X})\" }} }} }})");
    with_line(
        text,
        |current_line| current_line.starts_with(SHADOW_MARKER),
        &line,
    )
}

pub fn shadow_alpha() -> u8 {
    shadow_alpha_in(&read(Area::Appearance))
}

pub fn set_shadow_alpha(alpha: u8) -> std::io::Result<()> {
    write(
        Area::Appearance,
        &with_shadow_alpha(&read_for_edit(Area::Appearance)?, alpha),
    )
}

fn has_lines(text: &str, lines: &[&str]) -> bool {
    lines
        .iter()
        .all(|expected_line| text.lines().any(|line| line.trim() == *expected_line))
}

fn with_lines(text: &str, lines: &[&str], is_enabled: bool) -> String {
    let mut remaining_lines: Vec<&str> = text
        .lines()
        .filter(|line| !lines.contains(&line.trim()))
        .collect();
    while remaining_lines
        .last()
        .is_some_and(|line| line.trim().is_empty())
    {
        remaining_lines.pop();
    }
    if is_enabled {
        remaining_lines.extend_from_slice(lines);
    }
    let mut new_text = remaining_lines.join("\n");
    new_text.push('\n');
    new_text
}

pub fn are_lines_present(line_set: &Lines) -> bool {
    has_lines(&read(line_set.area), line_set.lines)
}

pub fn set_lines(line_set: &Lines, is_enabled: bool) -> std::io::Result<()> {
    write(
        line_set.area,
        &with_lines(&read_for_edit(line_set.area)?, line_set.lines, is_enabled),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settings_file_that_cannot_be_read_is_not_edited_as_empty() {
        let root =
            std::env::temp_dir().join(format!("proscenio-hyprconfig-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("unreadable.lua")).unwrap();

        assert_eq!(read_file_for_edit(&root.join("missing.lua")).unwrap(), "");
        assert!(read_file_for_edit(&root.join("unreadable.lua")).is_err());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_settings_file_is_replaced_through_its_link_and_leaves_no_staging_file() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("proscenio-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let real_path = root.join("dots/displays.lua");
        let link_path = root.join("settings/displays.lua");
        let dangling_target = root.join("dots/apps.lua");
        let dangling_link = root.join("settings/apps.lua");
        std::fs::create_dir_all(real_path.parent().unwrap()).unwrap();
        std::fs::create_dir_all(link_path.parent().unwrap()).unwrap();
        std::fs::write(&real_path, "old\n").unwrap();
        std::fs::set_permissions(&real_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&real_path, &link_path).unwrap();
        std::os::unix::fs::symlink(&dangling_target, &dangling_link).unwrap();

        replace_file(&link_path, "new\n").unwrap();
        replace_file(&dangling_link, "rules\n").unwrap();

        assert!(link_path.is_symlink() && dangling_link.is_symlink());
        assert_eq!(std::fs::read_to_string(&real_path).unwrap(), "new\n");
        assert_eq!(
            std::fs::read_to_string(&dangling_target).unwrap(),
            "rules\n"
        );
        let mode = std::fs::metadata(&real_path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        let mut names: Vec<String> = std::fs::read_dir(real_path.parent().unwrap())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["apps.lua", "displays.lua"]);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn smart_gaps_are_four_lines_added_and_removed_together() {
        let text = "-- mine\nhl.config({ general = { gaps_in = 4 } })\n";
        let lines = SMART_GAPS.lines;
        let text_with_lines = with_lines(text, lines, true);
        assert!(has_lines(&text_with_lines, lines));
        assert!(text_with_lines.starts_with(text));
        assert_eq!(with_lines(&text_with_lines, lines, true), text_with_lines);
        assert_eq!(with_lines(&text_with_lines, lines, false), text);
        assert!(!has_lines(&format!("{text}{}\n", lines[0]), lines));
    }

    #[test]
    fn a_shadow_color_is_one_black_line_that_stores_only_its_alpha() {
        let text = "hl.config({ general = { gaps_in = 4 } })\n";
        assert_eq!(shadow_alpha_in(text), SHADOW_DEFAULT_ALPHA);
        let colored_text = with_shadow_alpha(text, 0x80);
        assert_eq!(
            colored_text,
            "hl.config({ general = { gaps_in = 4 } })\nhl.config({ decoration = { shadow = { color = \"rgba(00000080)\" } } })\n"
        );
        assert_eq!(shadow_alpha_in(&colored_text), 0x80);
        let recolored_text = with_shadow_alpha(&colored_text, 0x05);
        assert_eq!(
            recolored_text,
            "hl.config({ general = { gaps_in = 4 } })\nhl.config({ decoration = { shadow = { color = \"rgba(00000005)\" } } })\n"
        );
        assert_eq!(shadow_alpha_in(&recolored_text), 0x05);
        assert_eq!(
            shadow_alpha_in(
                "hl.config({ decoration = { shadow = { color = \"rgba(000000ff)\" } } })\n"
            ),
            0xFF
        );
    }

    #[test]
    fn a_border_color_is_one_guarded_line_over_a_generated_palette_color() {
        let color = |key: &str, alpha: u8| BorderColor {
            key: key.to_owned(),
            alpha,
        };
        let text = "hl.config({ general = { gaps_in = 4 } })\n";
        assert_eq!(border_color_in(text, &ACTIVE_BORDER), color("active", 0x77));
        let colored_text = with_border_color(text, &ACTIVE_BORDER, &color("active", 0xCC));
        assert_eq!(
            colored_text,
            "hl.config({ general = { gaps_in = 4 } })\nif border_colors and border_colors.active then hl.config({ general = { col = { active_border = \"rgba(\" .. border_colors.active .. \"CC)\" } } }) end\n"
        );
        assert_eq!(
            border_color_in(&colored_text, &ACTIVE_BORDER),
            color("active", 0xCC)
        );
        assert_eq!(
            border_color_in(&colored_text, &INACTIVE_BORDER),
            color("inactive", 0x33)
        );
        let recolored_text =
            with_border_color(&colored_text, &ACTIVE_BORDER, &color("secondary", 0xFF));
        assert_eq!(recolored_text.lines().count(), 2);
        assert_eq!(
            border_color_in(&recolored_text, &ACTIVE_BORDER),
            color("secondary", 0xFF)
        );
    }

    #[test]
    fn a_border_line_without_the_palette_key_guard_is_read_and_replaced() {
        let text = "if border_colors then hl.config({ general = { col = { inactive_border = \"rgba(\" .. border_colors.inactive .. \"80)\" } } }) end\n";
        assert_eq!(
            border_color_in(text, &INACTIVE_BORDER),
            BorderColor {
                key: "inactive".to_owned(),
                alpha: 0x80,
            }
        );
        let colored_text = with_border_color(
            text,
            &INACTIVE_BORDER,
            &BorderColor {
                key: "primary".to_owned(),
                alpha: 0x80,
            },
        );
        assert_eq!(
            colored_text,
            "if border_colors and border_colors.primary then hl.config({ general = { col = { inactive_border = \"rgba(\" .. border_colors.primary .. \"80)\" } } }) end\n"
        );
    }

    #[test]
    fn a_border_alpha_with_a_sign_falls_back_to_the_default() {
        let text = "if border_colors and border_colors.active then hl.config({ general = { col = { active_border = \"rgba(\" .. border_colors.active .. \"+8)\" } } }) end\n";
        assert_eq!(
            border_color_in(text, &ACTIVE_BORDER),
            BorderColor {
                key: "active".to_owned(),
                alpha: 0x77,
            }
        );
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
    fn values_that_are_not_lua_numbers_are_written_as_escaped_strings() {
        assert_eq!(render("true"), "true");
        assert_eq!(render("-0.5"), "-0.5");
        assert_eq!(render("1e3"), "1e3");
        assert_eq!(render("nan"), "\"nan\"");
        assert_eq!(render("inf"), "\"inf\"");
        assert_eq!(render("+5"), "\"+5\"");
        assert_eq!(render("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn a_lua_string_is_read_with_its_escapes() {
        assert_eq!(
            read_lua_string("\"a\\tb\\65\\x42\\u{43}\\z   d\\\\\\\"\\100\\0\" })"),
            Some(("a\tbABCd\\\"d\0".to_owned(), " })"))
        );
        assert_eq!(
            read_lua_string(&lua_string("x\"\\\n\ry")),
            Some(("x\"\\\n\ry".to_owned(), ""))
        );
        assert_eq!(read_lua_string("\"open"), None);
    }
}
