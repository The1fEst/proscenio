use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::errors::{describe_path_error, format_error};

const MAX_ENTRIES: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    Dir(String),
    File(String, Vec<u8>),
}

pub fn export(path: &str) -> Result<(), String> {
    let entries = collect_entries(Path::new(path))?;
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(&encode_entries(&entries))
        .and_then(|()| stdout.flush())
        .map_err(format_error)
}

fn collect_entries(path: &Path) -> Result<Vec<Entry>, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| describe_path_error(path, error))?;
    if metadata.is_file() {
        let bytes = fs::read(path).map_err(|error| describe_path_error(path, error))?;
        return Ok(vec![Entry::File(String::new(), bytes)]);
    }
    if !metadata.is_dir() {
        return Err(format!(
            "{} is neither a file nor a directory",
            path.display()
        ));
    }
    let mut entries = Vec::new();
    collect_dir(path, "", &mut entries)?;
    Ok(entries)
}

fn collect_dir(dir: &Path, prefix: &str, entries: &mut Vec<Entry>) -> Result<(), String> {
    let mut children: Vec<_> = fs::read_dir(dir)
        .and_then(|children| children.collect::<io::Result<Vec<_>>>())
        .map_err(|error| describe_path_error(dir, error))?;
    children.sort_by_key(|child| child.file_name());
    for child in children {
        let name = child.file_name().to_string_lossy().into_owned();
        let relative = if prefix.is_empty() {
            name
        } else {
            format!("{prefix}/{name}")
        };
        let path = child.path();
        let metadata =
            fs::symlink_metadata(&path).map_err(|error| describe_path_error(&path, error))?;
        if metadata.is_dir() {
            entries.push(Entry::Dir(relative.clone()));
            collect_dir(&path, &relative, entries)?;
        } else if metadata.is_file() {
            let bytes = fs::read(&path).map_err(|error| describe_path_error(&path, error))?;
            entries.push(Entry::File(relative, bytes));
        } else {
            return Err(format!(
                "{} is neither a file nor a directory",
                path.display()
            ));
        }
    }
    Ok(())
}

fn encode_entries(entries: &[Entry]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for entry in entries {
        let (tag, path, content) = match entry {
            Entry::Dir(path) => (b'd', path, None),
            Entry::File(path, content) => (b'f', path, Some(content)),
        };
        bytes.push(tag);
        bytes.extend_from_slice(&(path.len() as u32).to_le_bytes());
        bytes.extend_from_slice(path.as_bytes());
        if let Some(content) = content {
            bytes.extend_from_slice(&(content.len() as u64).to_le_bytes());
            bytes.extend_from_slice(content);
        }
    }
    bytes.push(b'e');
    bytes
}

pub fn decode_entries(mut bytes: &[u8]) -> Result<Vec<Entry>, String> {
    fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Result<&'a [u8], String> {
        if bytes.len() < count {
            return Err("the export ends early".to_owned());
        }
        let (head, rest) = bytes.split_at(count);
        *bytes = rest;
        Ok(head)
    }
    let mut entries = Vec::new();
    loop {
        let tag = take(&mut bytes, 1)?[0];
        if tag == b'e' {
            return if bytes.is_empty() {
                Ok(entries)
            } else {
                Err("the export continues after its end".to_owned())
            };
        }
        if entries.len() >= MAX_ENTRIES {
            return Err("the export has too many entries".to_owned());
        }
        let length = u32::from_le_bytes(take(&mut bytes, 4)?.try_into().unwrap()) as usize;
        let path = String::from_utf8(take(&mut bytes, length)?.to_vec())
            .map_err(|_| "an export path is not UTF-8".to_owned())?;
        match tag {
            b'd' => entries.push(Entry::Dir(path)),
            b'f' => {
                let size = u64::from_le_bytes(take(&mut bytes, 8)?.try_into().unwrap());
                let size = usize::try_from(size).map_err(format_error)?;
                entries.push(Entry::File(path, take(&mut bytes, size)?.to_vec()));
            }
            _ => return Err(format!("unknown export entry {tag}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn an_export_reads_back_as_it_was_written() {
        let entries = vec![
            Entry::Dir("include".to_owned()),
            Entry::File("include/a.h".to_owned(), b"#pragma once\n".to_vec()),
            Entry::File(String::new(), Vec::new()),
        ];
        assert_eq!(decode_entries(&encode_entries(&entries)), Ok(entries));
    }

    #[test]
    fn a_damaged_export_is_refused() {
        let bytes = encode_entries(&[Entry::File("a.h".to_owned(), b"abc".to_vec())]);
        assert!(decode_entries(&bytes[..bytes.len() - 2]).is_err());
        assert!(decode_entries(&[]).is_err());
        assert!(decode_entries(b"x").is_err());
        assert!(decode_entries(b"ee").is_err());
        assert!(decode_entries(&[b'f', 255, 255, 255, 255]).is_err());
        let mut huge = vec![b'f', 0, 0, 0, 0];
        huge.extend_from_slice(&u64::MAX.to_le_bytes());
        assert!(decode_entries(&huge).is_err());
    }

    #[test]
    fn a_stage_with_a_symbolic_link_is_not_exported() {
        let root = std::env::temp_dir().join(format!("proscenio-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("include/hyprland")).unwrap();
        fs::write(root.join("include/hyprland/a.h"), "a").unwrap();
        assert_eq!(
            collect_entries(&root),
            Ok(vec![
                Entry::Dir("include".to_owned()),
                Entry::Dir("include/hyprland".to_owned()),
                Entry::File("include/hyprland/a.h".to_owned(), b"a".to_vec()),
            ])
        );
        symlink("/etc/hostname", root.join("include/hyprland/b.h")).unwrap();
        assert!(collect_entries(&root).is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
