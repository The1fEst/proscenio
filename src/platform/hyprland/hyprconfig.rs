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

const DEFAULT_OPTIONS: [(Area, &str, &str); 1] =
    [(Area::Multitasking, "scrolling:focus_fit_method", "0")];

fn add_default_option(
    text: &str,
    option: &str,
    value: &str,
    may_be_set_in_hyprland: impl FnOnce(&str) -> bool,
) -> Option<String> {
    let prefix = prefix_for(option);
    if text.lines().any(|line| line.starts_with(&prefix)) || may_be_set_in_hyprland(option) {
        return None;
    }
    Some(set_option(text, option, value))
}

fn may_be_set_in_hyprland(option: &str) -> bool {
    let has_config_errors =
        hypr::request("configerrors").is_none_or(|reply| !reply.trim().is_empty());
    has_config_errors
        || hypr::json(&format!("getoption {option}"))
            .and_then(|reply| reply.get("set").and_then(Value::as_bool))
            .unwrap_or(true)
}

pub fn apply_default_options() -> bool {
    let mut is_written = false;
    for (area, option, value) in DEFAULT_OPTIONS {
        let result = read_for_edit(area).and_then(|text| {
            match add_default_option(&text, option, value, may_be_set_in_hyprland) {
                Some(new_text) => write(area, &new_text).map(|()| true),
                None => Ok(false),
            }
        });
        match result {
            Ok(is_option_written) => is_written |= is_option_written,
            Err(error) => eprintln!("{}.lua: {error}", area.name()),
        }
    }
    is_written
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

pub const TITLE_BUTTONS_ON_LEFT: Lines = Lines {
    area: Area::Appearance,
    lines: &[
        "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({ plugin = { proscenio_bars = { button_side = \"left\" } } }) end end",
    ],
};

pub const TITLE_BARS_ON_ALL_WINDOWS: Lines = Lines {
    area: Area::Appearance,
    lines: &[
        "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({ plugin = { proscenio_bars = { windows = \"all\" } } }) end end",
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
const BAR_COLOR_MARKER: &str = "proscenio_bars = { bar_color = ";
const SCROLLING_PEEK_MARKER: &str = "proscenio_bars = { scrolling_peek = ";
const SCROLLING_PEEK_DEFAULT: i64 = 40;
const TOGGLED_LAYOUT_MARKER: &str = "proscenio_bars = { toggle_layout = ";
pub const SCROLLING_LAYOUT: &str = "scrolling";
pub const TOGGLED_LAYOUTS: [&str; 2] = [SCROLLING_LAYOUT, "monocle"];

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

fn scrolling_peek_in(text: &str) -> i64 {
    text.lines()
        .find_map(|line| {
            let value = line.split_once(SCROLLING_PEEK_MARKER)?.1;
            let digit_count = value.chars().take_while(char::is_ascii_digit).count();
            value[..digit_count].parse().ok()
        })
        .unwrap_or(SCROLLING_PEEK_DEFAULT)
}

fn with_scrolling_peek(text: &str, peek: i64) -> String {
    let line = format!(
        "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({{ plugin = {{ {SCROLLING_PEEK_MARKER}{peek} }} }} }}) end end"
    );
    with_line(
        text,
        |current_line| current_line.contains(SCROLLING_PEEK_MARKER),
        &line,
    )
}

pub fn scrolling_peek() -> i64 {
    scrolling_peek_in(&read(Area::Appearance))
}

pub fn set_scrolling_peek(peek: i64) -> std::io::Result<()> {
    write(
        Area::Appearance,
        &with_scrolling_peek(&read_for_edit(Area::Appearance)?, peek),
    )
}

fn toggled_layout_in(text: &str) -> &'static str {
    text.lines()
        .find_map(|line| {
            let value = line
                .split_once(TOGGLED_LAYOUT_MARKER)?
                .1
                .strip_prefix('"')?;
            let layout = value.split_once('"')?.0;
            TOGGLED_LAYOUTS.into_iter().find(|name| *name == layout)
        })
        .unwrap_or(SCROLLING_LAYOUT)
}

fn with_toggled_layout(text: &str, layout: &str) -> String {
    let line = format!(
        "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({{ plugin = {{ {TOGGLED_LAYOUT_MARKER}\"{layout}\" }} }} }}) end end"
    );
    with_line(
        text,
        |current_line| current_line.contains(TOGGLED_LAYOUT_MARKER),
        &line,
    )
}

pub fn toggled_layout() -> &'static str {
    toggled_layout_in(&read(Area::Appearance))
}

pub fn set_toggled_layout(layout: &str) -> std::io::Result<()> {
    write(
        Area::Appearance,
        &with_toggled_layout(&read_for_edit(Area::Appearance)?, layout),
    )
}

fn with_bar_colors(text: &str, bar_color: &str, other_colors: &[(&str, String)]) -> String {
    let format_color = |color: &str| {
        format!(
            "\"rgb({})\"",
            color.trim_start_matches('#').to_ascii_lowercase()
        )
    };
    let bar_color = format_color(bar_color);
    let mut options = format!("{bar_color}, inactive_bar_color = {bar_color}");
    for (option, color) in other_colors {
        options.push_str(&format!(", {option} = {}", format_color(color)));
    }
    let line = format!(
        "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({{ plugin = {{ {BAR_COLOR_MARKER}{options} }} }} }}) end end"
    );
    with_line(text, |line| line.contains(BAR_COLOR_MARKER), &line)
}

pub fn set_bar_colors(bar_color: &str, other_colors: &[(&str, String)]) -> std::io::Result<bool> {
    replace_if_changed(&path(Area::Appearance), |text| {
        with_bar_colors(text, bar_color, other_colors)
    })
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

const SETTINGS_LOADER: &str = r#"-- What the proscenio settings app writes, over everything above --
do
    local config_dir = package.path:match("^(.-)/%?%.lua")
    local listing = config_dir and io.popen("ls -1 '" .. (config_dir:gsub("'", "'\\''")) .. "/settings' 2>/dev/null")
    if listing then
        for name in listing:lines() do
            local area = name:match("^(.+)%.lua$")
            if area then
                require("settings." .. area)
            end
        end
        listing:close()
    end
end
"#;

fn strip_block_comments(text: &str) -> String {
    let mut code = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("--[[") {
        code.push_str(&rest[..start]);
        let comment = &rest[start..];
        rest = comment.find("]]").map_or("", |end| &comment[end + 2..]);
    }
    code.push_str(rest);
    code
}

fn loads_settings(config_text: &str) -> bool {
    strip_block_comments(config_text).lines().any(|line| {
        let code = line.split("--").next().unwrap_or_default();
        code.contains("require")
            && ["\"settings.\"", "'settings.'"]
                .iter()
                .any(|module_prefix| {
                    code.match_indices(module_prefix).any(|(index, _)| {
                        code[index + module_prefix.len()..]
                            .trim_start()
                            .starts_with("..")
                    })
                })
    })
}

fn with_settings_loader(config_text: &str) -> String {
    if loads_settings(config_text) {
        return config_text.to_owned();
    }
    let config_text = config_text.trim_end_matches('\n');
    if config_text.is_empty() {
        return SETTINGS_LOADER.to_owned();
    }
    format!("{config_text}\n\n{SETTINGS_LOADER}")
}

pub fn add_settings_loader() -> std::io::Result<bool> {
    let path = config_dir().join("hyprland.lua");
    if !path.is_file() {
        return Ok(false);
    }
    replace_if_changed(&path, with_settings_loader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_config_that_does_not_load_the_settings_gets_the_loader_at_its_end() {
        let dots_block = concat!(
            "-- What the settings app writes, over the defaults and every custom configuration --\n",
            "if is_file_exists(HOME .. \"/.config/hypr/settings.lua\") then\n",
            "    require(\"settings\")\n",
            "end\n",
            "local settings = io.popen(\"ls -1 \\\"\" .. HOME .. \"/.config/hypr/settings\\\" 2>/dev/null\")\n",
            "if settings then\n",
            "    for name in settings:lines() do\n",
            "        local area = name:match(\"^(.+)%.lua$\")\n",
            "        if area then\n",
            "            require(\"settings.\" .. area)\n",
            "        end\n",
            "    end\n",
            "    settings:close()\n",
            "end\n",
        );
        let dots_config =
            format!("require(\"hyprland.general\")\n{dots_block}require(\"custom.keybinds\")\n");
        let own_config = "hl.config({ general = { gaps_in = 4 } })\n";
        let own_config_with_loader =
            format!("hl.config({{ general = {{ gaps_in = 4 }} }})\n\n{SETTINGS_LOADER}");
        let cases = [
            ("the dots' block", dots_config.as_str(), dots_config.clone()),
            (
                "the loader itself",
                SETTINGS_LOADER,
                SETTINGS_LOADER.to_owned(),
            ),
            (
                "the folder in single quotes and without spaces",
                "require('settings.'..area)\n",
                "require('settings.'..area)\n".to_owned(),
            ),
            (
                "one area only",
                "require('settings.appearance')\n",
                format!("require('settings.appearance')\n\n{SETTINGS_LOADER}"),
            ),
            (
                "a loader in a block comment",
                "--[[\nrequire(\"settings.\" .. area)\n]]\n",
                format!("--[[\nrequire(\"settings.\" .. area)\n]]\n\n{SETTINGS_LOADER}"),
            ),
            (
                "a loader after a block comment",
                "--[[ old ]] require(\"settings.\" .. area)\n",
                "--[[ old ]] require(\"settings.\" .. area)\n".to_owned(),
            ),
            (
                "settings text that is not a module",
                "require(\"theme\").apply(\"settings.json\")\n",
                format!("require(\"theme\").apply(\"settings.json\")\n\n{SETTINGS_LOADER}"),
            ),
            ("no loader", own_config, own_config_with_loader.clone()),
            (
                "no trailing newline",
                own_config.trim_end(),
                own_config_with_loader.clone(),
            ),
            (
                "only the legacy single file",
                "require(\"settings\")\n",
                format!("require(\"settings\")\n\n{SETTINGS_LOADER}"),
            ),
            (
                "a commented-out require",
                "-- require(\"settings.\" .. area)\n",
                format!("-- require(\"settings.\" .. area)\n\n{SETTINGS_LOADER}"),
            ),
            (
                "a module whose name ends in settings",
                "require(\"mysettings.appearance\")\n",
                format!("require(\"mysettings.appearance\")\n\n{SETTINGS_LOADER}"),
            ),
            ("empty", "", SETTINGS_LOADER.to_owned()),
        ];
        for (name, config_text, expected) in cases {
            assert_eq!(with_settings_loader(config_text), expected, "{name}");
        }
    }

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
    fn a_default_option_is_added_only_when_nothing_sets_it() {
        let text = "hl.config({ general = { gaps_in = 4 } })\n";
        let text_with_choice = "hl.config({ scrolling = { focus_fit_method = 1 } })\n";
        let cases = [
            (
                "nothing sets it",
                text,
                false,
                Some(format!(
                    "{text}hl.config({{ scrolling = {{ focus_fit_method = 0 }} }})\n"
                )),
            ),
            ("Hyprland reports it set", text, true, None),
            ("the file has a line for it", text_with_choice, false, None),
        ];
        for (name, text, is_set_in_hyprland, expected) in cases {
            assert_eq!(
                add_default_option(text, "scrolling:focus_fit_method", "0", |_| {
                    is_set_in_hyprland
                }),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn the_scrolling_peek_is_one_guarded_line_that_stores_its_pixels() {
        let build_line = |peek: i64| {
            format!(
                "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({{ plugin = {{ proscenio_bars = {{ scrolling_peek = {peek} }} }} }}) end end"
            )
        };
        let other_line = TITLE_BUTTONS_ON_LEFT.lines[0];
        let text = format!("{other_line}\n");
        assert_eq!(scrolling_peek_in(&text), SCROLLING_PEEK_DEFAULT);
        let text_with_peek = with_scrolling_peek(&text, 48);
        assert_eq!(
            text_with_peek,
            format!("{other_line}\n{}\n", build_line(48))
        );
        assert_eq!(scrolling_peek_in(&text_with_peek), 48);
        let text_with_new_peek = with_scrolling_peek(&text_with_peek, 0);
        assert_eq!(
            text_with_new_peek,
            format!("{other_line}\n{}\n", build_line(0))
        );
        assert_eq!(scrolling_peek_in(&text_with_new_peek), 0);
    }

    #[test]
    fn the_toggled_layout_is_one_guarded_line_that_stores_its_name() {
        let build_line = |layout: &str| {
            format!(
                "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({{ plugin = {{ proscenio_bars = {{ toggle_layout = \"{layout}\" }} }} }}) end end"
            )
        };
        let other_line = TITLE_BUTTONS_ON_LEFT.lines[0];
        let text = format!("{other_line}\n");
        assert_eq!(toggled_layout_in(&text), "scrolling");
        let text_with_monocle = with_toggled_layout(&text, "monocle");
        assert_eq!(
            text_with_monocle,
            format!("{other_line}\n{}\n", build_line("monocle"))
        );
        assert_eq!(toggled_layout_in(&text_with_monocle), "monocle");
        let text_with_scrolling = with_toggled_layout(&text_with_monocle, "scrolling");
        assert_eq!(
            text_with_scrolling,
            format!("{other_line}\n{}\n", build_line("scrolling"))
        );
        assert_eq!(toggled_layout_in(&text_with_scrolling), "scrolling");
        assert_eq!(
            toggled_layout_in(&format!("{}\n", build_line("dwindle"))),
            "scrolling"
        );
    }

    #[test]
    fn the_bar_colors_are_one_guarded_line_that_replaces_the_last_one() {
        let build_line = |options: &str| {
            format!(
                "for _, plugin in ipairs(hl.get_loaded_plugins()) do if plugin.name == \"proscenio_bars\" then hl.config({{ plugin = {{ proscenio_bars = {{ {options} }} }} }}) end end"
            )
        };
        let bar_only_line =
            build_line("bar_color = \"rgb(111111)\", inactive_bar_color = \"rgb(111111)\"");
        let full_line = build_line(
            "bar_color = \"rgb(1b1d27)\", inactive_bar_color = \"rgb(1b1d27)\", title_color = \"rgb(e6e0e9)\", close_color = \"rgb(f2b8b5)\"",
        );
        let other_lines = format!("-- mine\n{}\n", TITLE_BUTTONS_ON_LEFT.lines[0]);
        let text_with_bar_only = format!("{other_lines}{bar_only_line}\n");
        let text_with_full_line = format!("{other_lines}{full_line}\n");
        let cases = [
            ("added", other_lines.as_str()),
            ("replaced from the bar colors alone", &text_with_bar_only),
            ("same colors", &text_with_full_line),
            ("empty file", ""),
        ];
        let other_colors = [
            ("title_color", "#E6E0E9".to_owned()),
            ("close_color", "f2b8b5".to_owned()),
        ];
        for (name, text) in cases {
            let expected = if text.is_empty() {
                format!("{full_line}\n")
            } else {
                text_with_full_line.clone()
            };
            assert_eq!(
                with_bar_colors(text, "#1B1D27", &other_colors),
                expected,
                "{name}"
            );
        }
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
