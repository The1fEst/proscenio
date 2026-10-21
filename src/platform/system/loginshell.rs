use gtk4::glib;
use std::path::{Path, PathBuf};

use crate::core::i18n::tr;
use crate::core::process;
use crate::platform::security::privileged;

const SHELLS: &str = "/etc/shells";
const NOT_INTERACTIVE: [&str; 4] = [
    "git-shell",
    "systemd-home-fallback-shell",
    "nologin",
    "rbash",
];
const PREFERRED_DIRECTORY: &str = "/usr/bin/";

pub struct Shell {
    pub path: String,
    pub name: String,
}

pub struct Choices {
    pub shells: Vec<Shell>,
    pub current_index: usize,
}

fn resolve_program(path: &str, root: &Path) -> Option<PathBuf> {
    let file = Path::new(path).file_name()?;
    let disk_path = root.join(path.trim_start_matches('/'));
    if !disk_path.is_file() {
        return None;
    }
    Some(disk_path.parent()?.canonicalize().ok()?.join(file))
}

pub fn parse(text: &str, root: &Path) -> Vec<Shell> {
    let mut programs: Vec<(PathBuf, Shell)> = Vec::new();
    for path in text.lines().map(str::trim) {
        if !path.starts_with('/') {
            continue;
        }
        let Some(name) = Path::new(path).file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if NOT_INTERACTIVE.contains(&name) {
            continue;
        }
        let Some(program) = resolve_program(path, root) else {
            continue;
        };
        match programs
            .iter_mut()
            .find(|(entry_program, _)| *entry_program == program)
        {
            Some((_, shell)) => {
                if path.starts_with(PREFERRED_DIRECTORY)
                    && !shell.path.starts_with(PREFERRED_DIRECTORY)
                {
                    shell.path = path.to_owned();
                }
            }
            None => programs.push((
                program,
                Shell {
                    path: path.to_owned(),
                    name: name.to_owned(),
                },
            )),
        }
    }
    programs.into_iter().map(|(_, shell)| shell).collect()
}

pub fn with_current(mut shells: Vec<Shell>, current: &str, root: &Path) -> Choices {
    let current_program = resolve_program(current, root);
    let position = shells.iter().position(|shell| {
        current_program.is_some() && resolve_program(&shell.path, root) == current_program
    });
    let current_index = position.unwrap_or_else(|| {
        shells.push(Shell {
            path: current.to_owned(),
            name: current.to_owned(),
        });
        shells.len() - 1
    });
    Choices {
        shells,
        current_index,
    }
}

pub fn parse_passwd_shell(entry: &str) -> Option<&str> {
    let shell = entry.lines().next()?.rsplit(':').next()?;
    Some(if shell.is_empty() { "/bin/sh" } else { shell })
}

pub fn list_installed() -> Vec<Shell> {
    let text = std::fs::read_to_string(SHELLS).unwrap_or_default();
    parse(&text, Path::new("/"))
}

pub async fn read_current() -> Option<String> {
    let user = glib::user_name().to_string_lossy().into_owned();
    let entry = process::capture_text(process::command(&["getent", "passwd", &user])).await?;
    parse_passwd_shell(&entry).map(str::to_owned)
}

pub async fn set(path: &str) -> Result<(), String> {
    let fallback_error = tr("The setting was not changed");
    if !list_installed().iter().any(|shell| shell.path == path) {
        return Err(fallback_error);
    }
    let user = glib::user_name().to_string_lossy().into_owned();
    privileged::execute(
        process::command(&["pkexec", "/usr/bin/chsh", "-s", path, &user]),
        fallback_error,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::symlink;
    use std::path::PathBuf;

    const ETC_SHELLS: &str = "# Pathnames of valid login shells.
# See shells(5) for details.

/bin/sh
/bin/bash
/bin/rbash
/usr/bin/sh
/usr/bin/bash
/usr/bin/rbash
/usr/bin/systemd-home-fallback-shell
/usr/bin/git-shell
/usr/bin/fish
/bin/fish
";

    struct Root(PathBuf);

    impl Root {
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Root {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn arch_root(name: &str) -> Root {
        let root = Root(std::env::temp_dir().join(format!(
            "proscenio-loginshell-{name}-{}",
            std::process::id()
        )));
        let _ = fs::remove_dir_all(root.path());
        let bin = root.path().join("usr/bin");
        fs::create_dir_all(&bin).unwrap();
        for program in ["bash", "fish", "systemd-home-fallback-shell", "git-shell"] {
            fs::write(bin.join(program), "").unwrap();
        }
        symlink("bash", bin.join("sh")).unwrap();
        symlink("bash", bin.join("rbash")).unwrap();
        symlink("usr/bin", root.path().join("bin")).unwrap();
        root
    }

    fn names_and_paths(shells: &[Shell]) -> Vec<(&str, &str)> {
        shells
            .iter()
            .map(|shell| (shell.name.as_str(), shell.path.as_str()))
            .collect()
    }

    #[test]
    fn a_shell_listed_under_bin_and_usr_bin_appears_once_with_the_usr_bin_path() {
        let root = arch_root("grouped");
        assert_eq!(
            names_and_paths(&parse(ETC_SHELLS, root.path())),
            [
                ("sh", "/usr/bin/sh"),
                ("bash", "/usr/bin/bash"),
                ("fish", "/usr/bin/fish"),
            ]
        );
    }

    #[test]
    fn a_shell_listed_only_under_bin_keeps_its_bin_path() {
        let root = arch_root("bin-only");
        assert_eq!(
            names_and_paths(&parse("/bin/fish\n", root.path())),
            [("fish", "/bin/fish")]
        );
    }

    #[test]
    fn a_listed_shell_that_is_not_installed_is_left_out() {
        let root = arch_root("missing");
        assert_eq!(
            names_and_paths(&parse("/usr/bin/zsh\n/usr/bin/fish\n", root.path())),
            [("fish", "/usr/bin/fish")]
        );
    }

    #[test]
    fn the_current_shell_is_selected_by_its_program_whichever_path_names_it() {
        let root = arch_root("selected");
        let shells = parse(ETC_SHELLS, root.path());
        assert_eq!(
            with_current(shells, "/bin/bash", root.path()).current_index,
            1
        );
    }

    #[test]
    fn a_current_shell_missing_from_the_list_is_added_and_selected() {
        let root = arch_root("unlisted");
        let shells = parse(ETC_SHELLS, root.path());
        let choices = with_current(shells, "/usr/local/bin/nu", root.path());
        assert_eq!(choices.current_index, 3);
        assert_eq!(choices.shells[3].path, "/usr/local/bin/nu");
        assert_eq!(choices.shells[3].name, "/usr/local/bin/nu");
    }

    #[test]
    fn the_login_shell_is_the_last_field_of_the_passwd_entry() {
        assert_eq!(
            parse_passwd_shell("fest:x:1000:1000:fEst:/home/fest:/usr/bin/bash\n"),
            Some("/usr/bin/bash")
        );
        assert_eq!(
            parse_passwd_shell("fest:x:1000:1000:fEst:/home/fest:"),
            Some("/bin/sh")
        );
        assert_eq!(
            parse_passwd_shell(
                "fest:x:1000:1000::/home/fest:/usr/bin/fish\nroot:x:0:0::/root:/bin/sh\n"
            ),
            Some("/usr/bin/fish")
        );
    }

    #[test]
    fn a_directory_listed_as_a_shell_is_left_out() {
        let root = arch_root("directory");
        assert_eq!(
            names_and_paths(&parse("/usr/bin/\n/usr/bin/fish\n", root.path())),
            [("fish", "/usr/bin/fish")]
        );
    }
}
