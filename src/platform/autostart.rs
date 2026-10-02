use gtk4::glib;
use std::path::{Path, PathBuf};

use crate::core::i18n::tr;
use crate::platform::desktop;

const GROUP: &str = "Desktop Entry";
const GNOME_ENABLED: &str = "X-GNOME-Autostart-enabled";
const STARTED: &str = "autostarted";

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub icon: String,
    pub command: String,
    pub enabled: bool,
}

pub fn directory() -> PathBuf {
    glib::user_config_dir().join("autostart")
}

fn key_file(path: &Path) -> Option<glib::KeyFile> {
    let file = glib::KeyFile::new();
    file.load_from_file(
        path,
        glib::KeyFileFlags::KEEP_COMMENTS | glib::KeyFileFlags::KEEP_TRANSLATIONS,
    )
    .ok()?;
    Some(file)
}

fn string(file: &glib::KeyFile, key: &str) -> String {
    file.string(GROUP, key)
        .map(|value| value.to_string())
        .unwrap_or_default()
}

fn flag(file: &glib::KeyFile, key: &str, fallback: bool) -> bool {
    file.boolean(GROUP, key).unwrap_or(fallback)
}

fn enabled(file: &glib::KeyFile) -> bool {
    !flag(file, "Hidden", false) && flag(file, GNOME_ENABLED, true)
}

pub fn list() -> Vec<Entry> {
    let Ok(listing) = std::fs::read_dir(directory()) else {
        return Vec::new();
    };
    let mut entries: Vec<Entry> = listing
        .flatten()
        .map(|item| item.path())
        .filter(|path| path.extension().and_then(|extension| extension.to_str()) == Some("desktop"))
        .filter_map(|path| {
            let file = key_file(&path)?;
            let name = file
                .locale_string(GROUP, "Name", None)
                .map(|value| value.to_string())
                .unwrap_or_else(|_| {
                    path.file_stem()
                        .map(|stem| stem.to_string_lossy().into_owned())
                        .unwrap_or_default()
                });
            Some(Entry {
                name,
                icon: string(&file, "Icon"),
                command: string(&file, "Exec"),
                enabled: enabled(&file),
                path,
            })
        })
        .collect();
    entries.sort_by_key(|entry| entry.name.to_lowercase());
    entries
}

pub fn set_enabled(path: &Path, on: bool) -> Result<(), String> {
    let file = key_file(path).ok_or_else(|| tr("The entry could not be read"))?;
    file.set_boolean(GROUP, "Hidden", !on);
    if file.has_key(GROUP, GNOME_ENABLED).unwrap_or(false) {
        file.set_boolean(GROUP, GNOME_ENABLED, on);
    }
    file.save_to_file(path)
        .map_err(|error| error.message().to_owned())
}

pub fn add_application(id: &str) -> Result<(), String> {
    let source = std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .map(|root| root.join("applications").join(id))
        .find(|path| path.is_file())
        .ok_or_else(|| tr("The application could not be found"))?;
    let target = directory().join(id);
    std::fs::create_dir_all(directory()).map_err(|error| error.to_string())?;
    std::fs::copy(source, &target).map_err(|error| error.to_string())?;
    set_enabled(&target, true)
}

pub fn file_name(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if slug.is_empty() {
        "command".to_owned()
    } else {
        slug
    };
    std::iter::once(format!("{base}.desktop"))
        .chain((2..).map(|number| format!("{base}-{number}.desktop")))
        .find(|candidate| !taken(candidate))
        .unwrap_or_default()
}

pub fn add_command(name: &str, command: &str) -> Result<(), String> {
    if glib::shell_parse_argv(command).is_err() {
        return Err(tr("The command could not be read"));
    }
    std::fs::create_dir_all(directory()).map_err(|error| error.to_string())?;
    let file = glib::KeyFile::new();
    file.set_string(GROUP, "Type", "Application");
    file.set_string(GROUP, "Name", if name.is_empty() { command } else { name });
    file.set_string(GROUP, "Exec", command);
    let target = directory().join(file_name(name, |candidate| {
        directory().join(candidate).exists()
    }));
    file.save_to_file(target)
        .map_err(|error| error.message().to_owned())
}

pub fn remove(path: &Path) -> Result<(), String> {
    std::fs::remove_file(path).map_err(|error| error.to_string())
}

pub fn for_desktop(file: &glib::KeyFile, desktops: &[String]) -> bool {
    let list = |key: &str| -> Vec<String> {
        file.string_list(GROUP, key)
            .map(|items| items.iter().map(|item| item.to_string()).collect())
            .unwrap_or_default()
    };
    let only = list("OnlyShowIn");
    let not = list("NotShowIn");
    let shown = only.is_empty() || only.iter().any(|desktop| desktops.contains(desktop));
    shown && !not.iter().any(|desktop| desktops.contains(desktop))
}

pub fn start_once() {
    let marker = glib::user_runtime_dir().join("proscenio").join(STARTED);
    if marker.exists() {
        return;
    }
    if let Some(parent) = marker.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&marker, "");
    let desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .map(str::to_owned)
        .collect();
    for entry in list().into_iter().filter(|entry| entry.enabled) {
        let Some(file) = key_file(&entry.path) else {
            continue;
        };
        let try_exec = string(&file, "TryExec");
        let runnable = try_exec.is_empty() || glib::find_program_in_path(&try_exec).is_some();
        if !for_desktop(&file, &desktops) || !runnable {
            continue;
        }
        if let Some(found) = desktop::load(&entry.path) {
            desktop::launch(&found);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_gets_a_free_file_name_from_its_name() {
        let taken = |candidate: &str| candidate == "sync-notes.desktop";
        assert_eq!(file_name("Sync Notes!", taken), "sync-notes-2.desktop");
        assert_eq!(file_name("Backup", taken), "backup.desktop");
        assert_eq!(file_name("Заметки", taken), "command.desktop");
    }

    #[test]
    fn entries_respect_only_and_not_show_in() {
        let file = |text: &str| {
            let file = glib::KeyFile::new();
            file.load_from_data(text, glib::KeyFileFlags::NONE).unwrap();
            file
        };
        let here = vec!["Hyprland".to_owned()];
        assert!(for_desktop(&file("[Desktop Entry]\nName=a\n"), &here));
        assert!(!for_desktop(
            &file("[Desktop Entry]\nOnlyShowIn=KDE;\n"),
            &here
        ));
        assert!(for_desktop(
            &file("[Desktop Entry]\nOnlyShowIn=KDE;Hyprland;\n"),
            &here
        ));
        assert!(!for_desktop(
            &file("[Desktop Entry]\nNotShowIn=Hyprland;\n"),
            &here
        ));
    }
}
