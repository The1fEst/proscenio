use gtk4::gio;
use gtk4::prelude::*;

use crate::platform::hyprland::{hyprconfig, hyprpm};

const INTERFACE: &str = "org.gnome.desktop.interface";
const WM_PREFERENCES: &str = "org.gnome.desktop.wm.preferences";

pub fn open(schema: &str) -> Option<gio::Settings> {
    gio::SettingsSchemaSource::default()?.lookup(schema, true)?;
    Some(gio::Settings::new(schema))
}

pub fn prefers_dark() -> bool {
    open(INTERFACE).is_some_and(|settings| settings.string("color-scheme") == "prefer-dark")
}

pub fn set_dark(is_dark: bool) {
    let Some(settings) = open(INTERFACE) else {
        return;
    };
    let (scheme, theme) = if is_dark {
        ("prefer-dark", "adw-gtk3-dark")
    } else {
        ("prefer-light", "adw-gtk3")
    };
    let _ = settings.set_string("color-scheme", scheme);
    let _ = settings.set_string("gtk-theme", theme);
    gio::Settings::sync();
}

fn button_layout(are_buttons_hidden: bool, are_buttons_on_left: bool) -> &'static str {
    match (are_buttons_hidden, are_buttons_on_left) {
        (true, _) => ":",
        (false, false) => "appmenu:minimize,maximize,close",
        (false, true) => "close,minimize,maximize:appmenu",
    }
}

pub fn sync_button_layout() {
    let Some(settings) = open(WM_PREFERENCES) else {
        return;
    };
    let are_buttons_hidden = hyprpm::is_plugin_enabled_in_hyprpm()
        && !hyprconfig::are_lines_present(&hyprconfig::TITLE_BARS_ON_ALL_WINDOWS);
    let layout = button_layout(
        are_buttons_hidden,
        hyprconfig::are_lines_present(&hyprconfig::TITLE_BUTTONS_ON_LEFT),
    );
    if settings.string("button-layout") == layout {
        return;
    }
    let _ = settings.set_string("button-layout", layout);
    gio::Settings::sync();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_layout_follows_the_title_bars() {
        let cases = [
            ("bars on floating windows only", true, false, ":"),
            (
                "bars on floating windows only, buttons on the left",
                true,
                true,
                ":",
            ),
            (
                "buttons on the right",
                false,
                false,
                "appmenu:minimize,maximize,close",
            ),
            (
                "buttons on the left",
                false,
                true,
                "close,minimize,maximize:appmenu",
            ),
        ];
        for (name, are_buttons_hidden, are_buttons_on_left, expected) in cases {
            assert_eq!(
                button_layout(are_buttons_hidden, are_buttons_on_left),
                expected,
                "{name}"
            );
        }
    }
}
