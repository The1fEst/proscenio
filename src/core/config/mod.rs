mod parser;

use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use toml_edit::{DocumentMut, Item, TableLike};

use crate::core::{paths, watch};

pub const BASE_BAR_HEIGHT: i32 = 40;
pub const BASE_VERTICAL_BAR_WIDTH: i32 = 46;
pub const HYPRLAND_GAPS_OUT: i32 = 5;
pub const NOTIFICATION_TIMEOUT_MILLIS: i64 = 7000;
pub const NOTIFICATION_TIMEOUT_RANGE_MILLIS: (i64, i64) = (1000, 60000);
pub const OSD_TIMEOUT_MILLIS: i64 = 1000;
pub const TIME_FORMAT: &str = "hh:mm";
pub const DATE_FORMAT: &str = "ddd, dd/MM";
pub const DOCK_PINNED_APPS: [&str; 3] = ["org.kde.dolphin", "brave-origin", "kitty"];
pub const CHEATSHEET_SUPER_KEY: &str = "\u{f05b3}";
pub const NIGHT_FROM: &str = "19:00";
pub const NIGHT_TO: &str = "06:30";
pub const NIGHT_TRANSITION_MINUTES: i32 = 30;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CornerStyle {
    Hug,
    Float,
    Plain,
}

pub struct RegionOptions {
    pub show_pointer: Cell<bool>,
    pub countdown: Cell<i64>,
    pub remember_region: Cell<bool>,
    pub record_sound: Cell<bool>,
    pub target_windows: bool,
    pub target_layers: bool,
    pub show_label: bool,
    pub target_opacity: f64,
    pub selection_padding: f64,
    pub aim_lines: bool,
    pub circle_stroke: f64,
    pub circle_padding: f64,
    pub save: Cell<bool>,
    pub save_path: String,
}

impl RegionOptions {
    pub fn set_show_pointer(&self, is_on: bool) {
        self.show_pointer.set(is_on);
        store("regionSelector", "showPointer", Value::Bool(is_on));
    }

    pub fn set_countdown(&self, seconds: i64) {
        self.countdown.set(seconds);
        store("regionSelector", "countdownSeconds", Value::from(seconds));
    }

    pub fn set_remember_region(&self, is_on: bool) {
        self.remember_region.set(is_on);
        store("regionSelector", "rememberRegion", Value::Bool(is_on));
    }

    pub fn set_record_sound(&self, is_on: bool) {
        self.record_sound.set(is_on);
        store("regionSelector", "recordSound", Value::Bool(is_on));
    }

    pub fn set_save(&self, is_on: bool) {
        self.save.set(is_on);
        store("screenSnip", "save", Value::Bool(is_on));
    }
}

pub struct Config {
    pub corner_style: CornerStyle,
    pub bottom: bool,
    pub vertical: bool,
    pub borderless: bool,
    pub show_background: bool,
    pub float_shadow: bool,
    pub dock_enable: bool,
    pub dock_monochrome: bool,
    pub dock_icon_size: i32,
    pub dock_hover_region: i32,
    pub dock_pinned_on_startup: bool,
    pub dock_hover_to_reveal: bool,
    pub dock_pinned: RefCell<Vec<String>>,
    pub dock_ignored: Vec<String>,
    pub cheatsheet_super: String,
    pub cheatsheet_mac_symbols: bool,
    pub cheatsheet_fn_symbols: bool,
    pub cheatsheet_mouse_symbols: bool,
    pub cheatsheet_split: bool,
    pub cheatsheet_key_size: i32,
    pub cheatsheet_comment_size: i32,
    pub region: RegionOptions,
    pub overview_enable: bool,
    pub overview_scale: f64,
    pub overview_rows: i32,
    pub overview_columns: i32,
    pub overview_right_left: bool,
    pub overview_bottom_up: bool,
    pub overview_center_icons: bool,
    pub search_action: String,
    pub search_app: String,
    pub search_clipboard: String,
    pub search_emojis: String,
    pub search_math: String,
    pub search_shell: String,
    pub search_default_actions: bool,
    pub search_non_app_delay: u64,
    pub race_condition_delay: u64,
    pub safety_clipboard: bool,
    pub safety_networks: Vec<String>,
    pub safety_links: Vec<String>,
    pub safety_wallpaper: bool,
    pub safety_files: Vec<String>,
    pub app_terminal: String,
    pub fake_rounding: i32,
    pub dead_pixel: bool,
    pub corner_open: bool,
    pub corner_bottom: bool,
    pub corner_value_scroll: bool,
    pub corner_clickless: bool,
    pub corner_width: i32,
    pub corner_height: i32,
    pub corner_visualize: bool,
    pub corner_clickless_end: bool,
    pub corner_vertical_offset: i32,
    pub verbose: bool,
    pub workspaces_shown: i32,
    pub workspaces_show_app_icons: bool,
    pub workspaces_always_show_numbers: bool,
    pub workspaces_monochrome_icons: bool,
    pub workspaces_use_nerd_font: bool,
    pub workspaces_number_map: Vec<String>,
    pub hover_region_width: i32,
    pub auto_hide: bool,
    pub auto_hide_push_windows: bool,
    pub super_show: bool,
    pub super_show_delay: u32,
    pub weather_enable: bool,
    pub weather_city: String,
    pub weather_gps: bool,
    pub weather_uscs: bool,
    pub weather_interval: i32,
    pub update_interval: u32,
    pub time_second_precision: bool,
    pub date_with_year_format: String,
    pub util_screen_snip: bool,
    pub util_screen_record: bool,
    pub util_color_picker: bool,
    pub util_keyboard: bool,
    pub util_mic: bool,
    pub util_dark_mode: bool,
    pub util_power_profile: bool,
    pub updates_enable_check: bool,
    pub updates_interval: i32,
    pub updates_advise: i32,
    pub updates_strongly_advise: i32,
    pub battery_low: f64,
    pub battery_critical: f64,
    pub battery_full: f64,
    pub battery_automatic_suspend: bool,
    pub battery_suspend: f64,
    pub notifications_silent: bool,
    pub notifications_show_unread_count: bool,
    pub tray_monochrome_icons: bool,
    pub tray_show_item_id: bool,
    pub font_nerd: String,
    pub font_monospace: String,
    pub screens: Vec<String>,
    pub gaps_out: i32,
    pub rounding: i32,
    pub font_main: String,
    pub font_title: String,
    pub font_reading: String,
    pub font_expressive: String,
    pub extra_background_tint: bool,
    pub transparency_enable: bool,
    pub transparency_automatic: bool,
    pub background_transparency: f32,
    pub content_transparency: f32,
    pub time_format: String,
    pub date_format: String,
    pub short_date_format: String,
    pub memory_warning: f64,
    pub swap_warning: f64,
    pub cpu_warning: f64,
    pub tray_pinned: RefCell<Vec<String>>,
    pub tray_invert_pinned: bool,
    pub tray_filter_passive: bool,
    pub wallpaper: String,
    pub thumbnail: String,
    pub hide_when_fullscreen: bool,
    pub parallax_workspace: bool,
    pub parallax_vertical: bool,
    pub parallax_auto_vertical: bool,
    pub parallax_sidebar: bool,
    pub workspace_zoom: f64,
    pub quick_sliders: bool,
    pub slider_volume: bool,
    pub slider_mic: bool,
    pub slider_brightness: bool,
    pub toggles: RefCell<Vec<(String, i32)>>,
    pub toggle_columns: i32,
    pub toggle_style: String,
    pub night_temperature: i32,
    pub night_automatic: bool,
    pub night_from: String,
    pub night_to: String,
    pub night_transition: i32,
    pub pomodoro_focus: i64,
    pub pomodoro_break: i64,
    pub pomodoro_long_break: i64,
    pub pomodoro_cycles: i64,
    pub sounds_pomodoro: bool,
    pub sounds_battery: bool,
    pub sounds_devices: bool,
    pub sounds_theme: String,
    pub protection: crate::services::media::audio::Protection,
}

impl Config {
    pub fn load() -> Self {
        Self::parse(&root().unwrap_or(Value::Null))
    }

    pub fn store_night(key: &str, value: Value) {
        store_at(&["light", "night"], key, value);
    }

    pub fn wants_screen(&self, name: &str) -> bool {
        self.screens.is_empty() || self.screens.iter().any(|screen| screen == name)
    }

    pub fn is_floating(&self) -> bool {
        self.corner_style == CornerStyle::Float
    }

    pub fn bar_height(&self) -> i32 {
        if self.is_floating() {
            BASE_BAR_HEIGHT + HYPRLAND_GAPS_OUT * 2
        } else {
            BASE_BAR_HEIGHT
        }
    }

    pub fn hug_rounding(&self) -> i32 {
        self.rounding
    }

    pub fn hugs_corners(&self) -> bool {
        self.corner_style == CornerStyle::Hug && self.show_background
    }

    pub fn vertical_bar_width(&self) -> i32 {
        if self.is_floating() {
            BASE_VERTICAL_BAR_WIDTH + HYPRLAND_GAPS_OUT * 2
        } else {
            BASE_VERTICAL_BAR_WIDTH
        }
    }

    pub fn bar_thickness(&self) -> i32 {
        if self.vertical {
            self.vertical_bar_width()
        } else {
            self.bar_height()
        }
    }

    pub fn surface_thickness(&self) -> i32 {
        self.bar_thickness() + self.hug_rounding()
    }

    pub fn exclusive_zone(&self) -> i32 {
        let base = if self.vertical {
            BASE_VERTICAL_BAR_WIDTH
        } else {
            BASE_BAR_HEIGHT
        };
        if self.is_floating() {
            base + HYPRLAND_GAPS_OUT
        } else {
            base
        }
    }

    pub fn shortening_level(&self, screen_width: i32) -> i32 {
        match screen_width {
            width if width <= 1000 => 2,
            width if width <= 1200 => 1,
            _ => 0,
        }
    }

    pub fn center_side_width(&self, screen_width: i32) -> i32 {
        match self.shortening_level(screen_width) {
            2 => 190,
            1 => 280,
            _ if self.verbose => 360,
            _ => 140,
        }
    }

    pub fn tray_shows(&self, id: &str) -> bool {
        self.tray_pinned.borrow().iter().any(|pinned| pinned == id) != self.tray_invert_pinned
    }

    pub fn set_quick_toggles(&self, toggles: Vec<(String, i32)>) {
        let list = Value::Array(
            toggles
                .iter()
                .map(|(kind, size)| serde_json::json!({ "size": size, "type": kind }))
                .collect(),
        );
        self.toggles.replace(toggles);
        store_at(&["sidebar", "quickToggles", "android"], "toggles", list);
    }

    pub fn toggle_pin(&self, id: &str) {
        {
            let mut pinned = self.tray_pinned.borrow_mut();
            match pinned.iter().position(|entry| entry == id) {
                Some(index) => {
                    pinned.remove(index);
                }
                None => pinned.push(id.to_owned()),
            }
        }
        let pins = Value::Array(
            self.tray_pinned
                .borrow()
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        );
        store("tray", "pinnedItems", pins);
    }

    pub fn toggle_dock_pin(&self, id: &str) {
        {
            let mut pinned = self.dock_pinned.borrow_mut();
            match pinned
                .iter()
                .position(|entry| entry.eq_ignore_ascii_case(id))
            {
                Some(index) => {
                    pinned.remove(index);
                }
                None => pinned.push(id.to_owned()),
            }
        }
        self.store_dock_pins();
    }

    pub fn set_dock_pins(&self, pins: Vec<String>) {
        self.dock_pinned.replace(pins);
        self.store_dock_pins();
    }

    fn store_dock_pins(&self) {
        let pins = Value::Array(
            self.dock_pinned
                .borrow()
                .iter()
                .map(|id| Value::String(id.clone()))
                .collect(),
        );
        store("dock", "pinnedApps", pins);
    }
}

pub fn store_flag(section: &str, key: &str, value: bool) {
    store(section, key, Value::Bool(value));
}

fn store(section: &str, key: &str, value: Value) {
    store_at(&[section], key, value);
}

fn store_at(sections: &[&str], key: &str, value: Value) {
    store_value(&format!("/{}/{key}", sections.join("/")), value);
}

pub fn value(pointer: &str) -> Option<Value> {
    root()?.pointer(pointer).cloned()
}

pub fn value_bool(pointer: &str, default: bool) -> bool {
    value(pointer)
        .and_then(|value| value.as_bool())
        .unwrap_or(default)
}

pub fn value_i64(pointer: &str, default: i64) -> i64 {
    value(pointer)
        .and_then(|value| value.as_i64())
        .unwrap_or(default)
}

pub fn value_f64(pointer: &str, default: f64) -> f64 {
    value(pointer)
        .and_then(|value| value.as_f64())
        .unwrap_or(default)
}

pub fn value_str(pointer: &str) -> Option<String> {
    value(pointer).and_then(|value| value.as_str().map(str::to_owned))
}

pub const RENDERER: &str = "/renderer";
pub const DEFAULT_RENDERER: &str = "cairo";
static ACTIVE_RENDERER: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn choose_renderer() -> bool {
    if let Some(renderer) = std::env::var_os("GSK_RENDERER") {
        let _ = ACTIVE_RENDERER.set(renderer.to_string_lossy().into_owned());
        return false;
    }
    let renderer = value_str(RENDERER).unwrap_or_else(|| DEFAULT_RENDERER.to_owned());
    unsafe { std::env::set_var("GSK_RENDERER", &renderer) };
    let _ = ACTIVE_RENDERER.set(renderer);
    true
}

pub fn active_renderer() -> &'static str {
    ACTIVE_RENDERER
        .get()
        .map_or(DEFAULT_RENDERER, String::as_str)
}

pub const RENDERER_FALLBACK: &str = "/rendererFallback";

pub fn renderer_fallback() -> Option<String> {
    value_str(RENDERER_FALLBACK).filter(|fallback| !fallback.is_empty())
}

pub fn pick_renderer(renderer: &str) {
    if renderer == active_renderer() {
        store_value(RENDERER_FALLBACK, Value::from(""));
    } else if renderer_fallback().is_none() {
        store_value(RENDERER_FALLBACK, Value::from(active_renderer()));
    }
    store_value(RENDERER, Value::from(renderer));
}

pub fn settle_renderer(should_keep: bool) {
    if !should_keep && let Some(fallback) = renderer_fallback() {
        store_value(RENDERER, Value::from(fallback));
    }
    store_value(RENDERER_FALLBACK, Value::from(""));
}

pub fn reset_renderer() {
    store_value(RENDERER, Value::from(DEFAULT_RENDERER));
    store_value(RENDERER_FALLBACK, Value::from(""));
}

pub fn store_value(pointer: &str, value: Value) {
    let text = match std::fs::read_to_string(config_path()) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(_) => return,
    };
    if let Some(text) = with_value(&text, pointer, &value) {
        write(&text);
        CURRENT.with_borrow_mut(|slot| {
            if let Some(current) = slot.as_mut() {
                current.generation = u64::MAX;
            }
        });
    }
}

fn with_value(text: &str, pointer: &str, value: &Value) -> Option<String> {
    let mut document = text.parse::<DocumentMut>().ok()?;
    let (parents, key) = pointer.rsplit_once('/')?;
    let mut table: &mut dyn TableLike = document.as_table_mut();
    for section in parents.split('/').skip(1) {
        table = table
            .entry(section)
            .or_insert_with(toml_edit::table)
            .as_table_like_mut()?;
    }
    match toml_value(value) {
        None => {
            table.remove(key);
        }
        Some(mut replacement) => match table.get_mut(key).and_then(Item::as_value_mut) {
            Some(current) => {
                *replacement.decor_mut() = current.decor().clone();
                *current = replacement;
            }
            None => {
                table.insert(key, Item::Value(replacement));
            }
        },
    }
    Some(document.to_string())
}

fn toml_value(value: &Value) -> Option<toml_edit::Value> {
    match value {
        Value::Null => None,
        Value::Bool(flag) => Some((*flag).into()),
        Value::Number(number) => number
            .as_i64()
            .map(Into::into)
            .or_else(|| number.as_f64().map(Into::into)),
        Value::String(text) => Some(text.as_str().into()),
        Value::Array(items) => Some(toml_edit::Value::Array(
            items.iter().filter_map(toml_value).collect(),
        )),
        Value::Object(entries) => Some(toml_edit::Value::InlineTable(
            entries
                .iter()
                .filter_map(|(key, value)| Some((key.as_str(), toml_value(value)?)))
                .collect(),
        )),
    }
}

pub fn write(text: &str) {
    let path = config_path();
    let _ = std::fs::create_dir_all(paths::config());
    let staging_path = path.with_extension("toml.tmp");
    if std::fs::write(&staging_path, text).is_ok() {
        let _ = std::fs::rename(staging_path, path);
    }
}

pub fn config_path() -> PathBuf {
    paths::config().join("config.toml")
}

pub fn root() -> Option<Value> {
    let text = std::fs::read_to_string(config_path()).ok()?;
    toml_edit::de::from_str(&text).ok()
}

struct Current {
    config: Rc<Config>,
    generation: u64,
    config_watch: watch::Watch,
}

thread_local! {
    static CURRENT: RefCell<Option<Current>> = const { RefCell::new(None) };
}

/// The config as the file says now, for code that must follow edits without a restart.
pub fn current() -> Rc<Config> {
    let generation = watch::generation();
    let current_config = CURRENT.with_borrow(|slot| {
        slot.as_ref()
            .filter(|current| current.generation == generation)
            .map(|current| current.config.clone())
    });
    if let Some(config) = current_config {
        return config;
    }
    let config_watch = CURRENT
        .with_borrow_mut(|slot| slot.take().map(|current| current.config_watch))
        .unwrap_or_else(|| watch::config("", || {}));
    let config = Rc::new(Config::load());
    CURRENT.with_borrow_mut(|slot| {
        *slot = Some(Current {
            config: config.clone(),
            generation: watch::generation(),
            config_watch,
        })
    });
    config
}

pub fn is_twelve_hour(time_format: &str) -> bool {
    time_format.contains("AP") || time_format.contains("ap") || time_format.contains('A')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_time_format_with_an_am_pm_marker_is_twelve_hour() {
        for time_format in ["h:mm ap", "h:mm AP", "hh:mm A"] {
            assert!(is_twelve_hour(time_format), "{time_format}");
        }
        for time_format in [TIME_FORMAT, "HH:mm:ss"] {
            assert!(!is_twelve_hour(time_format), "{time_format}");
        }
    }

    #[test]
    fn a_write_keeps_the_rest_of_the_file_as_the_user_left_it() {
        let text = "# mine\n[bar]\nfloating = false # keep\n\n[light.night]\nautomatic = true\n";
        let cases = [
            (
                "/bar/floating",
                json!(true),
                "# mine\n[bar]\nfloating = true # keep\n\n[light.night]\nautomatic = true\n",
            ),
            (
                "/light/night/colorTemperature",
                json!(4000),
                "# mine\n[bar]\nfloating = false # keep\n\n[light.night]\nautomatic = true\ncolorTemperature = 4000\n",
            ),
            (
                "/bar/floating",
                Value::Null,
                "# mine\n[bar]\n\n[light.night]\nautomatic = true\n",
            ),
            (
                "/appearance/palette/accentColor",
                json!("#ff6f00"),
                "# mine\n[bar]\nfloating = false # keep\n\n[light.night]\nautomatic = true\n\n[appearance]\n\n[appearance.palette]\naccentColor = \"#ff6f00\"\n",
            ),
        ];
        for (pointer, value, expected) in cases {
            assert_eq!(
                with_value(text, pointer, &value).unwrap(),
                expected,
                "{pointer}"
            );
        }
    }
}
