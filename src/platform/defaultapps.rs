use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gtk4::gio;
use gtk4::prelude::*;

const WEB: &[&str] = &[
    "x-scheme-handler/http",
    "x-scheme-handler/https",
    "text/html",
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
        if candidates.is_empty() {
            continue;
        }
        let default = defaults
            .get(mime)
            .unwrap_or(&empty)
            .iter()
            .find(|entry| known.contains_key(*entry))
            .cloned()
            .unwrap_or_else(|| candidates[0].clone());
        if !candidates.contains(&default) {
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

fn claimed<'a>(
    types: &[&'a str],
    declared: &[String],
    is_a: impl Fn(&str, &str) -> bool,
) -> Vec<&'a str> {
    types
        .iter()
        .enumerate()
        .filter(|(index, kind)| {
            *index == 0 || declared.iter().any(|supported| is_a(kind, supported))
        })
        .map(|(_, kind)| *kind)
        .collect()
}

pub fn set(types: &[&str], entry: &str) {
    let app = gio::AppInfo::all()
        .into_iter()
        .find(|app| app.id().is_some_and(|id| id == entry));
    let Some(app) = app else {
        return;
    };
    let declared: Vec<String> = app
        .supported_types()
        .into_iter()
        .map(String::from)
        .collect();
    for kind in claimed(types, &declared, gio::content_type_is_a) {
        let _ = app.set_as_default_for_type(kind);
    }
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
    fn a_role_claims_the_types_its_app_opens_and_always_its_first() {
        let declared = ["image/png".to_owned(), "image/x-bmp".to_owned()];
        let is_a = |kind: &str, supported: &str| {
            kind == supported || (kind == "image/bmp" && supported == "image/x-bmp")
        };
        assert_eq!(
            claimed(
                &["image/jpeg", "image/png", "image/webp", "image/bmp"],
                &declared,
                is_a
            ),
            ["image/jpeg", "image/png", "image/bmp"]
        );
    }
}
