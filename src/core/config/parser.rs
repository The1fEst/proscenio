use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;

use super::{
    BLUETOOTH_MEDIA_BUTTONS, CHEATSHEET_SUPER_KEY, Config, CornerStyle, DARK_MODE_DARK_FROM,
    DARK_MODE_LIGHT_FROM, DATE_FORMAT, DOCK_PINNED_APPS, NIGHT_FROM, NIGHT_TO,
    NIGHT_TRANSITION_MINUTES, RegionOptions, SLIDESHOW_INTERVAL_MINUTES,
    SLIDESHOW_INTERVAL_RANGE_MINUTES, ScheduleMode, SlideshowOrder, TIME_FORMAT,
};
use crate::core::paths;
use crate::platform::hyprland::hypr;

const POMODORO_MIN_SECONDS: i64 = 60;

impl Config {
    pub(super) fn parse(root: &Value) -> Self {
        let bar = root.get("bar").cloned().unwrap_or(Value::Null);
        let time = root.get("time").cloned().unwrap_or(Value::Null);
        let resources = bar.get("resources").cloned().unwrap_or(Value::Null);
        let tray = root.get("tray").cloned().unwrap_or(Value::Null);
        let background = root.get("background").cloned().unwrap_or(Value::Null);
        let parallax = background.get("parallax").cloned().unwrap_or(Value::Null);
        let slideshow = background.get("slideshow").cloned().unwrap_or(Value::Null);
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
            show_background: flag(&bar, "showBackground", true),
            float_shadow: flag(&bar, "floatStyleShadow", true),
            dock_enable: flag(&dock, "enable", false),
            dock_monochrome: flag(&dock, "monochromeIcons", true),
            dock_icon_size: number(&dock, "iconSize", 35.0) as i32,
            dock_hover_region: number(&dock, "hoverRegionHeight", 2.0) as i32,
            dock_pinned_on_startup: flag(&dock, "pinnedOnStartup", false),
            dock_hover_to_reveal: flag(&dock, "hoverToReveal", true),
            dock_shows_trash: flag(&dock, "showTrash", false),
            dock_pinned: RefCell::new(strings_or(dock.get("pinnedApps"), &DOCK_PINNED_APPS)),
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
                should_notify: flag(&snip, "notify", true),
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
            cheatsheet_super: text(&cheatsheet, "superKey", CHEATSHEET_SUPER_KEY),
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
            should_show_privacy_dots_in_fullscreen: flag(&bar, "privacyDotsInFullscreen", true),
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
            should_show_calendar_events: flag(&time, "calendarEvents", true),
            date_with_year_format: text(&time, "dateWithYearFormat", "dd/MM/yyyy"),
            util_screen_snip: flag(&util, "showScreenSnip", true),
            util_screen_record: flag(&util, "showScreenRecord", false),
            util_color_picker: flag(&util, "showColorPicker", false),
            util_keyboard: flag(&util, "showKeyboardToggle", true),
            util_mic: flag(&util, "showMicToggle", false),
            util_dark_mode: flag(&util, "showDarkModeToggle", true),
            util_power_profile: flag(&util, "showPerformanceProfileToggle", false),
            util_notepad: flag(&util, "showNotepad", false),
            updates_enable_check: flag(&updates, "enableCheck", true),
            updates_include_flatpak: flag(&updates, "includeFlatpak", true),
            updates_interval: number(&updates, "checkInterval", 120.0) as i32,
            updates_advise: number(&updates, "adviseUpdateThreshold", 1.0) as i32,
            updates_strongly_advise: number(&updates, "stronglyAdviseUpdateThreshold", 200.0)
                as i32,
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
            time_format: text(&time, "format", TIME_FORMAT),
            date_format: text(&time, "dateFormat", DATE_FORMAT),
            short_date_format: text(&time, "shortDateFormat", "dd/MM"),
            memory_warning: number(&resources, "memoryWarningThreshold", 95.0),
            swap_warning: number(&resources, "swapWarningThreshold", 85.0),
            cpu_warning: number(&resources, "cpuWarningThreshold", 90.0),
            tray_pinned: RefCell::new(strings(&tray, "pinnedItems")),
            tray_invert_pinned: flag(&tray, "invertPinnedItems", true),
            tray_filter_passive: flag(&tray, "filterPassive", true),
            wallpaper: text(&background, "wallpaperPath", ""),
            thumbnail: text(&background, "thumbnailPath", ""),
            is_wallpaper_per_monitor: root
                .pointer("/background/perMonitor/enable")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            monitor_wallpapers: read_monitor_wallpapers(root),
            is_slideshow_enabled: flag(&slideshow, "enable", false),
            slideshow_interval_minutes: integer_within(
                &slideshow,
                "intervalMinutes",
                SLIDESHOW_INTERVAL_MINUTES,
                SLIDESHOW_INTERVAL_RANGE_MINUTES,
            ) as u64,
            slideshow_order: parse_slideshow_order(slideshow.get("order")),
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
            toggles: RefCell::new(toggles_of(root)),
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
            night_schedule: parse_schedule_mode(root.pointer("/light/night/schedule")),
            night_from: root
                .pointer("/light/night/from")
                .and_then(Value::as_str)
                .unwrap_or(NIGHT_FROM)
                .to_owned(),
            night_to: root
                .pointer("/light/night/to")
                .and_then(Value::as_str)
                .unwrap_or(NIGHT_TO)
                .to_owned(),
            night_transition: root
                .pointer("/light/night/transition")
                .and_then(Value::as_i64)
                .map_or(NIGHT_TRANSITION_MINUTES, |minutes| {
                    minutes.clamp(0, 120) as i32
                }),
            is_dark_mode_automatic: root
                .pointer("/appearance/darkMode/automatic")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            dark_mode_schedule: parse_schedule_mode(root.pointer("/appearance/darkMode/schedule")),
            dark_mode_dark_from: root
                .pointer("/appearance/darkMode/darkFrom")
                .and_then(Value::as_str)
                .unwrap_or(DARK_MODE_DARK_FROM)
                .to_owned(),
            dark_mode_light_from: root
                .pointer("/appearance/darkMode/lightFrom")
                .and_then(Value::as_str)
                .unwrap_or(DARK_MODE_LIGHT_FROM)
                .to_owned(),
            pomodoro_focus: integer_within(
                &pomodoro,
                "focus",
                1500,
                (POMODORO_MIN_SECONDS, 180 * 60),
            ),
            pomodoro_break: integer_within(
                &pomodoro,
                "breakTime",
                300,
                (POMODORO_MIN_SECONDS, 60 * 60),
            ),
            pomodoro_long_break: integer_within(
                &pomodoro,
                "longBreak",
                900,
                (POMODORO_MIN_SECONDS, 120 * 60),
            ),
            pomodoro_cycles: integer_within(&pomodoro, "cyclesBeforeLongBreak", 4, (1, i64::MAX)),
            sounds_pomodoro: flag(&sounds, "pomodoro", false),
            sounds_battery: flag(&sounds, "battery", false),
            sounds_devices: flag(&sounds, "devices", true),
            sounds_notifications: flag(&sounds, "notifications", false),
            sounds_theme: text(&sounds, "theme", "freedesktop"),
            protection: crate::services::media::audio::Protection {
                is_enabled: flag(&protection, "enable", false),
                max_allowed_increase: number(&protection, "maxAllowedIncrease", 10.0) / 100.0,
                max_allowed: number(&protection, "maxAllowed", 99.0) / 100.0,
            },
            audio_aliases: read_aliases(root),
            has_bluetooth_media_buttons: root
                .pointer(BLUETOOTH_MEDIA_BUTTONS)
                .and_then(Value::as_bool)
                .unwrap_or(true),
        }
    }
}

fn read_aliases(root: &Value) -> HashMap<String, String> {
    root.pointer("/audio/aliases")
        .and_then(Value::as_object)
        .map(|aliases| {
            aliases
                .iter()
                .filter_map(|(name, alias)| {
                    let alias = alias.as_str()?.trim();
                    (!alias.is_empty()).then(|| (name.clone(), alias.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn read_monitor_wallpapers(root: &Value) -> HashMap<String, String> {
    root.pointer("/background/perMonitor/wallpapers")
        .and_then(Value::as_object)
        .map(|wallpapers| {
            wallpapers
                .iter()
                .filter_map(|(monitor, path)| {
                    let path = path.as_str()?.trim();
                    (!path.is_empty()).then(|| (monitor.clone(), path.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn parse_slideshow_order(node: Option<&Value>) -> SlideshowOrder {
    match node.and_then(Value::as_str) {
        Some("name") => SlideshowOrder::Name,
        _ => SlideshowOrder::Shuffle,
    }
}

const DEFAULT_TOGGLES: [(&str, i32); 6] = [
    ("network", 2),
    ("bluetooth", 2),
    ("idleInhibitor", 1),
    ("mic", 1),
    ("audio", 2),
    ("nightLight", 2),
];

fn toggles_of(root: &Value) -> Vec<(String, i32)> {
    let Some(list) = root
        .pointer("/sidebar/quickToggles/android/toggles")
        .and_then(Value::as_array)
    else {
        return DEFAULT_TOGGLES
            .iter()
            .map(|(kind, size)| ((*kind).to_owned(), *size))
            .collect();
    };
    list.iter()
        .filter_map(|entry| {
            Some((
                entry.get("type")?.as_str()?.to_owned(),
                entry.get("size").and_then(Value::as_i64).unwrap_or(1) as i32,
            ))
        })
        .collect()
}

fn flag(node: &Value, key: &str, default: bool) -> bool {
    node.get(key).and_then(Value::as_bool).unwrap_or(default)
}

fn parse_schedule_mode(node: Option<&Value>) -> ScheduleMode {
    match node.and_then(Value::as_str) {
        Some("sun") => ScheduleMode::Sun,
        _ => ScheduleMode::Times,
    }
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

fn integer_within(node: &Value, key: &str, default: i64, range: (i64, i64)) -> i64 {
    (number(node, key, default as f64) as i64).clamp(range.0, range.1)
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
    fn quick_toggles_default_to_the_six_of_config_qml_only_when_none_are_listed() {
        let defaults = toggles_of(&json!({}));
        assert_eq!(defaults.len(), 6);
        assert_eq!(defaults[0], ("network".to_owned(), 2));
        assert_eq!(defaults[2], ("idleInhibitor".to_owned(), 1));
        let empty_list = json!({ "sidebar": { "quickToggles": { "android": { "toggles": [] } } } });
        assert!(toggles_of(&empty_list).is_empty());
        let own_list = json!({ "sidebar": { "quickToggles": { "android": { "toggles": [{ "type": "mic" }] } } } });
        assert_eq!(toggles_of(&own_list), [("mic".to_owned(), 1)]);
    }

    #[test]
    fn notification_sounds_are_off_until_switched_on() {
        assert!(!Config::parse(&json!({})).sounds_notifications);
        let root = json!({ "sounds": { "notifications": true } });
        assert!(Config::parse(&root).sounds_notifications);
    }

    #[test]
    fn only_the_sun_value_selects_the_sun_schedule() {
        let mode = |value: Value| {
            Config::parse(&json!({ "light": { "night": { "schedule": value } } })).night_schedule
        };
        assert_eq!(mode(json!("sun")), ScheduleMode::Sun);
        for other in [json!("times"), json!("Sun"), json!(1), Value::Null] {
            assert_eq!(mode(other), ScheduleMode::Times);
        }
        assert_eq!(
            Config::parse(&json!({})).night_schedule,
            ScheduleMode::Times
        );
    }

    #[test]
    fn bluetooth_media_buttons_control_the_player_unless_turned_off() {
        let parse = |root: Value| Config::parse(&root).has_bluetooth_media_buttons;
        assert!(parse(json!({})));
        assert!(parse(json!({ "bluetooth": { "mediaButtons": "no" } })));
        assert!(!parse(json!({ "bluetooth": { "mediaButtons": false } })));
    }

    #[test]
    fn audio_aliases_keep_trimmed_text_values_by_device_name() {
        assert!(read_aliases(&json!({})).is_empty());
        let root = json!({ "audio": { "aliases": {
            "alsa_output.pci-0000_00_1f.3.analog-stereo": " Desk speakers ",
            "blank": "  ",
            "number": 3,
        } } });
        assert_eq!(
            Config::parse(&root).audio_aliases,
            HashMap::from([(
                "alsa_output.pci-0000_00_1f.3.analog-stereo".to_owned(),
                "Desk speakers".to_owned()
            )])
        );
    }

    #[test]
    fn automatic_dark_mode_is_off_and_keeps_the_times_it_is_given() {
        let config = Config::parse(&json!({}));
        assert!(!config.is_dark_mode_automatic);
        assert_eq!(config.dark_mode_schedule, ScheduleMode::Times);
        assert_eq!(config.dark_mode_dark_from, "20:00");
        assert_eq!(config.dark_mode_light_from, "07:00");
        let config = Config::parse(&json!({ "appearance": { "darkMode": {
            "automatic": true,
            "schedule": "sun",
            "darkFrom": "21:15",
            "lightFrom": "06:00",
        } } }));
        assert!(config.is_dark_mode_automatic);
        assert_eq!(config.dark_mode_schedule, ScheduleMode::Sun);
        assert_eq!(config.dark_mode_dark_from, "21:15");
        assert_eq!(config.dark_mode_light_from, "06:00");
    }

    #[test]
    fn a_position_is_needed_only_by_a_sun_schedule_that_is_on_or_by_the_weather_with_gps() {
        let needs_position = |root: Value| Config::parse(&root).needs_position();
        assert!(!needs_position(json!({})));
        let night = |is_automatic: bool, schedule: &str| json!({ "light": { "night": { "automatic": is_automatic, "schedule": schedule } } });
        let dark_mode = |is_automatic: bool, schedule: &str| json!({ "appearance": { "darkMode": { "automatic": is_automatic, "schedule": schedule } } });
        let weather = |is_enabled: bool, is_gps: bool| json!({ "bar": { "weather": { "enable": is_enabled, "enableGPS": is_gps } } });
        assert!(needs_position(night(true, "sun")));
        assert!(!needs_position(night(false, "sun")));
        assert!(!needs_position(night(true, "times")));
        assert!(needs_position(dark_mode(true, "sun")));
        assert!(!needs_position(dark_mode(true, "times")));
        assert!(!needs_position(dark_mode(false, "sun")));
        assert!(needs_position(weather(true, true)));
        assert!(!needs_position(weather(true, false)));
        assert!(!needs_position(weather(false, true)));
    }

    #[test]
    fn privacy_dots_over_fullscreen_windows_are_on_unless_switched_off() {
        assert!(Config::parse(&json!({})).should_show_privacy_dots_in_fullscreen);
        let enabled_root = json!({ "bar": { "privacyDotsInFullscreen": true } });
        assert!(Config::parse(&enabled_root).should_show_privacy_dots_in_fullscreen);
        let disabled_root = json!({ "bar": { "privacyDotsInFullscreen": false } });
        assert!(!Config::parse(&disabled_root).should_show_privacy_dots_in_fullscreen);
    }

    #[test]
    fn calendar_events_are_on_unless_the_config_turns_them_off() {
        let parse = |root: Value| Config::parse(&root).should_show_calendar_events;
        assert!(parse(json!({})));
        assert!(parse(json!({ "time": { "calendarEvents": true } })));
        assert!(!parse(json!({ "time": { "calendarEvents": false } })));
    }

    #[test]
    fn pomodoro_lengths_and_cycles_outside_their_range_are_clamped() {
        let pomodoro = |config: &Config| {
            (
                config.pomodoro_focus,
                config.pomodoro_break,
                config.pomodoro_long_break,
                config.pomodoro_cycles,
            )
        };
        assert_eq!(pomodoro(&Config::parse(&json!({}))), (1500, 300, 900, 4));
        for (focus, cycles) in [(json!(0), json!(0)), (json!(-30), json!(0.5))] {
            let root = json!({ "time": { "pomodoro": {
                "focus": focus,
                "breakTime": 0.5,
                "longBreak": 59,
                "cyclesBeforeLongBreak": cycles,
            } } });
            assert_eq!(pomodoro(&Config::parse(&root)), (60, 60, 60, 1));
        }
        let root = json!({ "time": { "pomodoro": {
            "focus": 1e300,
            "breakTime": 3601,
            "longBreak": 9.3e18,
        } } });
        assert_eq!(
            pomodoro(&Config::parse(&root)),
            (180 * 60, 60 * 60, 120 * 60, 4)
        );
    }

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
    fn the_slideshow_is_off_and_takes_its_settings_within_range() {
        let defaults = Config::parse(&json!({}));
        assert!(!defaults.is_slideshow_enabled);
        assert_eq!(defaults.slideshow_interval_minutes, 30);
        assert_eq!(defaults.slideshow_order, SlideshowOrder::Shuffle);
        let slideshow =
            |node: Value| Config::parse(&json!({ "background": { "slideshow": node } }));
        let configured =
            slideshow(json!({ "enable": true, "intervalMinutes": 5, "order": "name" }));
        assert!(configured.is_slideshow_enabled);
        assert_eq!(configured.slideshow_interval_minutes, 5);
        assert_eq!(configured.slideshow_order, SlideshowOrder::Name);
        for (minutes, expected) in [(0, 1), (-4, 1), (99999, 1440)] {
            let config = slideshow(json!({ "intervalMinutes": minutes }));
            assert_eq!(config.slideshow_interval_minutes, expected, "{minutes}");
        }
        for order in [json!("Name"), json!("shuffle"), json!(3)] {
            assert_eq!(
                slideshow(json!({ "order": order })).slideshow_order,
                SlideshowOrder::Shuffle
            );
        }
    }

    #[test]
    fn a_monitor_takes_its_own_wallpaper_only_while_per_monitor_wallpapers_are_on() {
        let root = |is_enabled: bool| {
            json!({ "background": {
                "wallpaperPath": "/main.png",
                "perMonitor": {
                    "enable": is_enabled,
                    "wallpapers": { "DP-2": " /side.png ", "DP-3": "  ", "DP-4": 7 },
                },
            } })
        };
        let enabled = Config::parse(&root(true));
        assert_eq!(enabled.wallpaper_for("DP-2"), "/side.png");
        for monitor in ["DP-1", "DP-3", "DP-4"] {
            assert_eq!(enabled.wallpaper_for(monitor), "/main.png", "{monitor}");
        }
        let disabled = Config::parse(&root(false));
        assert_eq!(disabled.wallpaper_for("DP-2"), "/main.png");
        let unset = Config::parse(&json!({ "background": { "wallpaperPath": "/main.png" } }));
        assert!(!unset.is_wallpaper_per_monitor);
        assert_eq!(unset.wallpaper_for("DP-2"), "/main.png");
    }
}
