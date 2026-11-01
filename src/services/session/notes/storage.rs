use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

pub const MAX_NOTE_BYTES: u64 = 2 * 1024 * 1024;
pub const MAX_TITLE_CHARS: usize = 60;
const MAX_INDEX_BYTES: u64 = 1024 * 1024;
const MAX_ID: u64 = u32::MAX as u64;
const INDEX_NAME: &str = "index.json";
const NOTE_EXTENSION: &str = ".txt";

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub id: u64,
    pub custom_title: Option<String>,
    pub derived_title: Option<String>,
    pub cursor: usize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Index {
    pub active_id: u64,
    pub next_id: u64,
    pub entries: Vec<Entry>,
}

#[derive(Debug, PartialEq)]
pub enum Content {
    Text(String),
    Absence,
    Overflow,
    Failure,
}

pub fn directory() -> PathBuf {
    crate::core::paths::state().join("notes")
}

pub fn index_path(directory: &Path) -> PathBuf {
    directory.join(INDEX_NAME)
}

pub fn note_path(directory: &Path, id: u64) -> PathBuf {
    directory.join(note_file_name(id))
}

fn note_file_name(id: u64) -> String {
    format!("{id}{NOTE_EXTENSION}")
}

fn parse_note_file_name(name: &str) -> Option<u64> {
    let id: u64 = name.strip_suffix(NOTE_EXTENSION)?.parse().ok()?;
    (id <= MAX_ID && note_file_name(id) == name).then_some(id)
}

pub fn clamp_title(title: &str) -> String {
    title
        .trim()
        .chars()
        .take(MAX_TITLE_CHARS)
        .collect::<String>()
        .trim_end()
        .to_owned()
}

pub fn derive_title(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(clamp_title)
}

pub fn serialize_index(index: &Index) -> String {
    let notes: Vec<Value> = index
        .entries
        .iter()
        .map(|entry| {
            let mut note = json!({ "id": entry.id, "cursor": entry.cursor });
            if let Some(title) = &entry.custom_title {
                note["title"] = Value::from(title.as_str());
            }
            if let Some(title) = &entry.derived_title {
                note["derived"] = Value::from(title.as_str());
            }
            note
        })
        .collect();
    let root = json!({ "active": index.active_id, "next": index.next_id, "notes": notes });
    serde_json::to_string_pretty(&root).unwrap_or_default() + "\n"
}

pub fn parse_index(text: &str) -> Option<Index> {
    let root: Value = serde_json::from_str(text).ok()?;
    let mut listed_ids = HashSet::new();
    let entries: Vec<Entry> = root
        .get("notes")?
        .as_array()?
        .iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_u64().filter(|id| *id <= MAX_ID)?;
            if !listed_ids.insert(id) {
                return None;
            }
            let title = |key: &str| {
                item.get(key)
                    .and_then(Value::as_str)
                    .map(clamp_title)
                    .filter(|title| !title.is_empty())
            };
            Some(Entry {
                id,
                custom_title: title("title"),
                derived_title: title("derived"),
                cursor: item.get("cursor").and_then(Value::as_u64).unwrap_or(0) as usize,
            })
        })
        .collect();
    let first_free_id = entries
        .iter()
        .map(|entry| entry.id.saturating_add(1))
        .max()
        .unwrap_or(0);
    let active_id = root
        .get("active")
        .and_then(Value::as_u64)
        .filter(|id| listed_ids.contains(id))
        .or_else(|| entries.first().map(|entry| entry.id))
        .unwrap_or(0);
    Some(Index {
        active_id,
        next_id: root
            .get("next")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(MAX_ID)
            .max(first_free_id),
        entries,
    })
}

pub fn load_index(directory: &Path) -> Index {
    let mut index = read_limited(&index_path(directory), MAX_INDEX_BYTES)
        .ok()
        .flatten()
        .and_then(|bytes| parse_index(&String::from_utf8_lossy(&bytes)))
        .unwrap_or_default();
    adopt_orphans(directory, &mut index);
    index
}

fn adopt_orphans(directory: &Path, index: &mut Index) {
    let known: HashSet<u64> = index.entries.iter().map(|entry| entry.id).collect();
    let mut orphans: Vec<u64> = std::fs::read_dir(directory)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|item| parse_note_file_name(item.file_name().to_str()?))
        .filter(|id| !known.contains(id))
        .collect();
    orphans.sort_unstable();
    for id in orphans {
        index.next_id = index.next_id.max(id.saturating_add(1));
        index.entries.push(Entry {
            id,
            custom_title: None,
            derived_title: None,
            cursor: 0,
        });
    }
    if !index
        .entries
        .iter()
        .any(|entry| entry.id == index.active_id)
    {
        index.active_id = index.entries.first().map_or(0, |entry| entry.id);
    }
}

pub fn read_note(directory: &Path, id: u64) -> Content {
    match read_limited(&note_path(directory, id), MAX_NOTE_BYTES) {
        Ok(Some(bytes)) if bytes.contains(&0) => Content::Failure,
        Ok(Some(bytes)) => String::from_utf8(bytes).map_or(Content::Failure, Content::Text),
        Ok(None) => Content::Overflow,
        Err(error) if error.kind() == io::ErrorKind::NotFound => Content::Absence,
        Err(_) => Content::Failure,
    }
}

fn read_limited(path: &Path, limit: u64) -> io::Result<Option<Vec<u8>>> {
    let mut bytes = Vec::new();
    File::open(path)?.take(limit + 1).read_to_end(&mut bytes)?;
    Ok((bytes.len() as u64 <= limit).then_some(bytes))
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(io::ErrorKind::InvalidInput.into());
    };
    std::fs::create_dir_all(parent)?;
    let mut staging_name = name.to_os_string();
    staging_name.push(".tmp");
    let staging_path = path.with_file_name(staging_name);
    let result =
        write_synced(&staging_path, bytes).and_then(|()| std::fs::rename(&staging_path, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&staging_path);
    }
    result
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_scratch_directory(name: &str) -> PathBuf {
        let folder =
            std::env::temp_dir().join(format!("proscenio-notes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    fn make_entry(
        id: u64,
        custom_title: Option<&str>,
        derived_title: Option<&str>,
        cursor: usize,
    ) -> Entry {
        Entry {
            id,
            custom_title: custom_title.map(str::to_owned),
            derived_title: derived_title.map(str::to_owned),
            cursor,
        }
    }

    #[test]
    fn the_title_is_the_first_line_with_text_cut_to_the_limit() {
        let long = "x".repeat(MAX_TITLE_CHARS + 20);
        let cases = [
            ("Shopping\nmilk", Some("Shopping")),
            ("\n  \n   Padded title  \nsecond", Some("Padded title")),
            ("", None),
            (" \n\t\n", None),
            (long.as_str(), Some(&long[..MAX_TITLE_CHARS])),
            ("日本語のメモ\n二行目", Some("日本語のメモ")),
        ];
        for (text, title) in cases {
            assert_eq!(derive_title(text).as_deref(), title, "{text:?}");
        }
        let spaced = format!("{}{}tail", "a".repeat(MAX_TITLE_CHARS - 1), " ");
        assert_eq!(derive_title(&spaced), Some("a".repeat(MAX_TITLE_CHARS - 1)));
    }

    #[test]
    fn the_index_survives_a_round_trip() {
        let index = Index {
            active_id: 4,
            next_id: 9,
            entries: vec![
                make_entry(2, None, Some("Groceries"), 14),
                make_entry(4, Some("Plans \"2027\""), Some("Plans"), 0),
                make_entry(8, None, None, 3),
            ],
        };
        assert_eq!(parse_index(&serialize_index(&index)), Some(index));
    }

    #[test]
    fn an_index_is_repaired_where_it_contradicts_itself() {
        let index = parse_index(
            r#"{"active": 77, "next": 1, "notes": [
                {"id": 5, "cursor": "far"}, {"id": 5, "title": "again"}, {"id": 6, "title": ""}, {"title": "no id"}, 3
            ]}"#,
        )
        .unwrap();
        assert_eq!(
            index,
            Index {
                active_id: 5,
                next_id: 7,
                entries: vec![make_entry(5, None, None, 0), make_entry(6, None, None, 0)],
            }
        );
    }

    #[test]
    fn ids_too_large_to_count_on_are_ignored() {
        let folder = create_scratch_directory("ids");
        let index = parse_index(
            r#"{"active": 1, "next": 18446744073709551615, "notes": [
                {"id": 1}, {"id": 18446744073709551615}
            ]}"#,
        )
        .unwrap();
        assert_eq!(index.entries, vec![make_entry(1, None, None, 0)]);
        assert_eq!(index.next_id, MAX_ID);
        std::fs::write(folder.join("18446744073709551615.txt"), "text").unwrap();
        assert_eq!(load_index(&folder), Index::default());
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn text_that_is_not_an_index_is_refused() {
        for text in ["", "not json", "[]", r#"{"notes": 3}"#, r#"{"active": 1}"#] {
            assert_eq!(parse_index(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_missing_or_broken_index_is_rebuilt_from_the_note_files() {
        let folder = create_scratch_directory("recovery");
        assert_eq!(load_index(&folder), Index::default());
        for name in [
            "3.txt",
            "11.txt",
            "7.txt",
            "007.txt",
            "notes.txt",
            "4.txt.tmp",
            "index.json.tmp",
        ] {
            std::fs::write(folder.join(name), "text").unwrap();
        }
        let expected = Index {
            active_id: 3,
            next_id: 12,
            entries: vec![
                make_entry(3, None, None, 0),
                make_entry(7, None, None, 0),
                make_entry(11, None, None, 0),
            ],
        };
        assert_eq!(load_index(&folder), expected);
        std::fs::write(index_path(&folder), "{ broken").unwrap();
        assert_eq!(load_index(&folder), expected);
        std::fs::write(
            index_path(&folder),
            vec![b' '; MAX_INDEX_BYTES as usize + 1],
        )
        .unwrap();
        assert_eq!(load_index(&folder), expected);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_note_file_the_index_does_not_list_is_kept() {
        let folder = create_scratch_directory("orphan");
        let listed = Index {
            active_id: 2,
            next_id: 3,
            entries: vec![make_entry(2, Some("Kept"), None, 5)],
        };
        write_atomic(&index_path(&folder), serialize_index(&listed).as_bytes()).unwrap();
        std::fs::write(folder.join("9.txt"), "stray").unwrap();
        let index = load_index(&folder);
        assert_eq!(index.active_id, 2);
        assert_eq!(index.next_id, 10);
        assert_eq!(
            index.entries,
            vec![
                make_entry(2, Some("Kept"), None, 5),
                make_entry(9, None, None, 0)
            ]
        );
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_note_larger_than_the_limit_is_not_read() {
        let folder = create_scratch_directory("limit");
        let cases = [
            (MAX_NOTE_BYTES as usize, true),
            (MAX_NOTE_BYTES as usize + 1, false),
        ];
        for (index, (size, is_read)) in cases.into_iter().enumerate() {
            std::fs::write(note_path(&folder, index as u64), vec![b'a'; size]).unwrap();
            let content = read_note(&folder, index as u64);
            assert_eq!(matches!(content, Content::Text(_)), is_read, "{size}");
            assert_eq!(content == Content::Overflow, !is_read, "{size}");
        }
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_note_reads_back_as_written_and_odd_files_are_told_apart() {
        let folder = create_scratch_directory("read");
        write_atomic(&note_path(&folder, 1), "строка\n日本語".as_bytes()).unwrap();
        assert_eq!(
            read_note(&folder, 1),
            Content::Text("строка\n日本語".to_owned())
        );
        assert_eq!(read_note(&folder, 2), Content::Absence);
        std::fs::write(note_path(&folder, 3), [0xff, 0xfe, b'a']).unwrap();
        assert_eq!(read_note(&folder, 3), Content::Failure);
        std::fs::write(note_path(&folder, 5), b"before\0after").unwrap();
        assert_eq!(read_note(&folder, 5), Content::Failure);
        std::fs::create_dir(note_path(&folder, 4)).unwrap();
        assert_eq!(read_note(&folder, 4), Content::Failure);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_write_replaces_the_file_whole_and_leaves_no_staging_file() {
        let folder = create_scratch_directory("atomic");
        let path = folder.join("nested/1.txt");
        write_atomic(&path, b"first version, the longer one").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"second");
        let names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
            .unwrap()
            .flatten()
            .map(|item| item.file_name())
            .collect();
        assert_eq!(names, ["1.txt"]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_failed_write_keeps_the_old_file_and_cleans_up() {
        let folder = create_scratch_directory("failed");
        let path = folder.join("1.txt");
        write_atomic(&path, b"kept").unwrap();
        std::fs::create_dir(folder.join("2.txt")).unwrap();
        assert!(write_atomic(&folder.join("2.txt"), b"lost").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"kept");
        assert!(!folder.join("2.txt.tmp").exists());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
