use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::prelude::*;
use gtk4::{gio, glib};
use std::path::{Path, PathBuf};

use crate::core::i18n::{tr, trf};
use crate::platform::dbus::notify::{self, Notification};

const APP_NAME: &str = "Screenshot";
const PREVIEW_SIZE: i32 = 480;
const OPEN_KEY: &str = "default";
const OPEN_FOLDER_KEY: &str = "folder";

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let mut is_copied = false;
    let mut file = None;
    let mut preview = None;
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--copied" => is_copied = true,
            "--file" => file = rest.next().map(PathBuf::from),
            "--preview" => preview = rest.next().map(PathBuf::from),
            _ => {}
        }
    }
    let file_name = file
        .as_deref()
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned());
    let image = preview
        .and_then(|path| Pixbuf::from_file_at_scale(path, PREVIEW_SIZE, PREVIEW_SIZE, true).ok());
    let actions = list_actions(file.is_some());
    let action_pairs: Vec<(&str, &str)> = actions
        .iter()
        .map(|(key, label)| (*key, label.as_str()))
        .collect();
    let key = notify::send_blocking(&Notification {
        app: APP_NAME,
        summary: &tr("Screenshot taken"),
        body: &format_body(is_copied, file_name.as_deref()),
        image_path: &file
            .as_deref()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default(),
        image: image.as_ref(),
        is_transient: true,
        actions: &action_pairs,
        ..Default::default()
    });
    let Some(file) = file else {
        return glib::ExitCode::SUCCESS;
    };
    match key.as_deref() {
        Some(OPEN_KEY) => open(&file),
        Some(OPEN_FOLDER_KEY) => {
            if let Some(folder) = file.parent() {
                open(folder);
            }
        }
        _ => {}
    }
    glib::ExitCode::SUCCESS
}

fn format_body(is_copied: bool, file_name: Option<&str>) -> String {
    match (is_copied, file_name) {
        (true, Some(name)) => trf("Copied to the clipboard and saved as %1", &[name]),
        (false, Some(name)) => trf("Saved as %1", &[name]),
        (true, None) => tr("Copied to the clipboard"),
        (false, None) => String::new(),
    }
}

fn list_actions(has_file: bool) -> Vec<(&'static str, String)> {
    if !has_file {
        return Vec::new();
    }
    vec![(OPEN_KEY, tr("Open")), (OPEN_FOLDER_KEY, tr("Open folder"))]
}

fn open(location: &Path) {
    let uri = gio::File::for_path(location).uri();
    let _ = gio::AppInfo::launch_default_for_uri(&uri, gio::AppLaunchContext::NONE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::i18n::use_language;

    #[test]
    fn the_body_says_what_happened_to_the_screenshot() {
        use_language("en_US");
        let cases = [
            (true, None, "Copied to the clipboard"),
            (false, Some("a.png"), "Saved as a.png"),
            (
                true,
                Some("a.png"),
                "Copied to the clipboard and saved as a.png",
            ),
            (false, None, ""),
        ];
        for (is_copied, file_name, expected) in cases {
            assert_eq!(format_body(is_copied, file_name), expected);
        }
    }

    #[test]
    fn only_a_saved_screenshot_offers_actions() {
        use_language("en_US");
        assert_eq!(
            list_actions(true),
            [
                ("default", "Open".to_owned()),
                ("folder", "Open folder".to_owned())
            ]
        );
        assert!(list_actions(false).is_empty());
    }
}
