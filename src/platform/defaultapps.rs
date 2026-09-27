use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gtk4::gio;
use gtk4::prelude::*;

const DEFAULTS: &str = "Default Applications";
const ADDED: &str = "Added Associations";

const WEB: &[&str] = &[
    "x-scheme-handler/http",
    "x-scheme-handler/https",
    "text/html",
    "application/xhtml+xml",
];

const GROUPS: [(&str, &str); 4] = [
    ("image/", "photos"),
    ("audio/", "music"),
    ("video/", "video"),
    ("text/", "text"),
];

const MUSIC: &[&str] = &[
    "audio/mpeg",
    "audio/aac",
    "audio/mp4",
    "audio/mpegurl",
    "audio/ogg",
    "audio/vnd.rn-realaudio",
    "audio/vorbis",
    "audio/x-flac",
    "audio/x-mp3",
    "audio/x-mpegurl",
    "audio/x-ms-wma",
    "audio/x-musepack",
    "audio/x-oggflac",
    "audio/x-pn-realaudio",
    "audio/x-scpls",
    "audio/x-speex",
    "audio/x-vorbis",
    "audio/x-vorbis+ogg",
    "audio/x-wav",
];

const VIDEO: &[&str] = &[
    "video/mp4",
    "video/3gp",
    "video/3gpp",
    "video/3gpp2",
    "video/avi",
    "video/divx",
    "video/dv",
    "video/fli",
    "video/flv",
    "video/mp2t",
    "video/mp4v-es",
    "video/mpeg",
    "video/msvideo",
    "video/ogg",
    "video/quicktime",
    "video/vnd.divx",
    "video/vnd.mpegurl",
    "video/vnd.rn-realvideo",
    "video/webm",
    "video/x-avi",
    "video/x-flv",
    "video/x-m4v",
    "video/x-matroska",
    "video/x-mpeg2",
    "video/x-ms-asf",
    "video/x-msvideo",
    "video/x-ms-wmv",
    "video/x-ms-wmx",
    "video/x-ogm",
    "video/x-ogm+ogg",
    "video/x-theora",
    "video/x-theora+ogg",
    "application/x-matroska",
];

const PHOTOS: &[&str] = &[
    "image/png",
    "image/jpeg",
    "image/webp",
    "image/avif",
    "image/heif",
    "image/bmp",
    "image/x-icns",
];

const TEXT: &[&str] = &[
    "text/plain",
    "text/x-cmake",
    "text/markdown",
    "application/x-docbook+xml",
    "application/json",
    "application/x-yaml",
];

pub const ROLES: [(&str, &str, &[&str]); 8] = [
    ("web", "Web", WEB),
    ("mail", "Mail", &["x-scheme-handler/mailto"]),
    ("calendar", "Calendar", &["text/calendar"]),
    ("music", "Music", MUSIC),
    ("video", "Video", VIDEO),
    ("photos", "Photos", PHOTOS),
    ("text", "Text", TEXT),
    ("files", "Files", &["inode/directory"]),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub entry: String,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Role {
    pub key: &'static str,
    pub title: &'static str,
    pub types: &'static [&'static str],
    pub default: String,
    pub candidates: Vec<Candidate>,
}

type Sections = HashMap<String, Vec<(String, String)>>;

fn env_or(name: &str, fallback: impl FnOnce() -> String) -> String {
    std::env::var(name)
        .ok()
        .filter(|value| !value.is_empty())
        .unwrap_or_else(fallback)
}

fn home(path: &str) -> String {
    gtk4::glib::home_dir()
        .join(path)
        .to_string_lossy()
        .into_owned()
}

fn data_directories() -> Vec<PathBuf> {
    let data_home = env_or("XDG_DATA_HOME", || home(".local/share"));
    let data_dirs = env_or("XDG_DATA_DIRS", || "/usr/local/share:/usr/share".to_owned());
    std::iter::once(data_home)
        .chain(data_dirs.split(':').map(str::to_owned))
        .map(PathBuf::from)
        .collect()
}

fn application_directories() -> Vec<PathBuf> {
    data_directories()
        .into_iter()
        .map(|path| path.join("applications"))
        .collect()
}

fn association_files() -> Vec<PathBuf> {
    let config_home = env_or("XDG_CONFIG_HOME", || home(".config"));
    let config_dirs = env_or("XDG_CONFIG_DIRS", || "/etc/xdg".to_owned());
    let desktops: Vec<String> = std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .filter(|name| !name.is_empty())
        .map(str::to_lowercase)
        .collect();
    let mut paths = Vec::new();
    let directories = std::iter::once(PathBuf::from(config_home))
        .chain(config_dirs.split(':').map(PathBuf::from))
        .chain(application_directories());
    for directory in directories {
        for desktop in &desktops {
            paths.push(directory.join(format!("{desktop}-mimeapps.list")));
        }
        paths.push(directory.join("mimeapps.list"));
    }
    paths
}

pub fn parse_sections(text: &str) -> Sections {
    let mut found: Sections = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let name = line[1..line.len() - 1].to_owned();
            found.entry(name.clone()).or_default();
            current = Some(name);
            continue;
        }
        let (Some(section), Some((key, value))) = (current.as_ref(), line.split_once('=')) else {
            continue;
        };
        let pairs = found.entry(section.clone()).or_default();
        let key = key.trim();
        if !pairs.iter().any(|(known, _)| known == key) {
            pairs.push((key.to_owned(), value.trim().to_owned()));
        }
    }
    found
}

fn sections(path: &Path) -> Sections {
    std::fs::read(path)
        .map(|bytes| parse_sections(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default()
}

fn entries(value: &str) -> Vec<String> {
    value
        .split(';')
        .filter(|entry| !entry.is_empty())
        .map(str::to_owned)
        .collect()
}

fn installed() -> HashMap<String, PathBuf> {
    let mut known = HashMap::new();
    for directory in application_directories() {
        let Ok(listing) = std::fs::read_dir(&directory) else {
            continue;
        };
        let mut names: Vec<String> = listing
            .flatten()
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            if name.ends_with(".desktop") {
                known
                    .entry(name.clone())
                    .or_insert_with(|| directory.join(&name));
            }
        }
    }
    known
}

fn entry_name(entry: &str, path: &Path) -> String {
    let text = std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .unwrap_or_default();
    text.lines()
        .find_map(|line| line.strip_prefix("Name="))
        .map(|name| name.trim().to_owned())
        .unwrap_or_else(|| entry.trim_end_matches(".desktop").to_owned())
}

fn subclasses() -> HashMap<String, Vec<String>> {
    let mut parents: HashMap<String, Vec<String>> = HashMap::new();
    for directory in data_directories() {
        let Ok(bytes) = std::fs::read(directory.join("mime/subclasses")) else {
            continue;
        };
        for line in String::from_utf8_lossy(&bytes).lines() {
            let (child, parent) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
            if !child.is_empty() && !parent.is_empty() {
                parents
                    .entry(child.to_owned())
                    .or_default()
                    .push(parent.to_owned());
            }
        }
    }
    parents
}

fn family(mime: &str, parents: &HashMap<String, Vec<String>>) -> Vec<String> {
    let mut chain = vec![mime.to_owned()];
    let mut index = 0;
    while index < chain.len() {
        for parent in parents.get(&chain[index]).into_iter().flatten() {
            if !chain.contains(parent) {
                chain.push(parent.clone());
            }
        }
        index += 1;
    }
    chain
}

type Lists = HashMap<String, Vec<String>>;

fn gather(lists: &mut Lists, pairs: Option<&Vec<(String, String)>>) {
    for (mime, value) in pairs.into_iter().flatten() {
        lists
            .entry(mime.clone())
            .or_default()
            .extend(entries(value));
    }
}

pub fn read() -> Vec<Role> {
    let known = installed();
    let mut registered = Lists::new();
    for directory in application_directories() {
        gather(
            &mut registered,
            sections(&directory.join("mimeinfo.cache")).get("MIME Cache"),
        );
    }
    let (mut defaults, mut added, mut removed) = (Lists::new(), Lists::new(), Lists::new());
    for path in association_files() {
        let found = sections(&path);
        gather(&mut defaults, found.get("Default Applications"));
        gather(&mut removed, found.get("Removed Associations"));
        gather(&mut added, found.get("Added Associations"));
    }
    let parents = subclasses();
    let empty = Vec::new();
    let mut roles = Vec::new();
    for (key, title, types) in ROLES {
        let mime = types[0];
        let mut candidates: Vec<String> = Vec::new();
        for kind in family(mime, &parents) {
            let gone = removed.get(&kind).unwrap_or(&empty);
            let offered = [&defaults, &added, &registered]
                .into_iter()
                .flat_map(|lists| lists.get(&kind).unwrap_or(&empty).iter());
            for entry in offered {
                if known.contains_key(entry) && !gone.contains(entry) && !candidates.contains(entry)
                {
                    candidates.push(entry.clone());
                }
            }
        }
        let default = defaults
            .get(mime)
            .unwrap_or(&empty)
            .iter()
            .find(|entry| known.contains_key(*entry))
            .or(candidates.first())
            .cloned()
            .unwrap_or_default();
        if !default.is_empty() && !candidates.contains(&default) {
            candidates.insert(0, default.clone());
        }
        roles.push(Role {
            key,
            title,
            types,
            default,
            candidates: candidates
                .into_iter()
                .map(|entry| Candidate {
                    name: entry_name(&entry, &known[&entry]),
                    entry,
                })
                .collect(),
        });
    }
    roles
}

pub fn applications() -> Vec<gio::AppInfo> {
    let mut found: Vec<gio::AppInfo> = gio::AppInfo::all()
        .into_iter()
        .filter(|app| app.should_show() && app.id().is_some())
        .collect();
    found.sort_by_key(|app| app.display_name().to_lowercase());
    found.dedup_by(|one, other| one.id() == other.id());
    found
}

fn owner(kind: &str, parents: &HashMap<String, Vec<String>>) -> Option<&'static str> {
    if let Some((key, _, _)) = ROLES.iter().find(|(_, _, types)| types.contains(&kind)) {
        return Some(key);
    }
    if let Some((_, key)) = GROUPS.iter().find(|(prefix, _)| kind.starts_with(prefix)) {
        return Some(key);
    }
    family(kind, parents)
        .iter()
        .any(|parent| parent == "text/plain")
        .then_some("text")
}

fn scope(role: &str, all: &[String], parents: &HashMap<String, Vec<String>>) -> Vec<String> {
    let listed = ROLES
        .iter()
        .find(|(key, _, _)| *key == role)
        .map(|(_, _, types)| *types)
        .unwrap_or_default();
    let mut types: Vec<String> = listed.iter().map(|kind| (*kind).to_owned()).collect();
    for kind in all {
        if !types.contains(kind) && owner(kind, parents) == Some(role) {
            types.push(kind.clone());
        }
    }
    types
}

fn all_types() -> Vec<String> {
    let mut found = Vec::new();
    for directory in data_directories() {
        let Ok(bytes) = std::fs::read(directory.join("mime/types")) else {
            continue;
        };
        for line in String::from_utf8_lossy(&bytes).lines() {
            let kind = line.trim();
            if !kind.is_empty() && !found.iter().any(|known| known == kind) {
                found.push(kind.to_owned());
            }
        }
    }
    found
}

type Ordered = Vec<(String, Vec<(String, String)>)>;

fn parse_ordered(text: &str) -> Ordered {
    let mut found: Ordered = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            found.push((line[1..line.len() - 1].to_owned(), Vec::new()));
            continue;
        }
        let (Some((_, pairs)), Some((key, value))) = (found.last_mut(), line.split_once('='))
        else {
            continue;
        };
        let key = key.trim();
        if !pairs.iter().any(|(known, _)| known == key) {
            pairs.push((key.to_owned(), value.trim().to_owned()));
        }
    }
    found
}

fn render(sections: &Ordered) -> String {
    let mut text = String::new();
    for (index, (name, pairs)) in sections.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(&format!("[{name}]\n"));
        for (key, value) in pairs {
            text.push_str(&format!("{key}={value}\n"));
        }
    }
    text
}

fn section<'a>(sections: &'a mut Ordered, name: &str) -> &'a mut Vec<(String, String)> {
    let index = match sections.iter().position(|(known, _)| known == name) {
        Some(index) => index,
        None => {
            sections.push((name.to_owned(), Vec::new()));
            sections.len() - 1
        }
    };
    &mut sections[index].1
}

fn listed(entries: &[String]) -> String {
    format!("{};", entries.join(";"))
}

fn assigned(text: &str, types: &[String], entry: &str, known: impl Fn(&str) -> bool) -> String {
    let mut sections = parse_ordered(text);
    for (name, pairs) in &mut sections {
        if name != DEFAULTS && name != ADDED {
            continue;
        }
        pairs.retain_mut(|(_, value)| {
            let all = entries(value);
            let alive: Vec<String> = all.iter().filter(|id| known(id)).cloned().collect();
            if alive.len() != all.len() {
                *value = listed(&alive);
            }
            !alive.is_empty()
        });
    }
    let defaults = section(&mut sections, DEFAULTS);
    for kind in types {
        let value = listed(&[entry.to_owned()]);
        match defaults.iter_mut().find(|(known, _)| known == kind) {
            Some(pair) => pair.1 = value,
            None => defaults.push((kind.clone(), value)),
        }
    }
    let added = section(&mut sections, ADDED);
    for kind in types {
        match added.iter_mut().find(|(known, _)| known == kind) {
            Some(pair) => {
                let mut order = vec![entry.to_owned()];
                order.extend(entries(&pair.1).into_iter().filter(|other| other != entry));
                pair.1 = listed(&order);
            }
            None => added.push((kind.clone(), listed(&[entry.to_owned()]))),
        }
    }
    render(&sections)
}

pub fn set(role: &str, entry: &str) {
    let mut known: Vec<String> = installed().into_keys().collect();
    known.extend(
        gio::AppInfo::all()
            .into_iter()
            .filter_map(|app| app.id().map(String::from)),
    );
    if !known.iter().any(|id| id == entry) {
        return;
    }
    let types = scope(role, &all_types(), &subclasses());
    let config_home = env_or("XDG_CONFIG_HOME", || home(".config"));
    let path = PathBuf::from(config_home).join("mimeapps.list");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let _ = std::fs::write(
        &path,
        assigned(&text, &types, entry, |id| {
            known.iter().any(|known| known == id)
        }),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_value_of_a_key_in_a_section_wins() {
        let found = parse_sections(
            "# note\n[Default Applications]\ntext/plain=a.desktop;b.desktop;\ntext/plain = c.desktop\nstray\n[Added Associations]\nimage/png=d.desktop\n",
        );
        assert_eq!(
            found["Default Applications"],
            [("text/plain".to_owned(), "a.desktop;b.desktop;".to_owned())]
        );
        assert_eq!(
            entries(&found["Default Applications"][0].1),
            ["a.desktop", "b.desktop"]
        );
        assert_eq!(found["Added Associations"].len(), 1);
    }

    #[test]
    fn a_kind_offers_what_opens_its_parents_too() {
        let parents = HashMap::from([
            ("text/x-rust".to_owned(), vec!["text/plain".to_owned()]),
            (
                "text/plain".to_owned(),
                vec!["application/octet-stream".to_owned()],
            ),
        ]);
        assert_eq!(
            family("text/x-rust", &parents),
            ["text/x-rust", "text/plain", "application/octet-stream"]
        );
    }

    #[test]
    fn a_role_owns_its_group_and_leaves_other_roles_types_alone() {
        let parents = HashMap::from([
            ("application/json".to_owned(), vec!["text/plain".to_owned()]),
            (
                "image/svg+xml".to_owned(),
                vec!["application/xml".to_owned()],
            ),
            ("application/xml".to_owned(), vec!["text/plain".to_owned()]),
            ("text/html".to_owned(), vec!["text/plain".to_owned()]),
        ]);
        let all: Vec<String> = [
            "image/gif",
            "image/svg+xml",
            "text/x-python",
            "text/html",
            "text/calendar",
            "application/xml",
            "application/pdf",
            "audio/flac",
        ]
        .map(str::to_owned)
        .to_vec();
        let photos = scope("photos", &all, &parents);
        assert_eq!(photos[..PHOTOS.len()], *PHOTOS);
        assert_eq!(photos[PHOTOS.len()..], ["image/gif", "image/svg+xml"]);
        let text = scope("text", &all, &parents);
        assert_eq!(text[..TEXT.len()], *TEXT);
        assert_eq!(text[TEXT.len()..], ["text/x-python", "application/xml"]);
        assert!(scope("music", &all, &parents).contains(&"audio/flac".to_owned()));
        assert_eq!(owner("application/pdf", &parents), None);
    }

    #[test]
    fn assigning_a_role_overrides_its_types_and_drops_missing_apps() {
        let text = "[Added Associations]\nimage/png=old.desktop;gone.desktop;\n\n[Default Applications]\nimage/png=old.desktop\nimage/jpg=gone.desktop\ntext/plain=gone.desktop;old.desktop;\ntext/csv=old.desktop\n\n[Removed Associations]\nimage/gif=old.desktop;\n";
        let types = ["image/png".to_owned(), "image/gif".to_owned()];
        let known = |id: &str| id != "gone.desktop";
        assert_eq!(
            assigned(text, &types, "new.desktop", known),
            "[Added Associations]\nimage/png=new.desktop;old.desktop;\nimage/gif=new.desktop;\n\n[Default Applications]\nimage/png=new.desktop;\ntext/plain=old.desktop;\ntext/csv=old.desktop\nimage/gif=new.desktop;\n\n[Removed Associations]\nimage/gif=old.desktop;\n"
        );
        assert_eq!(
            assigned("", &types[..1], "new.desktop", known),
            "[Default Applications]\nimage/png=new.desktop;\n\n[Added Associations]\nimage/png=new.desktop;\n"
        );
    }
}
