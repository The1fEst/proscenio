use std::path::{Path, PathBuf};

use crate::errors::{describe_path_error, format_error};

pub const STAGE_DIR: &str = "proscenio-stage";
pub const SYNC_DIR: &str = "sync";
const LOCAL_DIR: &str = "local";

pub fn prepare_stage(db_path: &Path) -> Result<PathBuf, String> {
    let stage = db_path.join(STAGE_DIR);
    match std::fs::remove_dir_all(&stage) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format_error(error)),
    }
    let staged_sync = stage.join(SYNC_DIR);
    std::fs::create_dir(&stage).map_err(|error| describe_path_error(&stage, error))?;
    std::fs::create_dir(&staged_sync).map_err(|error| describe_path_error(&staged_sync, error))?;
    std::os::unix::fs::symlink(db_path.join(LOCAL_DIR), stage.join(LOCAL_DIR))
        .map_err(|error| describe_path_error(&stage, error))?;
    Ok(stage)
}

pub fn list_databases(sync: &Path) -> Result<Vec<PathBuf>, String> {
    let mut databases = Vec::new();
    for entry in std::fs::read_dir(sync).map_err(|error| describe_path_error(sync, error))? {
        let path = entry
            .map_err(|error| describe_path_error(sync, error))?
            .path();
        let is_database = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".db") || name.ends_with(".db.sig"));
        if is_database {
            databases.push(path);
        }
    }
    Ok(databases)
}

pub fn require_signatures(system_sync: &Path, staged_sync: &Path) -> Result<(), String> {
    for path in list_databases(system_sync)? {
        let Some(name) = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| name.ends_with(".db.sig"))
        else {
            continue;
        };
        let is_staged = staged_sync.join(name.trim_end_matches(".sig")).exists();
        if is_staged && !staged_sync.join(name).exists() {
            return Err(format!(
                "the databases to install lack {name}, which the system databases have"
            ));
        }
    }
    Ok(())
}

pub fn install_staged_databases(staged_sync: &Path, system_sync: &Path) -> Result<(), String> {
    for path in list_databases(staged_sync)? {
        let Some(name) = path.file_name() else {
            continue;
        };
        std::fs::rename(&path, system_sync.join(name))
            .map_err(|error| describe_path_error(&path, error))?;
    }
    Ok(())
}

pub fn find_system_database_path() -> Result<PathBuf, String> {
    let pacman_config = pacmanconf::Config::new().map_err(format_error)?;
    Ok(PathBuf::from(pacman_config.db_path))
}

pub fn find_repository_names() -> Result<Vec<String>, String> {
    let pacman_config = pacmanconf::Config::new().map_err(format_error)?;
    Ok(pacman_config
        .repos
        .into_iter()
        .map(|repository| repository.name)
        .collect())
}

pub fn path_to_str(path: &Path) -> Result<&str, String> {
    path.to_str()
        .ok_or_else(|| format!("{} is not valid text", path.display()))
}

pub fn discard_stage() {
    if let Ok(db_path) = find_system_database_path() {
        let _ = std::fs::remove_dir_all(db_path.join(STAGE_DIR));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::ALPM_LOCK;

    fn make_scratch_folder(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("proscenio-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(&directory).unwrap();
        directory
    }

    #[test]
    fn a_fresh_stage_shares_the_installed_packages_and_starts_without_databases() {
        let db_path = make_scratch_folder("stage");
        let stage = db_path.join(STAGE_DIR);
        std::fs::create_dir_all(stage.join(SYNC_DIR)).unwrap();
        std::fs::write(stage.join(SYNC_DIR).join("extra.db"), "earlier stage").unwrap();
        std::fs::write(stage.join(ALPM_LOCK), "").unwrap();
        std::fs::create_dir_all(db_path.join(SYNC_DIR)).unwrap();
        std::fs::write(db_path.join(SYNC_DIR).join("core.db"), "system core").unwrap();

        let prepared = prepare_stage(&db_path);
        let staged_sync = stage.join(SYNC_DIR);
        let staged_count = std::fs::read_dir(&staged_sync).unwrap().count();
        let local = std::fs::read_link(stage.join(LOCAL_DIR));
        let has_lock = stage.join(ALPM_LOCK).exists();
        std::fs::remove_dir_all(&db_path).unwrap();
        assert_eq!(prepared, Ok(stage));
        assert_eq!(staged_count, 0);
        assert_eq!(local.ok(), Some(db_path.join(LOCAL_DIR)));
        assert!(!has_lock);
    }

    #[test]
    fn a_database_signed_in_the_system_folder_stays_signed_in_the_stage() {
        let directory = make_scratch_folder("signatures");
        let (staged, system) = (directory.join("staged"), directory.join("system"));
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        for name in ["core.db", "core.db.sig", "extra.db"] {
            std::fs::write(system.join(name), "").unwrap();
        }
        for name in ["core.db", "extra.db", "extra.db.sig"] {
            std::fs::write(staged.join(name), "").unwrap();
        }

        let unsigned_result = require_signatures(&system, &staged);
        std::fs::write(staged.join("core.db.sig"), "").unwrap();
        let signed_result = require_signatures(&system, &staged);
        std::fs::remove_file(system.join("core.db.sig")).unwrap();
        std::fs::remove_file(staged.join("core.db.sig")).unwrap();
        let never_signed_result = require_signatures(&system, &staged);
        std::fs::write(system.join("removed.db.sig"), "").unwrap();
        let orphaned_result = require_signatures(&system, &staged);
        std::fs::remove_dir_all(&directory).unwrap();

        assert!(unsigned_result.unwrap_err().contains("core.db.sig"));
        assert_eq!(signed_result, Ok(()));
        assert_eq!(never_signed_result, Ok(()));
        assert_eq!(orphaned_result, Ok(()));
    }

    #[test]
    fn staged_databases_replace_the_system_ones_and_nothing_else_moves() {
        let directory = make_scratch_folder("databases");
        let (staged, system) = (directory.join("staged"), directory.join("system"));
        std::fs::create_dir_all(&staged).unwrap();
        std::fs::create_dir_all(&system).unwrap();
        for (name, content) in [
            ("core.db", "new core"),
            ("core.db.sig", "new signature"),
            ("extra.db.part", "half a download"),
            ("notes.txt", "not a database"),
        ] {
            std::fs::write(staged.join(name), content).unwrap();
        }
        for (name, content) in [("core.db", "old core"), ("extra.db", "old extra")] {
            std::fs::write(system.join(name), content).unwrap();
        }

        let installed = install_staged_databases(&staged, &system);
        let read = |name: &str| std::fs::read_to_string(system.join(name)).ok();
        let results = (
            read("core.db"),
            read("core.db.sig"),
            read("extra.db"),
            read("extra.db.part"),
            read("notes.txt"),
            staged.join("core.db").exists(),
            staged.join("extra.db.part").exists(),
        );
        std::fs::remove_dir_all(&directory).unwrap();
        assert_eq!(installed, Ok(()));
        assert_eq!(
            results,
            (
                Some("new core".to_owned()),
                Some("new signature".to_owned()),
                Some("old extra".to_owned()),
                None,
                None,
                false,
                true,
            )
        );
    }
}
