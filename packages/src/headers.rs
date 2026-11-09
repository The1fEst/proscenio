use std::collections::HashSet;
use std::fs;
use std::path::Path;

use crate::errors::describe_path_error;
use crate::export::Entry;
use crate::files::{create_dir, create_dirs, is_name, remove_dir, write_new_file};

const NEW_HEADERS_DIR: &str = ".proscenio-new";
const HEADER_MODE: u32 = 0o644;

fn is_header_entry(entry: &Entry) -> bool {
    let (path, is_dir) = match entry {
        Entry::Dir(path) => (path, true),
        Entry::File(path, _) => (path, false),
    };
    let parts: Vec<&str> = path.split('/').collect();
    if !parts.iter().all(|part| is_name(part)) {
        return false;
    }
    match parts[..] {
        ["include"] | ["include", "hyprland"] | ["share"] | ["share", "pkgconfig"] => is_dir,
        ["share", "pkgconfig", "hyprland.pc"] => !is_dir,
        ["include", "hyprland", _, ..] => true,
        _ => false,
    }
}

pub fn replace_headers(headers_dir: &Path, entries: &[Entry]) -> Result<(), String> {
    if let Some(entry) = entries.iter().find(|entry| !is_header_entry(entry)) {
        return Err(format!("unexpected header entry {entry:?}"));
    }
    let has_package_config = entries.iter().any(
        |entry| matches!(entry, Entry::File(path, _) if path == "share/pkgconfig/hyprland.pc"),
    );
    let has_include_dir = entries
        .iter()
        .any(|entry| matches!(entry, Entry::Dir(path) if path == "include/hyprland"));
    if !has_package_config || !has_include_dir {
        return Err("the headers have no include/hyprland or hyprland.pc".to_owned());
    }
    let dirs: HashSet<&str> = entries
        .iter()
        .filter_map(|entry| match entry {
            Entry::Dir(path) => Some(path.as_str()),
            Entry::File(..) => None,
        })
        .collect();
    let orphan = entries.iter().find(|entry| match entry {
        Entry::File(path, _) => path
            .rsplit_once('/')
            .is_none_or(|(parent, _)| !dirs.contains(parent)),
        Entry::Dir(path) => path
            .rsplit_once('/')
            .is_some_and(|(parent, _)| !dirs.contains(parent)),
    });
    if let Some(entry) = orphan {
        return Err(format!("header entry without its directory: {entry:?}"));
    }
    create_dirs(headers_dir)?;
    let new_headers_dir = headers_dir.join(NEW_HEADERS_DIR);
    remove_dir(&new_headers_dir)?;
    create_dir(&new_headers_dir)?;
    let result = write_entries(&new_headers_dir, entries)
        .and_then(|()| swap_headers(headers_dir, &new_headers_dir));
    let _ = remove_dir(&new_headers_dir);
    result
}

fn write_entries(dir: &Path, entries: &[Entry]) -> Result<(), String> {
    entries.iter().try_for_each(|entry| match entry {
        Entry::Dir(path) => create_dir(&dir.join(path)),
        Entry::File(path, bytes) => write_new_file(&dir.join(path), bytes, HEADER_MODE),
    })
}

fn swap_headers(headers_dir: &Path, new_headers_dir: &Path) -> Result<(), String> {
    for (parent, name) in [("include", "hyprland"), ("share/pkgconfig", "hyprland.pc")] {
        let target = headers_dir.join(parent).join(name);
        create_dirs(&headers_dir.join(parent))?;
        remove_dir(&target)?;
        fs::rename(new_headers_dir.join(parent).join(name), &target)
            .map_err(|error| describe_path_error(&target, error))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn only_the_headers_layout_is_accepted_from_the_stage() {
        let file = |path: &str| Entry::File(path.to_owned(), Vec::new());
        let dir = |path: &str| Entry::Dir(path.to_owned());
        for entry in [
            dir("include"),
            dir("include/hyprland"),
            dir("include/hyprland/src"),
            file("include/hyprland/src/version.h"),
            file("include/hyprland/protocols/xdg-shell.hpp"),
            dir("share"),
            dir("share/pkgconfig"),
            file("share/pkgconfig/hyprland.pc"),
        ] {
            assert!(is_header_entry(&entry), "{entry:?}");
        }
        for entry in [
            file("include"),
            file("include/hyprland"),
            file("include/other.h"),
            dir("include/hyprland/../../../etc"),
            file("include/hyprland/./a.h"),
            file("include//hyprland/a.h"),
            file("/etc/passwd"),
            file(""),
            dir(""),
            file("share/pkgconfig/other.pc"),
            dir("share/pkgconfig/hyprland.pc"),
            dir("lib"),
            file("share/hyprland.pc"),
        ] {
            assert!(!is_header_entry(&entry), "{entry:?}");
        }
    }

    #[test]
    fn the_headers_replace_the_old_ones_with_fixed_modes() {
        let root = std::env::temp_dir().join(format!("proscenio-headers-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let headers_dir = root.join("headersRoot");
        let elsewhere = root.join("elsewhere");
        fs::create_dir_all(headers_dir.join("include")).unwrap();
        fs::create_dir_all(&elsewhere).unwrap();
        fs::write(elsewhere.join("kept.h"), "").unwrap();
        symlink(&elsewhere, headers_dir.join("include/hyprland")).unwrap();
        let entries = vec![
            Entry::Dir("include".to_owned()),
            Entry::Dir("include/hyprland".to_owned()),
            Entry::File("include/hyprland/a.h".to_owned(), b"a".to_vec()),
            Entry::Dir("share".to_owned()),
            Entry::Dir("share/pkgconfig".to_owned()),
            Entry::File("share/pkgconfig/hyprland.pc".to_owned(), b"pc".to_vec()),
        ];

        replace_headers(&headers_dir, &entries).unwrap();

        assert!(elsewhere.join("kept.h").exists());
        let header = headers_dir.join("include/hyprland/a.h");
        assert_eq!(fs::read(&header).unwrap(), b"a");
        let mode = |path: &Path| fs::symlink_metadata(path).unwrap().permissions().mode() & 0o7777;
        assert_eq!(mode(&header), 0o644);
        assert_eq!(mode(&headers_dir.join("include/hyprland")), 0o755);
        assert!(replace_headers(&headers_dir, &entries[..3]).is_err());
        let mut orphan_entries = entries.clone();
        orphan_entries.push(Entry::File(
            "include/hyprland/missing/b.h".to_owned(),
            Vec::new(),
        ));
        assert!(replace_headers(&headers_dir, &orphan_entries).is_err());
        assert!(header.exists());
        let empty_headers_dir = root.join("empty");
        fs::create_dir_all(&empty_headers_dir).unwrap();
        replace_headers(&empty_headers_dir, &entries).unwrap();
        assert!(
            empty_headers_dir
                .join("share/pkgconfig/hyprland.pc")
                .is_file()
        );
        let hostile_entries = vec![Entry::File(
            "include/hyprland/../../x".to_owned(),
            Vec::new(),
        )];
        assert!(replace_headers(&headers_dir, &hostile_entries).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
