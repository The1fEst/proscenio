use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

const GROUP: &str = "Desktop Entry";
const ACTION_GROUP: &str = "Desktop Action";
const SCOPE: [&str; 5] = ["systemd-run", "--user", "--scope", "--quiet", "--collect"];

pub struct DesktopEntry {
    pub id: String,
    file: glib::KeyFile,
}

pub struct DesktopAction {
    pub name: String,
    pub icon: String,
    pub command: Vec<String>,
}

impl DesktopEntry {
    pub fn string(&self, key: &str) -> Option<String> {
        self.file
            .string(GROUP, key)
            .ok()
            .map(|value| value.to_string())
    }

    fn localized(&self, group: &str, key: &str) -> String {
        self.file
            .locale_string(group, key, None)
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    fn flag(&self, key: &str) -> bool {
        self.file.boolean(GROUP, key).unwrap_or(false)
    }

    pub fn name(&self) -> String {
        self.localized(GROUP, "Name")
    }

    pub fn icon(&self) -> String {
        self.string("Icon").unwrap_or_default()
    }

    pub fn terminal(&self) -> bool {
        self.flag("Terminal")
    }

    pub fn shown(&self) -> bool {
        !self.flag("NoDisplay") && !self.flag("Hidden")
    }

    pub fn command(&self) -> Vec<String> {
        parse_exec(self.string("Exec").as_deref())
    }

    pub fn directory(&self) -> Option<String> {
        self.string("Path").filter(|path| !path.is_empty())
    }

    pub fn actions(&self) -> Vec<DesktopAction> {
        let names = self
            .file
            .string_list(GROUP, "Actions")
            .map(|list| list.iter().map(|name| name.to_string()).collect::<Vec<_>>())
            .unwrap_or_default();
        names
            .into_iter()
            .filter(|name| !name.is_empty())
            .filter_map(|name| {
                let group = format!("{ACTION_GROUP} {name}");
                if !self.file.has_group(&group) {
                    return None;
                }
                Some(DesktopAction {
                    name: self.localized(&group, "Name"),
                    icon: self
                        .file
                        .string(&group, "Icon")
                        .map(|value| value.to_string())
                        .unwrap_or_default(),
                    command: parse_exec(
                        self.file
                            .string(&group, "Exec")
                            .ok()
                            .as_ref()
                            .map(|value| value.as_str()),
                    ),
                })
            })
            .collect()
    }
}

fn parse_exec(line: Option<&str>) -> Vec<String> {
    line.and_then(|line| glib::shell_parse_argv(line).ok())
        .map(|argv| {
            argv.into_iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .filter(|argument| !argument.starts_with('%'))
                .collect()
        })
        .unwrap_or_default()
}

pub fn all() -> Vec<DesktopEntry> {
    let mut roots = vec![glib::user_data_dir()];
    roots.extend(glib::system_data_dirs());
    let mut entries: Vec<DesktopEntry> = Vec::new();
    for root in roots {
        let base = root.join("applications");
        let mut pending = vec![base.clone()];
        while let Some(directory) = pending.pop() {
            let Ok(listing) = std::fs::read_dir(&directory) else {
                continue;
            };
            for item in listing.flatten() {
                let path = item.path();
                if path.is_dir() {
                    pending.push(path);
                    continue;
                }
                if path.extension().and_then(|extension| extension.to_str()) != Some("desktop") {
                    continue;
                }
                let Some(id) = path
                    .strip_prefix(&base)
                    .ok()
                    .and_then(|relative| relative.to_str())
                    .map(|relative| relative.replace('/', "-"))
                else {
                    continue;
                };
                if entries.iter().any(|entry| entry.id == id) {
                    continue;
                }
                let file = glib::KeyFile::new();
                if file.load_from_file(&path, glib::KeyFileFlags::NONE).is_ok() {
                    entries.push(DesktopEntry { id, file });
                }
            }
        }
    }
    entries
}

pub fn find(id: &str) -> Option<DesktopEntry> {
    let entries = all();
    let wanted = format!("{}.desktop", id.to_lowercase());
    let class = |entry: &DesktopEntry| entry.string("StartupWMClass").unwrap_or_default();
    let position = entries
        .iter()
        .position(|entry| entry.id.to_lowercase() == wanted)
        .or_else(|| entries.iter().position(|entry| class(entry) == id))
        .or_else(|| {
            entries
                .iter()
                .position(|entry| class(entry).to_lowercase() == id.to_lowercase())
        })?;
    entries.into_iter().nth(position)
}

pub fn launch(entry: &DesktopEntry) {
    let command = entry.command();
    if entry.terminal() || command.is_empty() {
        let found = gio::AppInfo::all()
            .into_iter()
            .find(|info| info.id().as_deref() == Some(entry.id.as_str()));
        if let Some(info) = found {
            let _ = info.launch(&[], gio::AppLaunchContext::NONE);
        }
        return;
    }
    spawn(&command, entry.directory().as_deref(), Some(&entry.id));
}

pub fn launch_action(entry: &DesktopEntry, action: &DesktopAction) {
    if action.command.is_empty() {
        return;
    }
    spawn(&action.command, None, Some(&entry.id));
}

pub fn launch_in_terminal(entry: &DesktopEntry, line: &str) {
    spawn(&bash(line), None, Some(&entry.id));
}

pub fn shell(line: &str) {
    if line.is_empty() {
        return;
    }
    spawn(&bash(line), None, None);
}

fn bash(line: &str) -> [String; 3] {
    ["bash".to_owned(), "-c".to_owned(), line.to_owned()]
}

fn spawn(command: &[String], directory: Option<&str>, app: Option<&str>) {
    let unit = app.map(|id| format!("--unit={}", unit_name(id, &glib::uuid_string_random())));
    let mut argv: Vec<&std::ffi::OsStr> = SCOPE.iter().map(std::ffi::OsStr::new).collect();
    argv.extend(unit.as_deref().map(std::ffi::OsStr::new));
    argv.push(std::ffi::OsStr::new("--"));
    argv.extend(command.iter().map(std::ffi::OsStr::new));
    let launcher = crate::core::process::own_session(
        gio::SubprocessFlags::STDOUT_SILENCE | gio::SubprocessFlags::STDERR_SILENCE,
    );
    if let Some(directory) = directory {
        launcher.set_cwd(directory);
    }
    let _ = launcher.spawn(&argv);
}

fn unit_name(id: &str, uuid: &str) -> String {
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    let escaped: String = id
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b':' | b'_' | b'.' => {
                (byte as char).to_string()
            }
            _ => format!("\\x{byte:02x}"),
        })
        .collect();
    format!("app-{escaped}-{}.scope", uuid.replace('-', ""))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_unit_name_carries_the_escaped_desktop_id_as_plasma_reads_it() {
        assert_eq!(
            unit_name(
                "org.kde.plasma-systemmonitor.desktop",
                "0f8e6b2a-4c1d-4e5f-9a7b-3c2d1e0f9a8b"
            ),
            "app-org.kde.plasma\\x2dsystemmonitor-0f8e6b2a4c1d4e5f9a7b3c2d1e0f9a8b.scope"
        );
        assert_eq!(
            unit_name("kde4-kcalc ü.desktop", "1"),
            "app-kde4\\x2dkcalc\\x20\\xc3\\xbc-1.scope"
        );
    }
}
