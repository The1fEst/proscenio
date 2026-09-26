use gtk4::gio;
use gtk4::prelude::*;

const INTERFACE: &str = "org.gnome.desktop.interface";

pub fn open(schema: &str) -> Option<gio::Settings> {
    gio::SettingsSchemaSource::default()?.lookup(schema, true)?;
    Some(gio::Settings::new(schema))
}

pub fn prefers_dark() -> bool {
    open(INTERFACE).is_some_and(|settings| settings.string("color-scheme") == "prefer-dark")
}

pub fn set_dark(dark: bool) {
    let Some(settings) = open(INTERFACE) else {
        return;
    };
    let (scheme, theme) = if dark {
        ("prefer-dark", "adw-gtk3-dark")
    } else {
        ("prefer-light", "adw-gtk3")
    };
    let _ = settings.set_string("color-scheme", scheme);
    let _ = settings.set_string("gtk-theme", theme);
    gio::Settings::sync();
}
