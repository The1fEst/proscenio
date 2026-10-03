use gtk4::gio;

use crate::core::assets;
use crate::platform::appearance::{self, Parts};
use crate::platform::defaultapps;
use crate::theming::switchwall;

const APPS: [(&str, &str); 8] = [
    ("web", "brave-origin.desktop"),
    ("mail", "org.mozilla.Thunderbird.desktop"),
    ("calendar", "org.gnome.Calendar.desktop"),
    ("music", "vlc.desktop"),
    ("video", "vlc.desktop"),
    ("photos", "satty.desktop"),
    ("text", "com.microsoft.VSCode.desktop"),
    ("files", "org.kde.dolphin.desktop"),
];

const TEXT_FONT: &str = "Google Sans";
const MONOSPACE_FONT: &str = "JetBrainsMono Nerd Font";
const FONT_STYLE: &str = "Medium";
const FONTS: [(&str, &str, i64); 6] = [
    ("general", TEXT_FONT, 11),
    ("fixed", MONOSPACE_FONT, 11),
    ("small", TEXT_FONT, 9),
    ("toolbar", TEXT_FONT, 10),
    ("menu", TEXT_FONT, 11),
    ("title", TEXT_FONT, 10),
];

const CURSOR_THEME: &str = "Bibata-Modern-Ice";
const CURSOR_SIZE: i64 = 36;
const ICON_THEME: &str = "Papirus-Dark";
const GTK_THEME: &str = "adw-gtk3-dark";
const QT_STYLE: &str = "Darkly";

pub fn apply() {
    for (key, entry) in APPS {
        defaultapps::set(key, entry);
    }
    gio::spawn_blocking(|| {
        for (role, family, size) in FONTS {
            let parts = Parts {
                family: Some(family.to_owned()),
                style: Some(FONT_STYLE.to_owned()),
                size: Some(size),
            };
            appearance::set_fonts(role, &parts);
        }
        appearance::set_themes(GTK_THEME, QT_STYLE);
        appearance::set_icons(ICON_THEME);
        appearance::set_cursor(CURSOR_THEME, CURSOR_SIZE);
    });
    if let Some(wallpaper) = assets::default_wallpaper() {
        switchwall::detach(&["--image", &wallpaper.to_string_lossy()]);
    }
}
