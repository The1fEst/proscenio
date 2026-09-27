use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use gtk4::glib;

use crate::platform::hyprconfig::settings_path;

const SCHEMA: &str = "org.gnome.desktop.interface";
const SETTINGS: &str = "[Settings]";
const HYPR_HEADER: &str = "-- Written by the settings app. Hyprland sources it after its own configuration and\n-- before the files in custom/, so anything set here can still be overridden there.\n";

pub const ROLES: [(&str, &str, &str); 6] = [
    ("general", "[General]", "font"),
    ("fixed", "[General]", "fixed"),
    ("small", "[General]", "smallestReadableFont"),
    ("toolbar", "[General]", "toolBarFont"),
    ("menu", "[General]", "menuFont"),
    ("title", "[WM]", "activeFont"),
];

const WEIGHTS: [(&str, u32); 21] = [
    ("Thin", 100),
    ("Hairline", 100),
    ("ExtraLight", 200),
    ("UltraLight", 200),
    ("Ultralight", 200),
    ("Light", 300),
    ("Regular", 400),
    ("Normal", 400),
    ("Book", 400),
    ("", 400),
    ("Medium", 500),
    ("DemiBold", 600),
    ("SemiBold", 600),
    ("Semibold", 600),
    ("Demibold", 600),
    ("Bold", 700),
    ("ExtraBold", 800),
    ("UltraBold", 800),
    ("Ultrabold", 800),
    ("Black", 900),
    ("Heavy", 900),
];

const WEIGHT_NAMES: [(&str, u32); 9] = [
    ("Thin", 100),
    ("ExtraLight", 200),
    ("Light", 300),
    ("Regular", 400),
    ("Medium", 500),
    ("SemiBold", 600),
    ("Bold", 700),
    ("ExtraBold", 800),
    ("Black", 900),
];

static WRITING: Mutex<()> = Mutex::new(());

pub type Families = BTreeMap<String, Vec<String>>;

#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub family: String,
    pub style: String,
    pub size: i64,
}

impl Default for Font {
    fn default() -> Self {
        Font {
            family: String::new(),
            style: String::new(),
            size: 10,
        }
    }
}

#[derive(Clone, Debug)]
pub struct State {
    pub cursor_theme: String,
    pub cursor_size: i64,
    pub fonts: BTreeMap<&'static str, Font>,
    pub icon_theme: String,
    pub gtk_theme: String,
    pub qt_style: String,
    pub cursor_themes: Vec<String>,
    pub icon_themes: Vec<String>,
    pub gtk_themes: Vec<String>,
    pub qt_styles: Vec<String>,
}

impl Default for State {
    fn default() -> Self {
        State {
            cursor_theme: String::new(),
            cursor_size: 24,
            fonts: BTreeMap::new(),
            icon_theme: String::new(),
            gtk_theme: String::new(),
            qt_style: String::new(),
            cursor_themes: Vec::new(),
            icon_themes: Vec::new(),
            gtk_themes: Vec::new(),
            qt_styles: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Parts {
    pub family: Option<String>,
    pub style: Option<String>,
    pub size: Option<i64>,
}

#[derive(Clone, Copy)]
enum Kind {
    Cursor,
    Icon,
    Gtk,
}

fn home() -> PathBuf {
    glib::home_dir()
}

fn gtk_settings() -> [PathBuf; 2] {
    let config = glib::user_config_dir();
    [
        config.join("gtk-3.0/settings.ini"),
        config.join("gtk-4.0/settings.ini"),
    ]
}

fn kdeglobals() -> PathBuf {
    glib::user_config_dir().join("kdeglobals")
}

fn icon_default() -> PathBuf {
    home().join(".icons/default/index.theme")
}

fn run(command: &[&str]) -> String {
    Command::new(command[0])
        .args(&command[1..])
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).into_owned())
        .unwrap_or_default()
}

fn gsettings(key: &str) -> String {
    run(&["gsettings", "get", SCHEMA, key])
        .trim()
        .trim_matches('\'')
        .to_owned()
}

fn set_gsettings(key: &str, value: &str) {
    run(&["gsettings", "set", SCHEMA, key, value]);
}

fn line_starts(text: &str, from: usize) -> impl Iterator<Item = usize> + '_ {
    std::iter::once(from).chain(
        text[from..]
            .match_indices('\n')
            .map(move |(at, _)| from + at + 1),
    )
}

fn ends_line(rest: &str) -> bool {
    let rest = rest.trim_start_matches([' ', '\t']);
    rest.is_empty() || rest.starts_with('\n')
}

fn section_span(text: &str, section: &str) -> Option<(usize, usize)> {
    let start = line_starts(text, 0).find_map(|at| {
        let rest = text[at..].strip_prefix(section)?;
        ends_line(rest).then(|| text.len() - rest.trim_start_matches([' ', '\t']).len())
    })?;
    let end = line_starts(text, start)
        .skip(1)
        .find(|&at| {
            let Some(rest) = text[at..].strip_prefix('[') else {
                return false;
            };
            rest.find(']')
                .is_some_and(|close| close > 0 && ends_line(&rest[close + 1..]))
        })
        .unwrap_or(text.len());
    Some((start, end))
}

fn key_value(body: &str, key: &str) -> Option<(usize, usize)> {
    line_starts(body, 0).find_map(|at| {
        let line = body[at..].split('\n').next().unwrap_or("");
        let rest = line
            .strip_prefix(key)?
            .trim_start_matches([' ', '\t'])
            .strip_prefix('=')?
            .trim_start_matches([' ', '\t']);
        Some((at + line.len() - rest.len(), at + line.len()))
    })
}

pub fn ini_value(text: &str, key: &str, section: &str) -> String {
    let Some((start, end)) = section_span(text, section) else {
        return String::new();
    };
    let body = &text[start..end];
    key_value(body, key)
        .map(|(from, to)| body[from..to].trim().to_owned())
        .unwrap_or_default()
}

pub fn with_ini_value(text: &str, key: &str, value: &str, section: &str) -> String {
    let Some((start, end)) = section_span(text, section) else {
        return format!(
            "{}\n\n{section}\n{key}={value}\n",
            text.trim_end_matches('\n')
        );
    };
    let body = &text[start..end];
    let body = match key_value(body, key) {
        Some((from, to)) => format!("{}{value}{}", &body[..from], &body[to..]),
        None => {
            let mut body = format!("{}\n{key}={value}\n", body.trim_end_matches('\n'));
            if end < text.len() {
                body.push('\n');
            }
            body
        }
    };
    format!("{}{body}{}", &text[..start], &text[end..])
}

fn read_ini_key(path: &Path, key: &str, section: &str) -> String {
    std::fs::read_to_string(path)
        .map(|text| ini_value(&text, key, section))
        .unwrap_or_default()
}

fn set_ini_key(path: &Path, key: &str, value: &str, section: &str) {
    let text = std::fs::read_to_string(path).unwrap_or_else(|_| format!("{section}\n"));
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, with_ini_value(&text, key, value, section));
}

fn skip_space(text: &str) -> &str {
    text.trim_start_matches(char::is_whitespace)
}

pub fn with_lua_env(text: &str, name: &str, value: &str) -> String {
    let quoted = format!("\"{name}\"");
    for (at, opening) in text.match_indices("hl.env(") {
        let Some(rest) = skip_space(&text[at + opening.len()..]).strip_prefix(quoted.as_str())
        else {
            continue;
        };
        let Some(rest) = skip_space(rest).strip_prefix(',') else {
            continue;
        };
        let Some(rest) = skip_space(rest).strip_prefix('"') else {
            continue;
        };
        let Some(close) = rest.find('"') else {
            continue;
        };
        let from = text.len() - rest.len();
        return format!("{}{value}{}", &text[..from], &text[from + close..]);
    }
    format!(
        "{}\nhl.env(\"{name}\", \"{value}\")\n",
        text.trim_end_matches('\n')
    )
}

pub fn with_start_exec(text: &str, prefix: &str, command: &str) -> String {
    let opening = format!("hl.exec_cmd(\"{prefix}");
    let line = format!("hl.exec_cmd(\"{command}\")");
    for (at, _) in text.match_indices(opening.as_str()) {
        let rest = &text[at + opening.len()..];
        let Some(close) = rest.find('"') else {
            continue;
        };
        if rest[close + 1..].starts_with(')') {
            let end = at + opening.len() + close + 2;
            return format!("{}{line}{}", &text[..at], &text[end..]);
        }
    }
    format!(
        "{}\nhl.on(\"hyprland.start\", function()\n\t{line}\nend)\n",
        text.trim_end_matches('\n')
    )
}

fn edit_hypr_settings(edit: impl FnOnce(&str) -> String) {
    let path = settings_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|_| HYPR_HEADER.to_owned());
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, edit(&text));
}

fn set_lua_env(name: &str, value: &str) {
    edit_hypr_settings(|text| with_lua_env(text, name, value));
}

fn themes(kind: Kind) -> Vec<String> {
    let home = home();
    let roots = match kind {
        Kind::Gtk => [
            home.join(".themes"),
            home.join(".local/share/themes"),
            PathBuf::from("/usr/share/themes"),
        ],
        Kind::Cursor | Kind::Icon => [
            home.join(".icons"),
            home.join(".local/share/icons"),
            PathBuf::from("/usr/share/icons"),
        ],
    };
    let keeps = |path: &Path| match kind {
        Kind::Cursor => path.join("cursors").is_dir(),
        Kind::Icon => {
            !read_ini_key(&path.join("index.theme"), "Directories", "[Icon Theme]").is_empty()
        }
        Kind::Gtk => std::fs::read_dir(path).is_ok_and(|entries| {
            entries
                .flatten()
                .any(|entry| entry.file_name().to_string_lossy().starts_with("gtk-"))
        }),
    };
    let mut found = BTreeSet::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(&root) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if !path.is_dir() || name == "hicolor" {
                continue;
            }
            if keeps(&path) {
                found.insert(name);
            }
        }
    }
    found.into_iter().collect()
}

fn capitalize(word: &str) -> String {
    let mut characters = word.chars();
    characters.next().map_or_else(String::new, |first| {
        first
            .to_uppercase()
            .chain(characters.flat_map(char::to_lowercase))
            .collect()
    })
}

fn style_name(plugin: &str) -> String {
    let name = plugin.strip_suffix(".so").map_or(plugin, |stem| {
        stem.trim_end_matches(|c: char| c.is_ascii_digit())
    });
    capitalize(name)
}

fn qt_styles() -> Vec<String> {
    let mut found: BTreeSet<String> = ["Fusion", "Windows"].map(str::to_owned).into();
    for root in ["/usr/lib/qt6/plugins/styles", "/usr/lib/qt/plugins/styles"] {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            found.insert(style_name(&entry.file_name().to_string_lossy()));
        }
    }
    found.into_iter().collect()
}

fn weight_of(name: &str) -> Option<u32> {
    WEIGHTS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, weight)| *weight)
}

fn style_parts(style: &str) -> (String, bool) {
    let words: Vec<&str> = style.split_whitespace().collect();
    let slanted = |word: &&str| *word == "Italic" || *word == "Oblique";
    let italic = words.iter().any(slanted);
    let base: Vec<&str> = words.into_iter().filter(|word| !slanted(word)).collect();
    (base.join(" "), italic)
}

fn style_order(style: &str) -> (bool, u32, String) {
    let (base, italic) = style_parts(style);
    (italic, weight_of(&base).unwrap_or(400), style.to_owned())
}

pub fn parse_families(text: &str) -> Families {
    let mut catalogue: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for line in text.lines() {
        let (family, style) = line.split_once(":style=").unwrap_or((line, ""));
        let name = family.split(',').next().unwrap_or("").trim();
        if name.is_empty() {
            continue;
        }
        let styles = catalogue.entry(name.to_owned()).or_default();
        for one in style.split(',') {
            if !one.trim().is_empty() {
                styles.insert(one.trim().to_owned());
            }
        }
    }
    catalogue
        .into_iter()
        .map(|(name, styles)| {
            let mut styles: Vec<String> = styles.into_iter().collect();
            styles.sort_by_key(|style| style_order(style));
            (name, styles)
        })
        .collect()
}

pub fn families() -> Families {
    parse_families(&run(&["fc-list", ":", "family", "style"]))
}

pub fn spec(family: &str, style: &str, size: i64) -> String {
    let (base, italic) = style_parts(style);
    let weight = weight_of(&base.replace(' ', ""))
        .or_else(|| weight_of(&base))
        .unwrap_or(400);
    let style = if style.is_empty() { "Regular" } else { style };
    format!(
        "{family},{size},-1,5,{weight},{},0,0,0,0,0,0,0,0,0,1,{style},0,0",
        u8::from(italic)
    )
}

fn weight_name(weight: u32) -> &'static str {
    WEIGHT_NAMES
        .iter()
        .find(|(_, value)| *value == weight)
        .map_or("Regular", |(name, _)| name)
}

fn split_family(family: &str, installed: &Families) -> (String, String) {
    if installed.contains_key(family) {
        return (family.to_owned(), String::new());
    }
    let words: Vec<&str> = family.split_whitespace().collect();
    for cut in 1..=words.len().min(3) {
        let head = words[..words.len() - cut].join(" ");
        let tail = words[words.len() - cut..].join(" ");
        if installed
            .get(&head)
            .is_some_and(|styles| styles.contains(&tail))
        {
            return (head, tail);
        }
    }
    (family.to_owned(), String::new())
}

fn is_digits(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c.is_ascii_digit())
}

pub fn font_from(spec: &str, desktop_font: impl FnOnce() -> String, installed: &Families) -> Font {
    let parts: Vec<&str> = spec.split(',').map(str::trim).collect();
    if !parts[0].is_empty() {
        let size = parts
            .get(1)
            .filter(|size| is_digits(size))
            .and_then(|size| size.parse().ok())
            .unwrap_or(10);
        let style = if parts.len() > 16 && !parts[16].is_empty() && !is_digits(parts[16]) {
            parts[16].to_owned()
        } else if parts.len() > 5 {
            let weight = if is_digits(parts[4]) {
                parts[4].parse().unwrap_or(400)
            } else {
                400
            };
            let name = weight_name(weight);
            if parts[5] == "1" {
                format!("{name} Italic")
            } else {
                name.to_owned()
            }
        } else {
            String::new()
        };
        let (family, baked) = split_family(parts[0], installed);
        return Font {
            family,
            style: if baked.is_empty() { style } else { baked },
            size,
        };
    }
    let font = desktop_font();
    let (family, size) = font.rsplit_once(' ').unwrap_or(("", &font));
    let (family, baked) = split_family(if family.is_empty() { &font } else { family }, installed);
    Font {
        family,
        style: baked,
        size: if is_digits(size) {
            size.parse().unwrap_or(11)
        } else {
            11
        },
    }
}

fn role(name: &str) -> Option<(&'static str, &'static str, &'static str)> {
    ROLES.iter().find(|(role, _, _)| *role == name).copied()
}

fn read_font(name: &str, installed: &Families) -> Font {
    let Some((_, section, key)) = role(name) else {
        return Font::default();
    };
    font_from(
        &read_ini_key(&kdeglobals(), key, section),
        || gsettings("font-name"),
        installed,
    )
}

pub fn load() -> (State, Families) {
    let installed = families();
    let kdeglobals = kdeglobals();
    let icon_theme = read_ini_key(&kdeglobals, "Theme", "[Icons]");
    let state = State {
        cursor_theme: gsettings("cursor-theme"),
        cursor_size: gsettings("cursor-size").parse().unwrap_or(24),
        fonts: ROLES
            .iter()
            .map(|(role, _, _)| (*role, read_font(role, &installed)))
            .collect(),
        icon_theme: if icon_theme.is_empty() {
            gsettings("icon-theme")
        } else {
            icon_theme
        },
        gtk_theme: gsettings("gtk-theme"),
        qt_style: read_ini_key(&kdeglobals, "widgetStyle", "[KDE]"),
        cursor_themes: themes(Kind::Cursor),
        icon_themes: themes(Kind::Icon),
        gtk_themes: themes(Kind::Gtk),
        qt_styles: qt_styles(),
    };
    (state, installed)
}

pub fn set_cursor(theme: &str, size: i64) {
    let _writing = WRITING.lock();
    let size = size.to_string();
    set_gsettings("cursor-theme", theme);
    set_gsettings("cursor-size", &size);
    for path in gtk_settings() {
        set_ini_key(&path, "gtk-cursor-theme-name", theme, SETTINGS);
        set_ini_key(&path, "gtk-cursor-theme-size", &size, SETTINGS);
    }
    set_lua_env("XCURSOR_THEME", theme);
    set_lua_env("XCURSOR_SIZE", &size);
    set_lua_env("HYPRCURSOR_SIZE", &size);
    edit_hypr_settings(|text| {
        with_start_exec(
            text,
            "hyprctl setcursor",
            &format!("hyprctl setcursor {theme} {size}"),
        )
    });
    set_ini_key(&icon_default(), "Inherits", theme, "[Icon Theme]");
    run(&["hyprctl", "setcursor", theme, &size]);
}

pub fn set_icons(theme: &str) {
    let _writing = WRITING.lock();
    set_gsettings("icon-theme", theme);
    for path in gtk_settings() {
        set_ini_key(&path, "gtk-icon-theme-name", theme, SETTINGS);
    }
    set_ini_key(&kdeglobals(), "Theme", theme, "[Icons]");
    set_lua_env("QT_ICON_THEME", theme);
    set_lua_env("QS_ICON_THEME", theme);
}

fn variations(pango: &str) -> String {
    match pango.rfind('@') {
        Some(marker) if marker > 0 => format!(" {}", &pango[marker..]),
        _ => String::new(),
    }
}

fn set_font(name: &str, parts: &Parts, installed: &Families) {
    let Some((_, section, key)) = role(name) else {
        return;
    };
    let current = read_font(name, installed);
    let same_family = parts
        .family
        .as_ref()
        .is_none_or(|family| *family == current.family);
    let family = parts.family.clone().unwrap_or(current.family);
    let style = parts.style.clone().unwrap_or(current.style);
    let size = parts.size.unwrap_or(current.size);
    set_ini_key(&kdeglobals(), key, &spec(&family, &style, size), section);

    let shown_style = if style == "Regular" {
        ""
    } else {
        style.as_str()
    };
    let size = size.to_string();
    let pango = [family.as_str(), shown_style, &size]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    let with_axes = |key: &str| {
        let axes = if same_family {
            variations(&gsettings(key))
        } else {
            String::new()
        };
        format!("{pango}{axes}")
    };
    match name {
        "general" => {
            let font = with_axes("font-name");
            set_gsettings("font-name", &font);
            for path in gtk_settings() {
                set_ini_key(&path, "gtk-font-name", &font, SETTINGS);
            }
        }
        "fixed" => set_gsettings("monospace-font-name", &with_axes("monospace-font-name")),
        _ => {}
    }
}

pub fn set_fonts(name: &str, parts: &Parts) {
    let _writing = WRITING.lock();
    let installed = families();
    if name == "all" {
        for (role, _, _) in ROLES.iter().filter(|(role, _, _)| *role != "fixed") {
            set_font(role, parts, &installed);
        }
    } else {
        set_font(name, parts, &installed);
    }
}

pub fn set_themes(gtk: &str, qt: &str) {
    let _writing = WRITING.lock();
    if !gtk.is_empty() {
        set_gsettings("gtk-theme", gtk);
        for path in gtk_settings() {
            set_ini_key(&path, "gtk-theme-name", gtk, SETTINGS);
        }
    }
    if !qt.is_empty() {
        set_ini_key(&kdeglobals(), "widgetStyle", qt, "[KDE]");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FONTS: &str = "DejaVu Sans,DejaVu Sans Condensed:style=Condensed Bold,Bold\nDejaVu Sans:style=Book\nInter:style=Italic\nInter:style=Bold Italic,Italic\nInter:style=Regular\nInter:style=Medium\nSF Pro Text:style=Medium\nSF Pro Text:style=Regular\n:style=Nothing\nNoStyle\n";

    #[test]
    fn ini_keys_are_written_inside_their_own_section() {
        let cases = [
            (
                "[Settings]\ngtk-theme-name=Adw\n",
                "gtk-theme-name",
                "X",
                "[Settings]",
                "[Settings]\ngtk-theme-name=X\n",
            ),
            (
                "[Settings]\ngtk-theme-name=Adw\n",
                "gtk-font-name",
                "Sans 11",
                "[Settings]",
                "[Settings]\ngtk-theme-name=Adw\ngtk-font-name=Sans 11\n",
            ),
            (
                "[General]\nfont=A\n\n[KDE]\nwidgetStyle=Breeze\n",
                "fixed",
                "Mono",
                "[General]",
                "[General]\nfont=A\nfixed=Mono\n\n[KDE]\nwidgetStyle=Breeze\n",
            ),
            (
                "[General]\nfont=A\n\n[KDE]\nwidgetStyle=Breeze\n",
                "widgetStyle",
                "Fusion",
                "[KDE]",
                "[General]\nfont=A\n\n[KDE]\nwidgetStyle=Fusion\n",
            ),
            (
                "[General]\nfont = A\n",
                "font",
                "B",
                "[General]",
                "[General]\nfont = B\n",
            ),
            (
                "[General]\nfont=A\n",
                "Theme",
                "Papirus",
                "[Icons]",
                "[General]\nfont=A\n\n[Icons]\nTheme=Papirus\n",
            ),
            (
                "",
                "Theme",
                "Papirus",
                "[Icons]",
                "\n\n[Icons]\nTheme=Papirus\n",
            ),
            (
                "[General]  \n[WM]\nactiveFont=x\n",
                "font",
                "F",
                "[General]",
                "[General]  \nfont=F\n\n[WM]\nactiveFont=x\n",
            ),
            (
                "[General]\n[WM]\nactiveFont=x",
                "activeFont",
                "y",
                "[WM]",
                "[General]\n[WM]\nactiveFont=y",
            ),
            (
                "[General]\n[WM]\nactiveFont=x",
                "fontX",
                "y",
                "[WM]",
                "[General]\n[WM]\nactiveFont=x\nfontX=y\n",
            ),
        ];
        for (text, key, value, section, written) in cases {
            assert_eq!(with_ini_value(text, key, value, section), written);
            assert_eq!(ini_value(written, key, section), value);
        }
        assert_eq!(
            ini_value("[General]\nfont=A\n[WM]\nfont=B\n", "font", "[WM]"),
            "B"
        );
    }

    #[test]
    fn lua_lines_are_replaced_in_place_or_appended() {
        assert_eq!(
            with_lua_env("hl.env(\"XCURSOR_SIZE\", \"24\")\n", "XCURSOR_SIZE", "32"),
            "hl.env(\"XCURSOR_SIZE\", \"32\")\n"
        );
        assert_eq!(
            with_lua_env(
                "hl.env( \"XCURSOR_SIZE\" ,\n \"24\" )\nx\n",
                "XCURSOR_SIZE",
                "32"
            ),
            "hl.env( \"XCURSOR_SIZE\" ,\n \"32\" )\nx\n"
        );
        assert_eq!(
            with_lua_env(HYPR_HEADER, "QT_ICON_THEME", "Papirus"),
            format!("{HYPR_HEADER}hl.env(\"QT_ICON_THEME\", \"Papirus\")\n")
        );
        assert_eq!(
            with_lua_env(
                "hl.env(\"XCURSOR_SIZE_X\", \"24\")\n\n",
                "XCURSOR_SIZE",
                "32"
            ),
            "hl.env(\"XCURSOR_SIZE_X\", \"24\")\nhl.env(\"XCURSOR_SIZE\", \"32\")\n"
        );
        let started = "hl.on(\"hyprland.start\", function()\n\thl.exec_cmd(\"hyprctl setcursor B 32\")\nend)\n";
        assert_eq!(
            with_start_exec(
                "hl.on(\"hyprland.start\", function()\n\thl.exec_cmd(\"hyprctl setcursor A 24\")\nend)\n",
                "hyprctl setcursor",
                "hyprctl setcursor B 32"
            ),
            started
        );
        assert_eq!(
            with_start_exec("-- x\n", "hyprctl setcursor", "hyprctl setcursor B 32"),
            format!("-- x\n{started}")
        );
    }

    #[test]
    fn families_list_their_styles_from_thin_to_black_upright_first() {
        let families = parse_families(FONTS);
        let listed: Vec<(&str, Vec<&str>)> = families
            .iter()
            .map(|(name, styles)| (name.as_str(), styles.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            listed,
            [
                ("DejaVu Sans", vec!["Book", "Condensed Bold", "Bold"]),
                ("Inter", vec!["Regular", "Medium", "Italic", "Bold Italic"]),
                ("NoStyle", vec![]),
                ("SF Pro Text", vec!["Regular", "Medium"]),
            ]
        );
    }

    #[test]
    fn font_specs_round_trip_through_kdeglobals() {
        assert_eq!(
            [
                spec("Inter", "Bold Italic", 11),
                spec("Inter", "", 12),
                spec("Inter", "Semi Bold", 10),
                spec("X", "Weird", 9),
            ],
            [
                "Inter,11,-1,5,700,1,0,0,0,0,0,0,0,0,0,1,Bold Italic,0,0",
                "Inter,12,-1,5,400,0,0,0,0,0,0,0,0,0,0,1,Regular,0,0",
                "Inter,10,-1,5,600,0,0,0,0,0,0,0,0,0,0,1,Semi Bold,0,0",
                "X,9,-1,5,400,0,0,0,0,0,0,0,0,0,0,1,Weird,0,0",
            ]
        );
        let installed = parse_families(FONTS);
        let font = |family: &str, style: &str, size| Font {
            family: family.to_owned(),
            style: style.to_owned(),
            size,
        };
        let desktop = || "Inter Medium 13 @wght=500".to_owned();
        let cases = [
            (
                "Inter,11,-1,5,700,1,0,0,0,0,0,0,0,0,0,1,Bold Italic,0,0",
                font("Inter", "Bold Italic", 11),
            ),
            (
                "Inter,11,-1,5,500,0,0,0,0,0,0,0,0,0,0,1",
                font("Inter", "Medium", 11),
            ),
            (
                "SF Pro Text Medium,10,-1,5,50,0",
                font("SF Pro Text", "Medium", 10),
            ),
            ("Inter,abc", font("Inter", "", 10)),
            ("Inter", font("Inter", "", 10)),
            ("", font("Inter Medium 13", "", 11)),
        ];
        for (spec, read) in cases {
            assert_eq!(font_from(spec, desktop, &installed), read);
        }
        assert_eq!(
            font_from("", || "Plain".to_owned(), &installed),
            font("Plain", "", 11)
        );
    }

    #[test]
    fn qt_style_plugins_are_named_the_way_qt_lists_them() {
        assert_eq!(style_name("breeze6.so"), "Breeze");
        assert_eq!(style_name("libkvantum.so"), "Libkvantum");
        assert_eq!(style_name("README"), "Readme");
    }
}
