use serde_json::Value;
use std::cell::{Cell, RefCell};

use super::{
    CHEATSHEET_SUPER_KEY, Config, CornerStyle, DATE_FORMAT, DOCK_PINNED_APPS, NIGHT_FROM, NIGHT_TO,
    NIGHT_TRANSITION_MINUTES, RegionOptions, TIME_FORMAT,
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
            dock_icon_size: number(&dock, "iconSize", 35.0) as i32,
            dock_hover_region: number(&dock, "hoverRegionHeight", 2.0) as i32,
            dock_pinned_on_startup: flag(&dock, "pinnedOnStartup", false),
            dock_hover_to_reveal: flag(&dock, "hoverToReveal", true),
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
            util_screen_snip: flag(&util, "showScreenSnip", true),
            util_screen_record: flag(&util, "showScreenRecord", false),
            util_color_picker: flag(&util, "showColorPicker", false),
            util_keyboard: flag(&util, "showKeyboardToggle", true),
            util_mic: flag(&util, "showMicToggle", false),
            util_dark_mode: flag(&util, "showDarkModeToggle", true),
            util_power_profile: flag(&util, "showPerformanceProfileToggle", false),
            updates_enable_check: flag(&updates, "enableCheck", true),
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
            sounds_theme: text(&sounds, "theme", "freedesktop"),
            protection: crate::services::media::audio::Protection {
                is_enabled: flag(&protection, "enable", false),
                max_allowed_increase: number(&protection, "maxAllowedIncrease", 10.0) / 100.0,
                max_allowed: number(&protection, "maxAllowed", 99.0) / 100.0,
            },
        }
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
}
