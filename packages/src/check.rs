use std::io::Write;
use std::path::Path;
use std::time::Duration;

use crate::errors::format_error;
use crate::handle::{ALPM_LOCK, open_alpm};

const CHECK_LOCK_TIMEOUT: Duration = Duration::from_secs(120);
const CHECK_LOCK_RETRY_DELAY: Duration = Duration::from_millis(250);
const CHECK_LOCK: &str = "proscenio-check.lock";

fn lock_check_database(db_path: &Path) -> Result<std::fs::File, String> {
    let lock_file = std::fs::File::options()
        .create(true)
        .write(true)
        .truncate(false)
        .open(db_path.join(CHECK_LOCK))
        .map_err(format_error)?;
    let deadline = std::time::Instant::now() + CHECK_LOCK_TIMEOUT;
    loop {
        match lock_file.try_lock() {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(CHECK_LOCK_RETRY_DELAY);
            }
            Err(std::fs::TryLockError::WouldBlock) => {
                return Err("another check is still running".to_owned());
            }
            Err(std::fs::TryLockError::Error(error)) => return Err(format_error(error)),
        }
    }
    match std::fs::remove_file(db_path.join(ALPM_LOCK)) {
        Ok(()) => Ok(lock_file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(lock_file),
        Err(error) => Err(format_error(error)),
    }
}

pub fn check(db_path: &str) -> Result<(), String> {
    let _lock_file = lock_check_database(Path::new(db_path))?;
    let mut alpm = open_alpm(Some(db_path))?;
    alpm.set_sandbox_user(None::<&str>).map_err(format_error)?;
    alpm.set_disable_sandbox_filesystem(true);
    alpm.set_disable_sandbox_syscalls(true);
    alpm.syncdbs_mut().update(false).map_err(format_error)?;
    let ignored_packages: Vec<String> = alpm.ignorepkgs().iter().map(str::to_owned).collect();
    let mut stdout = std::io::stdout().lock();
    for package in alpm.localdb().pkgs() {
        let Some(sync_package) = package.sync_new_version(alpm.syncdbs()) else {
            continue;
        };
        if ignored_packages.iter().any(|name| name == package.name()) {
            continue;
        }
        writeln!(
            stdout,
            "{} {} -> {}",
            package.name(),
            package.version().as_str(),
            sync_package.version().as_str()
        )
        .map_err(format_error)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_check_holds_its_database_and_clears_a_lock_a_stopped_check_left() {
        let db_path = std::env::temp_dir().join(format!("proscenio-pacman-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&db_path);
        std::fs::create_dir_all(&db_path).unwrap();
        std::fs::write(db_path.join(ALPM_LOCK), "").unwrap();
        let lock_file = lock_check_database(&db_path);
        let has_alpm_lock = db_path.join(ALPM_LOCK).exists();
        let other_file = std::fs::File::open(db_path.join(CHECK_LOCK));
        let is_locked_while_held = other_file
            .as_ref()
            .is_ok_and(|file| file.try_lock().is_err());
        drop(lock_file);
        let free_deadline = std::time::Instant::now() + Duration::from_secs(5);
        let is_free_after = other_file.is_ok_and(|file| {
            while file.try_lock().is_err() {
                if std::time::Instant::now() >= free_deadline {
                    return false;
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            true
        });
        std::fs::remove_dir_all(&db_path).unwrap();
        assert!(!has_alpm_lock);
        assert!(is_locked_while_held);
        assert!(is_free_after);
    }
}
