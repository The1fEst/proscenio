use std::fs::{File, Metadata};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use crate::errors::describe_path_error;
use crate::stage::{SYNC_DIR, list_databases};

pub const MAX_DATABASE_BYTES: u64 = 256 << 20;
pub const MAX_TOTAL_DATABASE_BYTES: u64 = 512 << 20;
pub const MAX_DATABASE_FILE_COUNT: usize = 128;
const PASSWD: &str = "/etc/passwd";
const DATABASE_MODE: u32 = 0o644;
const O_NONBLOCK: i32 = 0o4000;
#[cfg(any(
    target_arch = "arm",
    target_arch = "aarch64",
    target_arch = "powerpc",
    target_arch = "powerpc64"
))]
const O_NOFOLLOW: i32 = 0o100000;
#[cfg(any(
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "riscv32",
    target_arch = "riscv64",
    target_arch = "loongarch64",
    target_arch = "s390x"
))]
const O_NOFOLLOW: i32 = 0o400000;
#[cfg(not(any(
    target_arch = "arm",
    target_arch = "aarch64",
    target_arch = "powerpc",
    target_arch = "powerpc64",
    target_arch = "x86",
    target_arch = "x86_64",
    target_arch = "riscv32",
    target_arch = "riscv64",
    target_arch = "loongarch64",
    target_arch = "s390x"
)))]
compile_error!("the open flags of this architecture are not known");

unsafe extern "C" {
    fn geteuid() -> u32;
}

pub fn check_location(private_path: &Path) -> Result<(), String> {
    let uid = unsafe { geteuid() };
    locate_private(private_path, &find_home(uid)?, uid).map(|_| ())
}

pub fn check_sync_location(private_path: &Path) -> Result<(), String> {
    let uid = unsafe { geteuid() };
    locate_sync(private_path, &find_home(uid)?, uid).map(|_| ())
}

pub fn find_home(uid: u32) -> Result<PathBuf, String> {
    let passwd = std::fs::read_to_string(PASSWD)
        .map_err(|error| describe_path_error(Path::new(PASSWD), error))?;
    parse_home(&passwd, uid).ok_or_else(|| format!("no home folder is known for user {uid}"))
}

fn parse_home(passwd: &str, uid: u32) -> Option<PathBuf> {
    passwd.lines().find_map(|line| {
        let fields: Vec<&str> = line.split(':').collect();
        let line_uid: u32 = fields.get(2)?.parse().ok()?;
        let home = PathBuf::from(fields.get(5)?);
        (line_uid == uid && home.is_absolute()).then_some(home)
    })
}

pub fn copy_databases(
    private_path: &Path,
    home: &Path,
    uid: u32,
    repositories: &[String],
    target_sync: &Path,
) -> Result<(), String> {
    let sync = locate_sync(private_path, home, uid)?;
    let databases: Vec<PathBuf> = list_databases(&sync)?
        .into_iter()
        .filter(|database| is_database_of(database, repositories))
        .collect();
    if databases.is_empty() {
        return Err(format!("{} holds no package databases", sync.display()));
    }
    if databases.len() > MAX_DATABASE_FILE_COUNT {
        return Err(format!(
            "{} holds more than {MAX_DATABASE_FILE_COUNT} database files",
            sync.display()
        ));
    }
    let total_bytes: u64 = databases
        .iter()
        .filter_map(|database| std::fs::symlink_metadata(database).ok())
        .map(|metadata| metadata.len())
        .sum();
    if total_bytes > MAX_TOTAL_DATABASE_BYTES {
        return Err(format!(
            "{} holds more than {MAX_TOTAL_DATABASE_BYTES} bytes of databases",
            sync.display()
        ));
    }
    let mut remaining_bytes = MAX_TOTAL_DATABASE_BYTES;
    for database in databases {
        copy_database(&database, uid, target_sync, &mut remaining_bytes)?;
    }
    Ok(())
}

fn is_database_of(path: &Path, repositories: &[String]) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.strip_suffix(".sig").unwrap_or(name))
        .and_then(|name| name.strip_suffix(".db"))
        .is_some_and(|repository| repositories.iter().any(|name| name == repository))
}

fn locate_private(private_path: &Path, home: &Path, uid: u32) -> Result<PathBuf, String> {
    let relative = private_path.strip_prefix(home).map_err(|_| {
        format!(
            "{} is not inside {}",
            private_path.display(),
            home.display()
        )
    })?;
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(format!("{} is not a plain path", private_path.display()));
    }
    let home_metadata =
        std::fs::metadata(home).map_err(|error| describe_path_error(home, error))?;
    check_directory(home, &home_metadata, uid)?;
    let mut folder = home.to_path_buf();
    for component in relative.components() {
        folder.push(component);
        check_folder(&folder, uid)?;
    }
    Ok(folder)
}

fn locate_sync(private_path: &Path, home: &Path, uid: u32) -> Result<PathBuf, String> {
    let sync = locate_private(private_path, home, uid)?.join(SYNC_DIR);
    check_folder(&sync, uid)?;
    Ok(sync)
}

fn check_folder(path: &Path, uid: u32) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| describe_path_error(path, error))?;
    check_directory(path, &metadata, uid)
}

fn check_directory(path: &Path, metadata: &Metadata, uid: u32) -> Result<(), String> {
    if !metadata.is_dir() {
        return Err(format!("{} is not a folder", path.display()));
    }
    check_owner(path, metadata, uid)
}

fn check_owner(path: &Path, metadata: &Metadata, uid: u32) -> Result<(), String> {
    if metadata.uid() != uid {
        return Err(format!("{} belongs to another user", path.display()));
    }
    Ok(())
}

fn copy_database(
    path: &Path,
    uid: u32,
    target_sync: &Path,
    remaining_bytes: &mut u64,
) -> Result<(), String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{} has no name", path.display()))?;
    let path_metadata =
        std::fs::symlink_metadata(path).map_err(|error| describe_path_error(path, error))?;
    if !path_metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let source = File::options()
        .read(true)
        .custom_flags(O_NOFOLLOW | O_NONBLOCK)
        .open(path)
        .map_err(|error| describe_path_error(path, error))?;
    let metadata = source
        .metadata()
        .map_err(|error| describe_path_error(path, error))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    check_owner(path, &metadata, uid)?;
    let limit_bytes = (*remaining_bytes).min(MAX_DATABASE_BYTES);
    if metadata.len() > limit_bytes {
        return Err(format!(
            "{} is larger than {limit_bytes} bytes",
            path.display()
        ));
    }
    let target = target_sync.join(name);
    let mut target_file = File::options()
        .write(true)
        .create_new(true)
        .mode(DATABASE_MODE)
        .open(&target)
        .map_err(|error| describe_path_error(&target, error))?;
    let copied_bytes = std::io::copy(&mut source.take(limit_bytes + 1), &mut target_file)
        .map_err(|error| format!("{} to {}: {error}", path.display(), target.display()))?;
    if copied_bytes > limit_bytes {
        return Err(format!(
            "{} is larger than {limit_bytes} bytes",
            path.display()
        ));
    }
    *remaining_bytes -= copied_bytes;
    metadata
        .modified()
        .and_then(|modified| target_file.set_modified(modified))
        .map_err(|error| describe_path_error(&target, error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, symlink};
    use std::time::Duration;

    struct Scratch {
        root: PathBuf,
        uid: u32,
    }

    impl Scratch {
        fn new(name: &str) -> Self {
            let root =
                std::env::temp_dir().join(format!("proscenio-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(root.join("home")).unwrap();
            std::fs::create_dir_all(root.join("target")).unwrap();
            let uid = std::fs::metadata(&root).unwrap().uid();
            Scratch { root, uid }
        }

        fn home(&self) -> PathBuf {
            self.root.join("home")
        }

        fn target(&self) -> PathBuf {
            self.root.join("target")
        }

        fn private_path(&self) -> PathBuf {
            self.home().join(".cache/proscenio/pacman")
        }

        fn fill_sync(&self, files: &[(&str, &str)]) -> PathBuf {
            let sync = self.private_path().join("sync");
            std::fs::create_dir_all(&sync).unwrap();
            for (name, content) in files {
                std::fs::write(sync.join(name), content).unwrap();
            }
            sync
        }

        fn copy(&self, private_path: &Path, uid: u32) -> Result<(), String> {
            let repositories: Vec<String> = std::fs::read_dir(self.private_path().join("sync"))
                .into_iter()
                .flatten()
                .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
                .filter_map(|name| name.strip_suffix(".db").map(str::to_owned))
                .collect();
            self.copy_repositories(private_path, uid, &repositories)
        }

        fn copy_repositories(
            &self,
            private_path: &Path,
            uid: u32,
            repositories: &[String],
        ) -> Result<(), String> {
            copy_databases(
                private_path,
                &self.home(),
                uid,
                repositories,
                &self.target(),
            )
        }

        fn list_target(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.target())
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            names
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn the_databases_and_their_signatures_are_copied_with_their_times() {
        let scratch = Scratch::new("copy");
        let sync = scratch.fill_sync(&[
            ("core.db", "core"),
            ("core.db.sig", "signature"),
            ("extra.db.part", "half a download"),
            ("notes.txt", "not a database"),
        ]);
        let synced_time = std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000);
        std::fs::File::options()
            .write(true)
            .open(sync.join("core.db"))
            .unwrap()
            .set_modified(synced_time)
            .unwrap();
        std::os::unix::fs::symlink(
            "/var/lib/pacman/local",
            scratch.private_path().join("local"),
        )
        .unwrap();

        let copy_result = scratch.copy(&scratch.private_path(), scratch.uid);
        let target = scratch.target();
        let results = (
            std::fs::read_to_string(target.join("core.db")).ok(),
            std::fs::read_to_string(target.join("core.db.sig")).ok(),
            std::fs::metadata(target.join("core.db"))
                .and_then(|metadata| metadata.modified())
                .ok(),
            scratch.list_target(),
        );
        assert_eq!(copy_result, Ok(()));
        assert_eq!(
            results,
            (
                Some("core".to_owned()),
                Some("signature".to_owned()),
                Some(synced_time),
                vec!["core.db".to_owned(), "core.db.sig".to_owned()],
            )
        );
    }

    #[test]
    fn only_the_databases_of_the_configured_repositories_are_copied() {
        let scratch = Scratch::new("repositories");
        scratch.fill_sync(&[
            ("core.db", "core"),
            ("core.db.sig", "signature"),
            ("removed.db", "removed"),
            ("removed.db.sig", "removed signature"),
            ("coreextra.db", "similar name"),
            (".db", "no name"),
        ]);

        let copy_result =
            scratch.copy_repositories(&scratch.private_path(), scratch.uid, &["core".to_owned()]);
        let target_names = scratch.list_target();
        let empty_result = scratch.copy_repositories(&scratch.private_path(), scratch.uid, &[]);

        assert_eq!(copy_result, Ok(()));
        assert_eq!(target_names, ["core.db", "core.db.sig"]);
        assert!(empty_result.is_err(), "{empty_result:?}");
    }

    #[test]
    fn only_a_path_inside_the_users_own_home_is_read() {
        let scratch = Scratch::new("location");
        scratch.fill_sync(&[("core.db", "core")]);
        let outside_path = scratch.root.join("outside/pacman");
        std::fs::create_dir_all(outside_path.join("sync")).unwrap();
        std::fs::write(outside_path.join("sync/core.db"), "core").unwrap();
        let climbing_path = scratch.home().join(".cache/../../outside/pacman");
        let relative_path = PathBuf::from(".cache/proscenio/pacman");
        let lookalike_path = scratch.root.join("home2/pacman");
        std::fs::create_dir_all(lookalike_path.join("sync")).unwrap();
        std::fs::write(lookalike_path.join("sync/core.db"), "core").unwrap();

        for path in [
            &outside_path,
            &climbing_path,
            &relative_path,
            &lookalike_path,
        ] {
            assert!(scratch.copy(path, scratch.uid).is_err(), "{path:?}");
        }
        assert!(scratch.list_target().is_empty());
    }

    #[test]
    fn the_folders_on_the_way_are_checked_before_the_first_synchronization_makes_the_databases() {
        let scratch = Scratch::new("first");
        std::fs::create_dir_all(scratch.private_path()).unwrap();

        let private_result = locate_private(&scratch.private_path(), &scratch.home(), scratch.uid);
        let sync_result = locate_sync(&scratch.private_path(), &scratch.home(), scratch.uid);
        let other_user_result =
            locate_private(&scratch.private_path(), &scratch.home(), scratch.uid + 1);
        let home_result = locate_private(&scratch.home(), &scratch.home(), scratch.uid);

        assert_eq!(private_result, Ok(scratch.private_path()));
        assert!(sync_result.is_err(), "{sync_result:?}");
        assert!(other_user_result.is_err(), "{other_user_result:?}");
        assert!(home_result.is_err(), "{home_result:?}");
    }

    #[test]
    fn a_folder_or_database_of_another_user_is_refused() {
        let scratch = Scratch::new("owner");
        scratch.fill_sync(&[("core.db", "core")]);

        let result = scratch.copy(&scratch.private_path(), scratch.uid + 1);

        assert!(result.is_err(), "{result:?}");
        assert!(scratch.list_target().is_empty());
    }

    #[test]
    fn a_symbolic_link_instead_of_a_folder_or_a_database_is_refused() {
        let scratch = Scratch::new("links");
        let secret_path = scratch.root.join("secret");
        std::fs::write(&secret_path, "secret").unwrap();

        scratch.fill_sync(&[("extra.db", "extra")]);
        symlink(&secret_path, scratch.private_path().join("sync/core.db")).unwrap();
        let database_link_result = scratch.copy(&scratch.private_path(), scratch.uid);
        std::fs::remove_file(scratch.private_path().join("sync/core.db")).unwrap();

        let real_sync = scratch.private_path().join("real-sync");
        std::fs::rename(scratch.private_path().join("sync"), &real_sync).unwrap();
        symlink(&real_sync, scratch.private_path().join("sync")).unwrap();
        let sync_link_result = scratch.copy(&scratch.private_path(), scratch.uid);
        std::fs::remove_file(scratch.private_path().join("sync")).unwrap();
        std::fs::rename(&real_sync, scratch.private_path().join("sync")).unwrap();

        let real_cache = scratch.home().join(".cache-real");
        std::fs::rename(scratch.home().join(".cache"), &real_cache).unwrap();
        symlink(&real_cache, scratch.home().join(".cache")).unwrap();
        let folder_link_result = scratch.copy(&scratch.private_path(), scratch.uid);

        assert!(database_link_result.is_err(), "{database_link_result:?}");
        assert!(sync_link_result.is_err(), "{sync_link_result:?}");
        assert!(folder_link_result.is_err(), "{folder_link_result:?}");
    }

    #[test]
    fn databases_that_together_exceed_the_total_limit_are_refused_before_any_is_copied() {
        let scratch = Scratch::new("total");
        let sync = scratch.fill_sync(&[]);
        let count = MAX_TOTAL_DATABASE_BYTES / MAX_DATABASE_BYTES + 1;
        for index in 0..count {
            std::fs::File::create(sync.join(format!("repository{index}.db")))
                .unwrap()
                .set_len(MAX_DATABASE_BYTES)
                .unwrap();
        }

        let result = scratch.copy(&scratch.private_path(), scratch.uid);

        assert!(result.is_err(), "{result:?}");
        assert!(scratch.list_target().is_empty());
    }

    #[test]
    fn a_pipe_a_huge_file_too_many_files_or_none_are_refused() {
        let scratch = Scratch::new("limits");
        let sync = scratch.fill_sync(&[("extra.db", "extra")]);
        let pipe_status = std::process::Command::new("mkfifo")
            .arg(sync.join("core.db"))
            .status()
            .unwrap();
        assert!(pipe_status.success());
        let pipe_result = scratch.copy(&scratch.private_path(), scratch.uid);
        std::fs::remove_file(sync.join("core.db")).unwrap();

        std::fs::File::create(sync.join("core.db"))
            .unwrap()
            .set_len(MAX_DATABASE_BYTES + 1)
            .unwrap();
        let huge_result = scratch.copy(&scratch.private_path(), scratch.uid);
        std::fs::remove_file(sync.join("core.db")).unwrap();

        for index in 0..MAX_DATABASE_FILE_COUNT {
            std::fs::write(sync.join(format!("repository{index}.db")), "").unwrap();
        }
        let many_result = scratch.copy(&scratch.private_path(), scratch.uid);

        let empty_scratch = Scratch::new("none");
        empty_scratch.fill_sync(&[("notes.txt", "not a database")]);
        let none_result = empty_scratch.copy(&empty_scratch.private_path(), empty_scratch.uid);

        assert!(pipe_result.is_err(), "{pipe_result:?}");
        assert!(huge_result.is_err(), "{huge_result:?}");
        assert!(many_result.is_err(), "{many_result:?}");
        assert!(none_result.is_err(), "{none_result:?}");
    }

    #[test]
    fn the_home_comes_from_the_passwd_line_of_the_uid() {
        let passwd = "root:x:0:0:root:/root:/bin/bash\n\
                      fest:x:1000:1000:fEst:/home/fest:/bin/fish\n\
                      broken:x:1001\n\
                      other:x:10000:1000::/home/other:/bin/sh\n";
        assert_eq!(parse_home(passwd, 1000), Some(PathBuf::from("/home/fest")));
        assert_eq!(parse_home(passwd, 0), Some(PathBuf::from("/root")));
        assert_eq!(parse_home(passwd, 1001), None);
        assert_eq!(parse_home(passwd, 7), None);
        assert_eq!(parse_home("fest:x:1000:1000:::/bin/sh\n", 1000), None);
    }
}
