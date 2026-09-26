use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use toml_edit::{DocumentMut, Item, TableLike};

use crate::core::{paths, watch};
use crate::platform::hypr;

pub const BASE_BAR_HEIGHT: i32 = 40;
pub const BASE_VERTICAL_BAR_WIDTH: i32 = 46;
pub const HYPRLAND_GAPS_OUT: i32 = 5;
pub const NOTIFICATION_TIMEOUT: i64 = 7000;
pub const OSD_TIMEOUT: i64 = 1000;

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
    pub fn set_show_pointer(&self, on: bool) {
        self.show_pointer.set(on);
        store("regionSelector", "showPointer", Value::Bool(on));
    }

    pub fn set_countdown(&self, seconds: i64) {
        self.countdown.set(seconds);
        store("regionSelector", "countdownSeconds", Value::from(seconds));
    }

    pub fn set_remember_region(&self, on: bool) {
        self.remember_region.set(on);
        store("regionSelector", "rememberRegion", Value::Bool(on));
    }

    pub fn set_record_sound(&self, on: bool) {
        self.record_sound.set(on);
        store("regionSelector", "recordSound", Value::Bool(on));
    }

    pub fn set_save(&self, on: bool) {
        self.save.set(on);
        store("screenSnip", "save", Value::Bool(on));
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
    pub dock_height: i32,
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
    pub util_updates: bool,
    pub util_screen_snip: bool,
    pub util_screen_record: bool,
    pub util_color_picker: bool,
    pub util_mic: bool,
    pub util_dark_mode: bool,
    pub util_power_profile: bool,
    pub updates_enable_check: bool,
    pub updates_interval: i32,
    pub updates_advise: i32,
    pub updates_strongly_advise: i32,
    pub app_update: String,
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
    pub pomodoro_focus: i64,
    pub pomodoro_break: i64,
    pub pomodoro_long_break: i64,
    pub pomodoro_cycles: i64,
    pub sounds_pomodoro: bool,
    pub sounds_battery: bool,
    pub sounds_theme: String,
    pub protection: crate::services::audio::Protection,
}

impl Config {
    pub fn load() -> Self {
        let root = root().unwrap_or(Value::Null);
        let bar = root.get("bar").cloned().unwrap_or(Value::Null);
        let time = root.get("time").cloned().unwrap_or(Value::Null);
        let resources = bar.get("resources").cloned().unwrap_or(Value::Null);
        let tray = root.get("tray").cloned().unwrap_or(Value::Null);
        let background = root.get("background").cloned().unwrap_or(Value::Null);
        let parallax = background.get("parallax").cloned().unwrap_or(Value::Null);
        let quick_sliders = root
            .pointer("/sidebar/quickSliders")
            .cloned()
            .unwrap_or(Value::Null);
        let workspaces = bar.get("workspaces").cloned().unwrap_or(Value::Null);
        let auto_hide = bar.get("autoHide").cloned().unwrap_or(Value::Null);
        let util = bar.get("utilButtons").cloned().unwrap_or(Value::Null);
        let updates = root.get("updates").cloned().unwrap_or(Value::Null);
        let apps = root.get("apps").cloned().unwrap_or(Value::Null);
        let battery = root.get("battery").cloned().unwrap_or(Value::Null);
        let pomodoro = time.get("pomodoro").cloned().unwrap_or(Value::Null);
        let sounds = root.get("sounds").cloned().unwrap_or(Value::Null);
        let protection = root
            .pointer("/audio/protection")
            .cloned()
            .unwrap_or(Value::Null);
        let notifications = root.get("notifications").cloned().unwrap_or(Value::Null);
        let appearance = root.get("appearance").cloned().unwrap_or(Value::Null);
        let fonts = appearance.get("fonts").cloned().unwrap_or(Value::Null);
        let transparency = appearance
            .get("transparency")
            .cloned()
            .unwrap_or(Value::Null);
        let dock = root.get("dock").cloned().unwrap_or(Value::Null);
        let cheatsheet = root.get("cheatsheet").cloned().unwrap_or(Value::Null);
        let prefix = root
            .pointer("/search/prefix")
            .cloned()
            .unwrap_or(Value::Null);
        let overview = root.get("overview").cloned().unwrap_or(Value::Null);
        let region = root.get("regionSelector").cloned().unwrap_or(Value::Null);
        let targets = region.get("targetRegions").cloned().unwrap_or(Value::Null);
        let snip = root.get("screenSnip").cloned().unwrap_or(Value::Null);

        Config {
            corner_style: match bar.get("cornerStyle").and_then(Value::as_i64) {
                Some(1) => CornerStyle::Float,
                Some(2) => CornerStyle::Plain,
                _ => CornerStyle::Hug,
            },
            bottom: flag(&bar, "bottom", false),
            vertical: flag(&bar, "vertical", false),
            borderless: flag(&bar, "borderless", false),
            show_background: flag(&bar, "showBackground", true),
            float_shadow: flag(&bar, "floatStyleShadow", true),
            dock_enable: flag(&dock, "enable", false),
            dock_monochrome: flag(&dock, "monochromeIcons", true),
            dock_height: number(&dock, "height", 60.0) as i32,
            dock_hover_region: number(&dock, "hoverRegionHeight", 2.0) as i32,
            dock_pinned_on_startup: flag(&dock, "pinnedOnStartup", false),
            dock_hover_to_reveal: flag(&dock, "hoverToReveal", true),
            dock_pinned: RefCell::new(strings(&dock, "pinnedApps")),
            dock_ignored: strings(&dock, "ignoredAppRegexes"),
            region: RegionOptions {
                show_pointer: Cell::new(flag(&region, "showPointer", false)),
                countdown: Cell::new(number(&region, "countdownSeconds", 0.0) as i64),
                remember_region: Cell::new(flag(&region, "rememberRegion", false)),
                record_sound: Cell::new(flag(&region, "recordSound", false)),
                target_windows: flag(&targets, "windows", true),
                target_layers: flag(&targets, "layers", false),
                show_label: flag(&targets, "showLabel", false),
                target_opacity: number(&targets, "opacity", 0.3),
                selection_padding: number(&targets, "selectionPadding", 5.0),
                aim_lines: region
                    .pointer("/rect/showAimLines")
                    .and_then(Value::as_bool)
                    .unwrap_or(true),
                circle_stroke: region
                    .pointer("/circle/strokeWidth")
                    .and_then(Value::as_f64)
                    .unwrap_or(6.0),
                circle_padding: region
                    .pointer("/circle/padding")
                    .and_then(Value::as_f64)
                    .unwrap_or(10.0),
                save: Cell::new(flag(&snip, "save", false)),
                save_path: paths::expand_home(&text(&snip, "savePath", "")),
            },
            overview_enable: flag(&overview, "enable", true),
            overview_scale: number(&overview, "scale", 0.18),
            overview_rows: number(&overview, "rows", 2.0) as i32,
            overview_columns: number(&overview, "columns", 5.0) as i32,
            overview_right_left: flag(&overview, "orderRightLeft", false),
            overview_bottom_up: flag(&overview, "orderBottomUp", false),
            overview_center_icons: flag(&overview, "centerIcons", true),
            search_action: text(&prefix, "action", "/"),
            search_app: text(&prefix, "app", ">"),
            search_clipboard: text(&prefix, "clipboard", ";"),
            search_emojis: text(&prefix, "emojis", ":"),
            search_math: text(&prefix, "math", "="),
            search_shell: text(&prefix, "shellCommand", "$"),
            search_default_actions: flag(&prefix, "showDefaultActionsWithoutPrefix", true),
            search_non_app_delay: root
                .pointer("/search/nonAppResultDelay")
                .and_then(Value::as_u64)
                .unwrap_or(30),
            race_condition_delay: root
                .pointer("/hacks/arbitraryRaceConditionDelay")
                .and_then(Value::as_u64)
                .unwrap_or(20),
            safety_clipboard: root
                .pointer("/workSafety/enable/clipboard")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            safety_networks: strings_or(
                root.pointer("/workSafety/triggerCondition/networkNameKeywords"),
                &[
                    "airport",
                    "cafe",
                    "college",
                    "company",
                    "eduroam",
                    "free",
                    "guest",
                    "public",
                    "school",
                    "university",
                ],
            ),
            safety_wallpaper: root
                .pointer("/workSafety/enable/wallpaper")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            safety_files: strings_or(
                root.pointer("/workSafety/triggerCondition/fileKeywords"),
                &[
                    "anime", "booru", "ecchi", "hentai", "yande.re", "konachan", "breast",
                    "nipples", "pussy", "nsfw", "spoiler", "girl",
                ],
            ),
            safety_links: strings_or(
                root.pointer("/workSafety/triggerCondition/linkKeywords"),
                &[
                    "hentai",
                    "porn",
                    "sukebei",
                    "hitomi.la",
                    "rule34",
                    "gelbooru",
                    "fanbox",
                    "dlsite",
                ],
            ),
            app_terminal: text(&apps, "terminal", "kitty -1"),
            cheatsheet_super: text(&cheatsheet, "superKey", ""),
            cheatsheet_mac_symbols: flag(&cheatsheet, "useMacSymbol", false),
            cheatsheet_fn_symbols: flag(&cheatsheet, "useFnSymbol", false),
            cheatsheet_mouse_symbols: flag(&cheatsheet, "useMouseSymbol", false),
            cheatsheet_split: flag(&cheatsheet, "splitButtons", false),
            cheatsheet_key_size: cheatsheet
                .pointer("/fontSize/key")
                .and_then(Value::as_i64)
                .unwrap_or(12) as i32,
            cheatsheet_comment_size: cheatsheet
                .pointer("/fontSize/comment")
                .and_then(Value::as_i64)
                .unwrap_or(12) as i32,
            fake_rounding: root
                .pointer("/appearance/fakeScreenRounding")
                .and_then(Value::as_i64)
                .unwrap_or(2) as i32,
            dead_pixel: root
                .pointer("/interactions/deadPixelWorkaround/enable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            corner_open: root
                .pointer("/sidebar/cornerOpen/enable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            corner_bottom: root
                .pointer("/sidebar/cornerOpen/bottom")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            corner_value_scroll: root
                .pointer("/sidebar/cornerOpen/valueScroll")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            corner_clickless: root
                .pointer("/sidebar/cornerOpen/clickless")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            corner_width: root
                .pointer("/sidebar/cornerOpen/cornerRegionWidth")
                .and_then(Value::as_i64)
                .unwrap_or(250) as i32,
            corner_height: root
                .pointer("/sidebar/cornerOpen/cornerRegionHeight")
                .and_then(Value::as_i64)
                .unwrap_or(5) as i32,
            corner_visualize: root
                .pointer("/sidebar/cornerOpen/visualize")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            corner_clickless_end: root
                .pointer("/sidebar/cornerOpen/clicklessCornerEnd")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            corner_vertical_offset: root
                .pointer("/sidebar/cornerOpen/clicklessCornerVerticalOffset")
                .and_then(Value::as_i64)
                .unwrap_or(1) as i32,
            verbose: flag(&bar, "verbose", true),
            workspaces_shown: bar
                .pointer("/workspaces/shown")
                .and_then(Value::as_i64)
                .unwrap_or(10) as i32,
            workspaces_show_app_icons: flag(&workspaces, "showAppIcons", true),
            workspaces_always_show_numbers: flag(&workspaces, "alwaysShowNumbers", false),
            workspaces_monochrome_icons: flag(&workspaces, "monochromeIcons", true),
            workspaces_use_nerd_font: flag(&workspaces, "useNerdFont", false),
            workspaces_number_map: strings(&workspaces, "numberMap"),
            hover_region_width: bar
                .pointer("/autoHide/hoverRegionWidth")
                .and_then(Value::as_i64)
                .unwrap_or(2) as i32,
            auto_hide: flag(&auto_hide, "enable", false),
            auto_hide_push_windows: flag(&auto_hide, "pushWindows", false),
            super_show: auto_hide
                .pointer("/showWhenPressingSuper/enable")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            super_show_delay: auto_hide
                .pointer("/showWhenPressingSuper/delay")
                .and_then(Value::as_u64)
                .unwrap_or(140) as u32,
            weather_enable: bar
                .pointer("/weather/enable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            weather_city: bar
                .pointer("/weather/city")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned(),
            weather_gps: bar
                .pointer("/weather/enableGPS")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            weather_uscs: bar
                .pointer("/weather/useUSCS")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            weather_interval: bar
                .pointer("/weather/fetchInterval")
                .and_then(Value::as_i64)
                .unwrap_or(10) as i32,
            update_interval: root
                .pointer("/resources/updateInterval")
                .and_then(Value::as_u64)
                .unwrap_or(3000) as u32,
            time_second_precision: flag(&time, "secondPrecision", false),
            date_with_year_format: text(&time, "dateWithYearFormat", "dd/MM/yyyy"),
            util_updates: flag(&util, "showUpdates", true),
            util_screen_snip: flag(&util, "showScreenSnip", true),
            util_screen_record: flag(&util, "showScreenRecord", false),
            util_color_picker: flag(&util, "showColorPicker", false),
            util_mic: flag(&util, "showMicToggle", false),
            util_dark_mode: flag(&util, "showDarkModeToggle", true),
            util_power_profile: flag(&util, "showPerformanceProfileToggle", false),
            updates_enable_check: flag(&updates, "enableCheck", true),
            updates_interval: number(&updates, "checkInterval", 120.0) as i32,
            updates_advise: number(&updates, "adviseUpdateThreshold", 75.0) as i32,
            updates_strongly_advise: number(&updates, "stronglyAdviseUpdateThreshold", 200.0)
                as i32,
            app_update: text(&apps, "update", ""),
            battery_low: number(&battery, "low", 20.0) / 100.0,
            battery_critical: number(&battery, "critical", 5.0) / 100.0,
            battery_full: number(&battery, "full", 101.0) / 100.0,
            battery_automatic_suspend: flag(&battery, "automaticSuspend", true),
            battery_suspend: number(&battery, "suspend", 3.0) / 100.0,
            notifications_silent: flag(&notifications, "silent", false),
            notifications_show_unread_count: bar
                .pointer("/indicators/notifications/showUnreadCount")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            tray_monochrome_icons: flag(&tray, "monochromeIcons", true),
            tray_show_item_id: flag(&tray, "showItemId", false),
            font_nerd: root
                .pointer("/appearance/fonts/iconNerd")
                .and_then(Value::as_str)
                .unwrap_or("JetBrains Mono NF")
                .to_owned(),
            font_monospace: root
                .pointer("/appearance/fonts/monospace")
                .and_then(Value::as_str)
                .unwrap_or("JetBrains Mono NF")
                .to_owned(),
            screens: bar
                .get("screenList")
                .and_then(Value::as_array)
                .map(|list| {
                    list.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            gaps_out: hypr::gaps_out_top().unwrap_or(5),
            rounding: hypr::option_int("decoration:rounding").unwrap_or(23),
            font_main: root
                .pointer("/appearance/fonts/main")
                .and_then(Value::as_str)
                .unwrap_or("Google Sans")
                .to_owned(),
            font_title: text(&fonts, "title", "Google Sans"),
            font_reading: text(&fonts, "reading", "Readex Pro"),
            font_expressive: text(&fonts, "expressive", "Space Grotesk"),
            extra_background_tint: flag(&appearance, "extraBackgroundTint", true),
            transparency_enable: flag(&transparency, "enable", false),
            transparency_automatic: flag(&transparency, "automatic", true),
            background_transparency: number(&transparency, "backgroundTransparency", 0.11) as f32,
            content_transparency: number(&transparency, "contentTransparency", 0.57) as f32,
            time_format: text(&time, "format", "hh:mm"),
            date_format: text(&time, "dateFormat", "ddd, dd/MM"),
            short_date_format: text(&time, "shortDateFormat", "dd/MM"),
            memory_warning: number(&resources, "memoryWarningThreshold", 95.0),
            swap_warning: number(&resources, "swapWarningThreshold", 85.0),
            cpu_warning: number(&resources, "cpuWarningThreshold", 90.0),
            tray_pinned: RefCell::new(strings(&tray, "pinnedItems")),
            tray_invert_pinned: flag(&tray, "invertPinnedItems", true),
            tray_filter_passive: flag(&tray, "filterPassive", true),
            wallpaper: text(&background, "wallpaperPath", ""),
            thumbnail: text(&background, "thumbnailPath", ""),
            hide_when_fullscreen: flag(&background, "hideWhenFullscreen", true),
            parallax_workspace: flag(&parallax, "enableWorkspace", true),
            parallax_vertical: flag(&parallax, "vertical", false),
            parallax_auto_vertical: flag(&parallax, "autoVertical", false),
            parallax_sidebar: flag(&parallax, "enableSidebar", true),
            workspace_zoom: number(&parallax, "workspaceZoom", 1.07),
            quick_sliders: flag(&quick_sliders, "enable", false),
            slider_volume: flag(&quick_sliders, "showVolume", true),
            slider_mic: flag(&quick_sliders, "showMic", false),
            slider_brightness: flag(&quick_sliders, "showBrightness", true),
            toggles: RefCell::new(
                root.pointer("/sidebar/quickToggles/android/toggles")
                    .and_then(Value::as_array)
                    .map(|list| {
                        list.iter()
                            .filter_map(|entry| {
                                Some((
                                    entry.get("type")?.as_str()?.to_owned(),
                                    entry.get("size").and_then(Value::as_i64).unwrap_or(1) as i32,
                                ))
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            ),
            toggle_columns: root
                .pointer("/sidebar/quickToggles/android/columns")
                .and_then(Value::as_i64)
                .unwrap_or(5) as i32,
            toggle_style: root
                .pointer("/sidebar/quickToggles/style")
                .and_then(Value::as_str)
                .unwrap_or("android")
                .to_owned(),
            night_temperature: root
                .pointer("/light/night/colorTemperature")
                .and_then(Value::as_i64)
                .unwrap_or(5000) as i32,
            night_automatic: root
                .pointer("/light/night/automatic")
                .and_then(Value::as_bool)
                .unwrap_or(true),
            night_from: root
                .pointer("/light/night/from")
                .and_then(Value::as_str)
                .unwrap_or("19:00")
                .to_owned(),
            night_to: root
                .pointer("/light/night/to")
                .and_then(Value::as_str)
                .unwrap_or("06:30")
                .to_owned(),
            pomodoro_focus: number(&pomodoro, "focus", 1500.0) as i64,
            pomodoro_break: number(&pomodoro, "breakTime", 300.0) as i64,
            pomodoro_long_break: number(&pomodoro, "longBreak", 900.0) as i64,
            pomodoro_cycles: number(&pomodoro, "cyclesBeforeLongBreak", 4.0) as i64,
            sounds_pomodoro: flag(&sounds, "pomodoro", false),
            sounds_battery: flag(&sounds, "battery", false),
            sounds_theme: text(&sounds, "theme", "freedesktop"),
            protection: crate::services::audio::Protection {
                enable: flag(&protection, "enable", false),
                max_allowed_increase: number(&protection, "maxAllowedIncrease", 10.0) / 100.0,
                max_allowed: number(&protection, "maxAllowed", 99.0) / 100.0,
            },
        }
    }

    pub fn store_night(key: &str, value: Value) {
        store_at(&["light", "night"], key, value);
    }

    pub fn wants_screen(&self, name: &str) -> bool {
        self.screens.is_empty() || self.screens.iter().any(|wanted| wanted == name)
    }

    pub fn floating(&self) -> bool {
        self.corner_style == CornerStyle::Float
    }

    pub fn bar_height(&self) -> i32 {
        if self.floating() {
            BASE_BAR_HEIGHT + HYPRLAND_GAPS_OUT * 2
        } else {
            BASE_BAR_HEIGHT
        }
    }

    pub fn hug_rounding(&self) -> i32 {
        self.rounding
    }

    pub fn hug_corners(&self) -> bool {
        self.corner_style == CornerStyle::Hug && self.show_background
    }

    pub fn vertical_bar_width(&self) -> i32 {
        if self.floating() {
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
        if self.floating() {
            base + HYPRLAND_GAPS_OUT
        } else {
            base
        }
    }

    pub fn shortened(&self, screen_width: i32) -> i32 {
        match screen_width {
            width if width <= 1000 => 2,
            width if width <= 1200 => 1,
            _ => 0,
        }
    }

    pub fn center_side_width(&self, screen_width: i32) -> i32 {
        match self.shortened(screen_width) {
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
static RUNNING_RENDERER: std::sync::OnceLock<String> = std::sync::OnceLock::new();

pub fn choose_renderer() -> bool {
    if let Some(given) = std::env::var_os("GSK_RENDERER") {
        let _ = RUNNING_RENDERER.set(given.to_string_lossy().into_owned());
        return false;
    }
    let renderer = value_str(RENDERER).unwrap_or_else(|| DEFAULT_RENDERER.to_owned());
    unsafe { std::env::set_var("GSK_RENDERER", &renderer) };
    let _ = RUNNING_RENDERER.set(renderer);
    true
}

pub fn running_renderer() -> &'static str {
    RUNNING_RENDERER
        .get()
        .map_or(DEFAULT_RENDERER, String::as_str)
}

pub const RENDERER_FALLBACK: &str = "/rendererFallback";

pub fn renderer_fallback() -> Option<String> {
    value_str(RENDERER_FALLBACK).filter(|fallback| !fallback.is_empty())
}

pub fn pick_renderer(renderer: &str) {
    if renderer == running_renderer() {
        store_value(RENDERER_FALLBACK, Value::from(""));
    } else if renderer_fallback().is_none() {
        store_value(RENDERER_FALLBACK, Value::from(running_renderer()));
    }
    store_value(RENDERER, Value::from(renderer));
}

pub fn settle_renderer(keep: bool) {
    if !keep && let Some(fallback) = renderer_fallback() {
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
    let staging = path.with_extension("toml.tmp");
    if std::fs::write(&staging, text).is_ok() {
        let _ = std::fs::rename(staging, path);
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
    follow: watch::Watch,
}

thread_local! {
    static CURRENT: RefCell<Option<Current>> = const { RefCell::new(None) };
}

/// The config as the file says now, for code that must follow edits without a restart.
pub fn current() -> Rc<Config> {
    let generation = watch::generation();
    let fresh = CURRENT.with_borrow(|slot| {
        slot.as_ref()
            .filter(|current| current.generation == generation)
            .map(|current| current.config.clone())
    });
    if let Some(config) = fresh {
        return config;
    }
    let follow = CURRENT
        .with_borrow_mut(|slot| slot.take().map(|current| current.follow))
        .unwrap_or_else(|| watch::config("", || {}));
    let config = Rc::new(Config::load());
    CURRENT.with_borrow_mut(|slot| {
        *slot = Some(Current {
            config: config.clone(),
            generation: watch::generation(),
            follow,
        })
    });
    config
}

fn flag(node: &Value, key: &str, default: bool) -> bool {
    node.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn text(node: &Value, key: &str, default: &str) -> String {
    node.get(key)
        .and_then(Value::as_str)
        .unwrap_or(default)
        .to_owned()
}

fn number(node: &Value, key: &str, default: f64) -> f64 {
    node.get(key).and_then(Value::as_f64).unwrap_or(default)
}

fn strings_or(list: Option<&Value>, defaults: &[&str]) -> Vec<String> {
    match list.and_then(Value::as_array) {
        Some(list) => list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        None => defaults.iter().map(|word| (*word).to_owned()).collect(),
    }
}

fn strings(node: &Value, key: &str) -> Vec<String> {
    node.get(key)
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_emptied_keyword_list_stays_empty_and_only_a_missing_one_takes_the_defaults() {
        let defaults = ["cafe", "guest"];
        assert_eq!(
            strings_or(Some(&json!([])), &defaults),
            Vec::<String>::new()
        );
        assert_eq!(strings_or(Some(&json!(["home"])), &defaults), ["home"]);
        assert_eq!(strings_or(None, &defaults), ["cafe", "guest"]);
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
