use alpm::{Alpm, PackageFrom};
use proscenio_protocol::Event;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use crate::errors::format_error;
use crate::output::emit;
use crate::private_database::{O_NOFOLLOW, O_NONBLOCK};

const ROOT_UID: u32 = 0;
const PART_SUFFIX: &str = ".part";
const SIGNATURE_PART_SUFFIX: &str = ".sig.part";
const COMPRESSION_MAGICS: [(&str, &[u8]); 3] = [
    (".zst", &[0x28, 0xB5, 0x2F, 0xFD]),
    (".xz", &[0xFD, 0x37, 0x7A, 0x58, 0x5A, 0x00]),
    (".gz", &[0x1F, 0x8B]),
];

#[derive(Debug, PartialEq)]
pub struct PackageFile {
    pub filename: String,
    pub size_bytes: u64,
}

#[derive(Debug, PartialEq)]
pub enum PartOutcome {
    Removed(PathBuf),
    Skipped(PathBuf, String),
}

pub fn list_cache_dirs(alpm: &Alpm) -> Vec<PathBuf> {
    alpm.cachedirs().iter().map(PathBuf::from).collect()
}

pub fn list_package_files(alpm: &Alpm) -> Vec<PackageFile> {
    alpm.trans_add()
        .iter()
        .filter(|package| package.origin() == PackageFrom::SyncDb)
        .filter_map(|package| {
            Some(PackageFile {
                filename: package.filename()?.to_owned(),
                size_bytes: package.size().max(0) as u64,
            })
        })
        .collect()
}

pub fn find_uncached<'a>(
    cache_dirs: &[PathBuf],
    package_files: &'a [PackageFile],
) -> Vec<&'a PackageFile> {
    package_files
        .iter()
        .filter(|package_file| {
            !cache_dirs.iter().any(|cache_dir| {
                std::fs::metadata(cache_dir.join(&package_file.filename))
                    .is_ok_and(|metadata| metadata.is_file())
            })
        })
        .collect()
}

pub fn count_download_bytes(cache_dirs: &[PathBuf], package_files: &[PackageFile]) -> u64 {
    find_uncached(cache_dirs, package_files)
        .into_iter()
        .map(|package_file| {
            let part_name = format!("{}{PART_SUFFIX}", package_file.filename);
            let part_bytes = cache_dirs
                .iter()
                .filter_map(|cache_dir| std::fs::metadata(cache_dir.join(&part_name)).ok())
                .find(|metadata| metadata.is_file())
                .map_or(0, |metadata| metadata.len());
            package_file.size_bytes.saturating_sub(part_bytes)
        })
        .sum()
}

pub fn clean_cache(cache_dirs: &[PathBuf], package_files: &[PackageFile]) {
    for outcome in remove_stale_parts(cache_dirs, package_files, ROOT_UID) {
        let line = match outcome {
            PartOutcome::Removed(path) => format!(
                "removed {}, a partial download that cannot complete",
                path.display()
            ),
            PartOutcome::Skipped(path, reason) => {
                format!("warning: {} left in place: {reason}", path.display())
            }
        };
        emit(&Event::Line(line));
    }
}

fn remove_stale_parts(
    cache_dirs: &[PathBuf],
    package_files: &[PackageFile],
    owner_uid: u32,
) -> Vec<PartOutcome> {
    let mut outcomes = Vec::new();
    for package_file in find_uncached(cache_dirs, package_files) {
        for cache_dir in cache_dirs {
            let part = cache_dir.join(format!("{}{PART_SUFFIX}", package_file.filename));
            let stale_file = open_owned(&part, owner_uid).and_then(|file| {
                let Some(mut file) = file else {
                    return Ok(None);
                };
                Ok(is_stale(&mut file, package_file)?.then_some(file))
            });
            let outcome = match stale_file {
                Ok(Some(file)) => remove_if_unchanged(&part, &file),
                Ok(None) => continue,
                Err(reason) => PartOutcome::Skipped(part, reason),
            };
            let is_removed = matches!(outcome, PartOutcome::Removed(_));
            outcomes.push(outcome);
            if !is_removed {
                continue;
            }
            let signature_part =
                cache_dir.join(format!("{}{SIGNATURE_PART_SUFFIX}", package_file.filename));
            match open_owned(&signature_part, owner_uid) {
                Ok(Some(file)) => outcomes.push(remove_if_unchanged(&signature_part, &file)),
                Ok(None) => {}
                Err(reason) => outcomes.push(PartOutcome::Skipped(signature_part, reason)),
            }
        }
    }
    outcomes
}

fn open_owned(path: &Path, owner_uid: u32) -> Result<Option<File>, String> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Err("not a regular file".to_owned()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format_error(error)),
    }
    let file = File::options()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK)
        .open(path)
        .map_err(format_error)?;
    let metadata = file.metadata().map_err(format_error)?;
    if !metadata.is_file() {
        return Err("not a regular file".to_owned());
    }
    if metadata.uid() != owner_uid {
        return Err(format!("belongs to user {}", metadata.uid()));
    }
    Ok(Some(file))
}

fn is_stale(file: &mut File, package_file: &PackageFile) -> Result<bool, String> {
    let part_bytes = file.metadata().map_err(format_error)?.len();
    if package_file.size_bytes != 0 && part_bytes >= package_file.size_bytes {
        return Ok(true);
    }
    let Some((_, magic)) = COMPRESSION_MAGICS
        .iter()
        .find(|(extension, _)| package_file.filename.ends_with(extension))
    else {
        return Ok(false);
    };
    let mut head = Vec::with_capacity(magic.len());
    file.take(magic.len() as u64)
        .read_to_end(&mut head)
        .map_err(format_error)?;
    Ok(!magic.starts_with(&head))
}

fn remove_if_unchanged(path: &Path, file: &File) -> PartOutcome {
    let is_same_file = match (file.metadata(), std::fs::symlink_metadata(path)) {
        (Ok(file_metadata), Ok(path_metadata)) => {
            file_metadata.dev() == path_metadata.dev() && file_metadata.ino() == path_metadata.ino()
        }
        _ => false,
    };
    if !is_same_file {
        return PartOutcome::Skipped(path.to_path_buf(), "changed while it was read".to_owned());
    }
    match std::fs::remove_file(path) {
        Ok(()) => PartOutcome::Removed(path.to_path_buf()),
        Err(error) => PartOutcome::Skipped(path.to_path_buf(), format_error(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const FILENAME: &str = "pkgconf-3.0.10-1.1-x86_64_v4.pkg.tar.zst";
    const SIZE_BYTES: u64 = 134368;
    const ZSTD_START: [u8; 6] = [0x28, 0xB5, 0x2F, 0xFD, 0x00, 0x58];
    const HTML_PAGE: &str = "<html><head><title>403 Forbidden</title></head></html>";

    struct Scratch {
        cache_dir: PathBuf,
        uid: u32,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let cache_dir =
                std::env::temp_dir().join(format!("proscenio-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&cache_dir);
            std::fs::create_dir_all(&cache_dir).unwrap();
            let uid = std::fs::metadata(&cache_dir).unwrap().uid();
            Scratch { cache_dir, uid }
        }

        fn write(&self, name: &str, content: &[u8]) -> PathBuf {
            let path = self.cache_dir.join(name);
            std::fs::write(&path, content).unwrap();
            path
        }

        fn cache_dirs(&self) -> &[PathBuf] {
            std::slice::from_ref(&self.cache_dir)
        }

        fn clean(&self, owner_uid: u32) -> Vec<PartOutcome> {
            remove_stale_parts(self.cache_dirs(), &package_files(), owner_uid)
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.cache_dir);
        }
    }

    fn package_files() -> [PackageFile; 1] {
        [PackageFile {
            filename: FILENAME.to_owned(),
            size_bytes: SIZE_BYTES,
        }]
    }

    fn part_name(suffix: &str) -> String {
        format!("{FILENAME}{suffix}")
    }

    #[test]
    fn a_part_as_large_as_the_package_is_removed_with_its_signature_part() {
        let scratch = Scratch::new("full-part");
        let mut content = ZSTD_START.to_vec();
        content.resize(SIZE_BYTES as usize, 0);
        let part = scratch.write(&part_name(PART_SUFFIX), &content);
        let signature_part = scratch.write(&part_name(SIGNATURE_PART_SUFFIX), b"half a signature");
        let download_bytes_before_cleaning =
            count_download_bytes(scratch.cache_dirs(), &package_files());

        let outcomes = scratch.clean(scratch.uid);

        assert_eq!(
            outcomes,
            [
                PartOutcome::Removed(part.clone()),
                PartOutcome::Removed(signature_part.clone())
            ]
        );
        assert!(!part.exists());
        assert!(!signature_part.exists());
        assert_eq!(download_bytes_before_cleaning, 0);
        assert_eq!(
            count_download_bytes(scratch.cache_dirs(), &package_files()),
            SIZE_BYTES
        );
    }

    #[test]
    fn a_smaller_part_that_is_not_a_package_is_removed() {
        let scratch = Scratch::new("html-part");
        let part = scratch.write(&part_name(PART_SUFFIX), HTML_PAGE.as_bytes());

        let outcomes = scratch.clean(scratch.uid);

        assert_eq!(outcomes, [PartOutcome::Removed(part.clone())]);
        assert!(!part.exists());
    }

    #[test]
    fn a_smaller_part_that_starts_as_the_package_does_is_kept_to_resume() {
        let scratch = Scratch::new("valid-part");
        let part = scratch.write(&part_name(PART_SUFFIX), &ZSTD_START);
        let short_part_name = "short-1-1-x86_64.pkg.tar.zst.part";
        let short_part = scratch.write(short_part_name, &ZSTD_START[..2]);
        let unsized_part_name = "unsized-1-1-x86_64.pkg.tar.zst.part";
        let unsized_part = scratch.write(unsized_part_name, &ZSTD_START);
        let package_files = [
            package_files().into_iter().next().unwrap(),
            PackageFile {
                filename: short_part_name.trim_end_matches(PART_SUFFIX).to_owned(),
                size_bytes: SIZE_BYTES,
            },
            PackageFile {
                filename: unsized_part_name.trim_end_matches(PART_SUFFIX).to_owned(),
                size_bytes: 0,
            },
        ];

        let outcomes = remove_stale_parts(scratch.cache_dirs(), &package_files, scratch.uid);

        assert_eq!(outcomes, []);
        assert!(part.exists());
        assert!(short_part.exists());
        assert!(unsized_part.exists());
        assert_eq!(
            count_download_bytes(scratch.cache_dirs(), &package_files),
            2 * SIZE_BYTES - ZSTD_START.len() as u64 - 2
        );
    }

    #[test]
    fn a_linked_part_or_one_of_another_user_is_left_in_place_and_reported() {
        let scratch = Scratch::new("linked-part");
        let target = scratch.write("target", HTML_PAGE.as_bytes());
        let part = scratch.cache_dir.join(part_name(PART_SUFFIX));
        symlink(&target, &part).unwrap();

        let link_outcomes = scratch.clean(scratch.uid);
        std::fs::remove_file(&part).unwrap();
        scratch.write(&part_name(PART_SUFFIX), HTML_PAGE.as_bytes());
        let owner_outcomes = scratch.clean(scratch.uid + 1);

        assert!(
            matches!(&link_outcomes[..], [PartOutcome::Skipped(path, _)] if *path == part),
            "{link_outcomes:?}"
        );
        assert!(
            matches!(&owner_outcomes[..], [PartOutcome::Skipped(path, _)] if *path == part),
            "{owner_outcomes:?}"
        );
        assert!(target.exists());
        assert!(part.exists());
    }

    #[test]
    fn a_part_of_a_package_outside_the_transaction_or_already_cached_is_kept() {
        let scratch = Scratch::new("unrelated-part");
        let part = scratch.write("other-1-1-x86_64.pkg.tar.zst.part", HTML_PAGE.as_bytes());
        let signature_part = scratch.write(&part_name(SIGNATURE_PART_SUFFIX), b"signature");

        let unrelated_outcomes = scratch.clean(scratch.uid);
        scratch.write(FILENAME, &ZSTD_START);
        let package_part = scratch.write(&part_name(PART_SUFFIX), HTML_PAGE.as_bytes());
        let cached_outcomes = scratch.clean(scratch.uid);

        assert_eq!(unrelated_outcomes, []);
        assert_eq!(cached_outcomes, []);
        assert!(part.exists());
        assert!(signature_part.exists());
        assert!(package_part.exists());
    }
}
