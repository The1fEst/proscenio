const PAGES: [(&str, &str); 17] = [
    ("modules/settings/QuickConfig.qml", "quick"),
    ("modules/settings/WifiConfig.qml", "wifi"),
    ("modules/settings/NetworkConfig.qml", "network"),
    ("modules/settings/BluetoothConfig.qml", "bluetooth"),
    ("modules/settings/DisplaysConfig.qml", "displays"),
    ("modules/settings/SoundConfig.qml", "sound"),
    ("modules/settings/PowerConfig.qml", "power"),
    ("modules/settings/MultitaskingConfig.qml", "multitasking"),
    ("modules/settings/AppearanceConfig.qml", "appearance"),
    ("modules/settings/AppsConfig.qml", "apps"),
    ("modules/settings/NotificationsConfig.qml", "notifications"),
    ("modules/settings/SearchConfig.qml", "search"),
    ("modules/settings/MouseConfig.qml", "mouse"),
    ("modules/settings/KeyboardConfig.qml", "keyboard"),
    ("modules/settings/AccessibilityConfig.qml", "accessibility"),
    ("modules/settings/PrivacyConfig.qml", "privacy"),
    ("modules/settings/SystemConfig.qml", "system"),
];

pub fn native(page: &str) -> &str {
    PAGES
        .iter()
        .find(|(component, _)| *component == page)
        .map_or(page, |(_, id)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::settings::pages;

    #[test]
    fn every_qs_page_path_names_a_settings_page() {
        for (component, _) in PAGES {
            assert!(pages::index_of(native(component)).is_some(), "{component}");
        }
        assert_eq!(native("wifi"), "wifi");
    }
}
