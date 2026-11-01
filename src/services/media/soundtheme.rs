use gtk4::glib;
use std::path::PathBuf;

const FALLBACK_THEME: &str = "freedesktop";
const DEFAULT_DIRECTORIES: [&str; 1] = ["stereo"];
const EXTENSIONS: [&str; 4] = ["disabled", "oga", "ogg", "wav"];
const DISABLED: &str = "disabled";

struct Theme {
    name: String,
    roots: Vec<PathBuf>,
    directories: Vec<String>,
    parents: Vec<String>,
}

enum Lookup {
    Hit(PathBuf),
    Silence,
    Miss,
}

pub fn data_dirs() -> Vec<PathBuf> {
    std::iter::once(glib::user_data_dir())
        .chain(glib::system_data_dirs())
        .map(|dir| dir.join("sounds"))
        .collect()
}

pub fn find_sound(bases: &[PathBuf], theme: &str, name: &str) -> Option<PathBuf> {
    let mut chain = Vec::new();
    collect_themes(bases, theme, &mut chain);
    collect_themes(bases, FALLBACK_THEME, &mut chain);
    let mut candidate = name;
    loop {
        match chain
            .iter()
            .map(|theme| theme.find(candidate))
            .find(|lookup| !matches!(lookup, Lookup::Miss))
        {
            Some(Lookup::Hit(path)) => return Some(path),
            Some(_) => return None,
            None => candidate = candidate.rsplit_once('-')?.0,
        }
    }
}

fn collect_themes(bases: &[PathBuf], name: &str, chain: &mut Vec<Theme>) {
    if chain.iter().any(|theme| theme.name == name) {
        return;
    }
    let theme = Theme::load(bases, name);
    let parents = theme.parents.clone();
    chain.push(theme);
    for parent in parents {
        collect_themes(bases, &parent, chain);
    }
}

impl Theme {
    fn load(bases: &[PathBuf], name: &str) -> Theme {
        let roots: Vec<PathBuf> = bases
            .iter()
            .map(|base| base.join(name))
            .filter(|root| root.is_dir())
            .collect();
        let index = glib::KeyFile::new();
        index.set_list_separator(glib::Char::from(b','));
        let has_index = roots.iter().any(|root| {
            index
                .load_from_file(root.join("index.theme"), glib::KeyFileFlags::NONE)
                .is_ok()
        });
        let read_list = |key: &str| -> Vec<String> {
            if !has_index {
                return Vec::new();
            }
            index
                .string_list("Sound Theme", key)
                .map(|items| {
                    items
                        .iter()
                        .map(|item| item.trim().to_owned())
                        .filter(|item| !item.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let mut directories = read_list("Directories");
        if directories.is_empty() {
            directories = DEFAULT_DIRECTORIES.map(str::to_owned).to_vec();
        }
        Theme {
            name: name.to_owned(),
            roots,
            directories,
            parents: read_list("Inherits"),
        }
    }

    fn find(&self, name: &str) -> Lookup {
        for root in &self.roots {
            for directory in &self.directories {
                let folder = root.join(directory);
                for extension in EXTENSIONS {
                    let path = folder.join(format!("{name}.{extension}"));
                    if !path.is_file() {
                        continue;
                    }
                    return if extension == DISABLED {
                        Lookup::Silence
                    } else {
                        Lookup::Hit(path)
                    };
                }
            }
        }
        Lookup::Miss
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn touch(base: &Path, relative: &str) -> PathBuf {
        let path = base.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"sound").unwrap();
        path
    }

    #[test]
    fn a_sound_name_resolves_through_the_theme_its_parents_and_the_fallback() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-sound-theme-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        let user = folder.join("user");
        let system = folder.join("system");
        let index = touch(&system, "custom/index.theme");
        std::fs::write(
            index,
            "[Sound Theme]\nName=Custom\nDirectories=stereo,mono\nInherits=parent\n",
        )
        .unwrap();
        let custom_ding = touch(&system, "custom/stereo/ding.oga");
        let mono_beep = touch(&system, "custom/mono/beep.wav");
        let parent_chime = touch(&system, "parent/stereo/chime.ogg");
        let user_ding = touch(&user, "custom/stereo/ding.oga");
        touch(&system, "parent/stereo/ding.oga");
        let fallback_message = touch(&system, "freedesktop/stereo/message.oga");
        touch(&system, "custom/stereo/muted.oga");
        touch(&system, "custom/stereo/muted.disabled");
        let bases = [user.clone(), system.clone()];
        let cases = [
            ("custom", "ding", Some(user_ding)),
            ("custom", "beep", Some(mono_beep)),
            ("custom", "chime", Some(parent_chime)),
            ("custom", "message", Some(fallback_message.clone())),
            ("custom", "message-new-instant", Some(fallback_message)),
            ("custom", "muted", None),
            ("custom", "muted-extra", None),
            ("custom", "absent", None),
            ("missing", "absent", None),
        ];
        for (theme, name, expected) in cases {
            assert_eq!(find_sound(&bases, theme, name), expected, "{theme} {name}");
        }
        assert_eq!(find_sound(&bases[1..], "custom", "ding"), Some(custom_ding));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
