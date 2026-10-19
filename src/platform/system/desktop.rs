use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;

use crate::core::process;

const GROUP: &str = "Desktop Entry";
const ACTION_GROUP: &str = "Desktop Action";

pub struct DesktopEntry {
    pub id: String,
    path: std::path::PathBuf,
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

    fn read_locale_string(&self, group: &str, key: &str) -> String {
        self.file
            .locale_string(group, key, None)
            .map(|value| value.to_string())
            .unwrap_or_default()
    }

    fn flag(&self, key: &str) -> bool {
        self.file.boolean(GROUP, key).unwrap_or(false)
    }

    pub fn name(&self) -> String {
        self.read_locale_string(GROUP, "Name")
    }

    pub fn icon(&self) -> String {
        self.string("Icon").unwrap_or_default()
    }

    pub fn runs_in_terminal(&self) -> bool {
        self.flag("Terminal")
    }

    pub fn is_shown(&self) -> bool {
        !self.flag("NoDisplay") && !self.flag("Hidden")
    }

    pub fn command(&self) -> Vec<String> {
        parse_exec(self.string("Exec").as_deref(), &self.fields())
    }

    fn fields(&self) -> Fields {
        Fields {
            icon: self.icon(),
            name: self.name(),
            location: self.path.to_string_lossy().into_owned(),
        }
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
        let fields = self.fields();
        names
            .into_iter()
            .filter(|name| !name.is_empty())
            .filter_map(|name| {
                let group = format!("{ACTION_GROUP} {name}");
                if !self.file.has_group(&group) {
                    return None;
                }
                Some(DesktopAction {
                    name: self.read_locale_string(&group, "Name"),
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
                        &fields,
                    ),
                })
            })
            .collect()
    }
}

type FileArgument = (Option<String>, String);

#[derive(Default)]
struct Fields {
    icon: String,
    name: String,
    location: String,
}

fn split_exec(line: Option<&str>) -> Vec<String> {
    line.and_then(|line| glib::shell_parse_argv(line).ok())
        .map(|argv| {
            argv.into_iter()
                .map(|argument| argument.to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default()
}

fn parse_exec(line: Option<&str>, fields: &Fields) -> Vec<String> {
    expand_exec(&split_exec(line), &[], fields)
        .pop()
        .unwrap_or_default()
}

fn expand_exec(argv: &[String], files: &[FileArgument], fields: &Fields) -> Vec<Vec<String>> {
    let fill = |files: &[FileArgument]| -> Vec<String> {
        argv.iter()
            .flat_map(|argument| match argument.as_str() {
                "%f" | "%F" => files.iter().filter_map(|(path, _)| path.clone()).collect(),
                "%u" | "%U" => files.iter().map(|(_, uri)| uri.clone()).collect(),
                "%i" if !fields.icon.is_empty() => vec!["--icon".to_owned(), fields.icon.clone()],
                other => {
                    let expansion = expand_argument(other, fields);
                    if expansion.is_empty() && !other.is_empty() {
                        Vec::new()
                    } else {
                        vec![expansion]
                    }
                }
            })
            .collect()
    };
    let contains_code = |code: &str| argv.iter().any(|argument| argument == code);
    if files.is_empty() || !(contains_code("%f") || contains_code("%u")) {
        return vec![fill(files)];
    }
    let commands: Vec<Vec<String>> = files
        .iter()
        .filter(|(path, _)| path.is_some() || !contains_code("%f"))
        .map(|file| fill(std::slice::from_ref(file)))
        .collect();
    if commands.is_empty() {
        return vec![fill(&[])];
    }
    commands
}

fn expand_argument(argument: &str, fields: &Fields) -> String {
    let mut text = String::new();
    let mut characters = argument.chars();
    while let Some(character) = characters.next() {
        if character != '%' {
            text.push(character);
            continue;
        }
        match characters.next() {
            Some('%') => text.push('%'),
            Some('c') => text.push_str(&fields.name),
            Some('k') => text.push_str(&fields.location),
            _ => {}
        }
    }
    text
}

pub fn all() -> Vec<DesktopEntry> {
    let mut roots = vec![glib::user_data_dir()];
    roots.extend(glib::system_data_dirs());
    let mut entries: Vec<DesktopEntry> = Vec::new();
    for root in roots {
        let base = root.join("applications");
        let mut pending_directories = vec![base.clone()];
        while let Some(directory) = pending_directories.pop() {
            let Ok(listing) = std::fs::read_dir(&directory) else {
                continue;
            };
            for item in listing.flatten() {
                let path = item.path();
                if path.is_dir() {
                    pending_directories.push(path);
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
                    entries.push(DesktopEntry { id, path, file });
                }
            }
        }
    }
    entries
}

pub fn load(path: &std::path::Path) -> Option<DesktopEntry> {
    let id = path.file_name()?.to_str()?.to_owned();
    let file = glib::KeyFile::new();
    file.load_from_file(path, glib::KeyFileFlags::NONE).ok()?;
    Some(DesktopEntry {
        id,
        path: path.to_owned(),
        file,
    })
}

pub fn find(id: &str) -> Option<DesktopEntry> {
    let entries = all();
    let desktop_id = format!("{}.desktop", id.to_lowercase());
    let class = |entry: &DesktopEntry| entry.string("StartupWMClass").unwrap_or_default();
    let position = entries
        .iter()
        .position(|entry| entry.id.to_lowercase() == desktop_id)
        .or_else(|| entries.iter().position(|entry| class(entry) == id))
        .or_else(|| {
            entries
                .iter()
                .position(|entry| class(entry).to_lowercase() == id.to_lowercase())
        })?;
    entries.into_iter().nth(position)
}

pub fn launch(entry: &DesktopEntry) {
    launch_with(entry, &[]);
}

pub fn launch_with(entry: &DesktopEntry, files: &[gio::File]) {
    if entry.runs_in_terminal() || entry.command().is_empty() {
        let app_info = gio::AppInfo::all()
            .into_iter()
            .find(|info| info.id().as_deref() == Some(entry.id.as_str()));
        if let Some(info) = app_info {
            let _ = info.launch(files, gio::AppLaunchContext::NONE);
        }
        return;
    }
    let file_arguments: Vec<FileArgument> = files
        .iter()
        .map(|file| {
            (
                file.path().map(|path| path.to_string_lossy().into_owned()),
                file.uri().to_string(),
            )
        })
        .collect();
    let argv = split_exec(entry.string("Exec").as_deref());
    for command in expand_exec(&argv, &file_arguments, &entry.fields()) {
        spawn(&command, entry.directory().as_deref(), Some(&entry.id));
    }
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

pub fn run_shell(line: &str) {
    if line.is_empty() {
        return;
    }
    spawn(&bash(line), None, None);
}

fn bash(line: &str) -> [String; 3] {
    ["bash".to_owned(), "-c".to_owned(), line.to_owned()]
}

fn spawn(command: &[String], directory: Option<&str>, app: Option<&str>) {
    let unit = app.map(|id| unit_name(id, &glib::uuid_string_random()));
    let mut scope_command = process::build_app_command(unit.as_deref(), command);
    if let Some(directory) = directory {
        scope_command.current_dir(directory);
    }
    process::start(scope_command);
}

fn unit_name(id: &str, uuid: &str) -> String {
    let id = id.strip_suffix(".desktop").unwrap_or(id);
    let escaped_id: String = id
        .bytes()
        .map(|byte| match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b':' | b'_' | b'.' => {
                (byte as char).to_string()
            }
            _ => format!("\\x{byte:02x}"),
        })
        .collect();
    format!("app-{escaped_id}-{}.scope", uuid.replace('-', ""))
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

    #[test]
    fn exec_field_codes_take_the_dropped_files_one_launch_per_file_for_single_codes() {
        let argv = |line: &str| split_exec(Some(line));
        let to_strings = |items: &[&str]| items.iter().map(|item| item.to_string()).collect();
        let files = [
            (
                Some("/home/a b.txt".to_owned()),
                "file:///home/a%20b.txt".to_owned(),
            ),
            (None, "https://example.org/c".to_owned()),
        ];
        let cases: [(&str, Vec<Vec<String>>); 5] = [
            (
                "dolphin %u",
                vec![
                    to_strings(&["dolphin", "file:///home/a%20b.txt"]),
                    to_strings(&["dolphin", "https://example.org/c"]),
                ],
            ),
            (
                "app --new %U",
                vec![to_strings(&[
                    "app",
                    "--new",
                    "file:///home/a%20b.txt",
                    "https://example.org/c",
                ])],
            ),
            ("viewer %F", vec![to_strings(&["viewer", "/home/a b.txt"])]),
            ("editor %f", vec![to_strings(&["editor", "/home/a b.txt"])]),
            ("plain %i", vec![to_strings(&["plain"])]),
        ];
        let empty_fields = Fields::default();
        for (line, expected) in cases {
            assert_eq!(
                expand_exec(&argv(line), &files, &empty_fields),
                expected,
                "{line}"
            );
        }
        assert_eq!(
            expand_exec(&argv("dolphin %u"), &[], &empty_fields),
            vec![to_strings(&["dolphin"])]
        );
        assert_eq!(
            expand_exec(&argv("editor %f"), &files[1..], &empty_fields),
            vec![to_strings(&["editor"])]
        );
    }

    #[test]
    fn exec_field_codes_expand_from_the_entry_and_inside_arguments() {
        let fields = Fields {
            icon: "editor-icon".to_owned(),
            name: "Text Editor".to_owned(),
            location: "/usr/share/applications/editor.desktop".to_owned(),
        };
        let cases = [
            (
                "editor %i --class=%c %k",
                vec![
                    "editor",
                    "--icon",
                    "editor-icon",
                    "--class=Text Editor",
                    "/usr/share/applications/editor.desktop",
                ],
            ),
            (
                "sh -c 'printf 100%%' %d --x=%f%z",
                vec!["sh", "-c", "printf 100%", "--x="],
            ),
        ];
        for (line, expected) in cases {
            assert_eq!(
                parse_exec(Some(line), &fields),
                expected
                    .iter()
                    .map(|item| item.to_string())
                    .collect::<Vec<_>>(),
                "{line}"
            );
        }
        assert_eq!(
            parse_exec(Some("editor %i %c"), &Fields::default()),
            ["editor"]
        );
    }
}
