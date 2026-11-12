use gtk4::glib;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const MAX_BYTES: u64 = 1024 * 1024;
const TIMESTAMP_FORMAT: &str = "%Y-%m-%dT%H:%M:%S%:z";

pub fn append_marker(path: &Path, text: &str) {
    write(path, &frame_marker(&format_now(), text));
}

pub fn append_report(path: &Path, text: &str) {
    write(path, &frame_report(&format_now(), text));
}

fn write(path: &Path, entry: &str) {
    if let Err(error) = append(path, entry, MAX_BYTES) {
        eprintln!("log: cannot write {}: {error}", path.display());
    }
}

fn format_now() -> String {
    glib::DateTime::now_local()
        .ok()
        .and_then(|now| now.format(TIMESTAMP_FORMAT).ok())
        .map_or_else(|| "unknown-time".to_owned(), Into::into)
}

fn frame_marker(timestamp: &str, text: &str) -> String {
    format!("{timestamp} {text}\n")
}

fn frame_report(timestamp: &str, text: &str) -> String {
    format!("\n{timestamp} {text}\n")
}

fn should_rotate(length: u64, max_bytes: u64) -> bool {
    length > max_bytes
}

fn rotated_path(path: &Path) -> PathBuf {
    let mut name: OsString = path.file_name().unwrap_or_default().to_owned();
    name.push(".1");
    path.with_file_name(name)
}

fn append(path: &Path, entry: &str, max_bytes: u64) -> io::Result<()> {
    if fs::metadata(path).is_ok_and(|metadata| should_rotate(metadata.len(), max_bytes)) {
        fs::rename(path, rotated_path(path))?;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    file.write_all(entry.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_folder(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("proscenio-logfile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn a_file_rotates_only_above_the_limit() {
        assert!(!should_rotate(MAX_BYTES, MAX_BYTES));
        assert!(should_rotate(MAX_BYTES + 1, MAX_BYTES));
    }

    #[test]
    fn the_rotated_name_keeps_the_file_name_and_adds_a_suffix() {
        assert_eq!(
            rotated_path(Path::new("/state/launcher-focus.log")),
            Path::new("/state/launcher-focus.log.1")
        );
    }

    #[test]
    fn markers_and_reports_are_separated_by_a_blank_line() {
        let folder = fresh_folder("framing");
        let path = folder.join("nested/launcher-focus.log");
        for entry in [
            frame_marker("2026-11-12T10:00:00+01:00", "proscenio started r1.abc"),
            frame_report(
                "2026-11-12T10:05:00+01:00",
                "overview focus lost: a\noverview focus lost: b",
            ),
            frame_report("2026-11-12T10:06:00+01:00", "overview focus lost: c"),
        ] {
            append(&path, &entry, MAX_BYTES).unwrap();
        }
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "2026-11-12T10:00:00+01:00 proscenio started r1.abc\n\
             \n\
             2026-11-12T10:05:00+01:00 overview focus lost: a\n\
             overview focus lost: b\n\
             \n\
             2026-11-12T10:06:00+01:00 overview focus lost: c\n"
        );
        fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_file_over_the_limit_moves_to_the_rotated_name_and_replaces_the_old_one() {
        let folder = fresh_folder("rotation");
        let path = folder.join("launcher-focus.log");
        let rotated = rotated_path(&path);
        for entry in ["first\n", "second\n", "third\n", "fourth\n", "fifth\n"] {
            append(&path, entry, 10).unwrap();
        }
        assert_eq!(fs::read_to_string(&rotated).unwrap(), "third\nfourth\n");
        assert_eq!(fs::read_to_string(&path).unwrap(), "fifth\n");
        fs::remove_dir_all(&folder).unwrap();
    }
}
