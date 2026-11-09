use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

use crate::errors::describe_path_error;
use crate::private_database::O_NOFOLLOW;

const DIR_MODE: u32 = 0o755;

pub fn is_name(part: &str) -> bool {
    !part.is_empty() && part != "." && part != ".." && !part.contains(['/', '\0'])
}

pub fn ensure_real_dirs(path: &Path) -> Result<(), String> {
    for ancestor in path.ancestors() {
        let metadata =
            fs::symlink_metadata(ancestor).map_err(|error| describe_path_error(ancestor, error))?;
        if !metadata.is_dir() {
            return Err(format!("{} is not a directory", ancestor.display()));
        }
    }
    Ok(())
}

pub fn create_dir(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => Ok(()),
        Ok(_) => Err(format!("{} is not a directory", path.display())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => fs::DirBuilder::new()
            .mode(DIR_MODE)
            .create(path)
            .and_then(|()| fs::set_permissions(path, fs::Permissions::from_mode(DIR_MODE)))
            .map_err(|error| describe_path_error(path, error)),
        Err(error) => Err(describe_path_error(path, error)),
    }
}

pub fn create_dirs(path: &Path) -> Result<(), String> {
    let mut ancestors: Vec<&Path> = path.ancestors().collect();
    ancestors.reverse();
    ancestors.into_iter().try_for_each(create_dir)
}

pub fn remove_dir(path: &Path) -> Result<(), String> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(describe_path_error(path, error)),
    };
    ensure_real_dirs(path.parent().unwrap_or(Path::new("/")))?;
    if metadata.is_dir() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
    .map_err(|error| describe_path_error(path, error))
}

pub fn write_new_file(target: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .custom_flags(O_NOFOLLOW)
        .open(target)
        .map_err(|error| describe_path_error(target, error))?;
    file.write_all(bytes)
        .and_then(|()| file.set_permissions(fs::Permissions::from_mode(mode)))
        .map_err(|error| describe_path_error(target, error))
}

pub fn write_file(target: &Path, bytes: &[u8], mode: u32) -> Result<(), String> {
    let name = target
        .file_name()
        .ok_or_else(|| format!("{} has no file name", target.display()))?;
    let temporary = target.with_file_name(format!(".{}.proscenio", name.to_string_lossy()));
    let _ = fs::remove_file(&temporary);
    write_new_file(&temporary, bytes, mode)
        .and_then(|()| {
            fs::rename(&temporary, target).map_err(|error| describe_path_error(target, error))
        })
        .inspect_err(|_| {
            let _ = fs::remove_file(&temporary);
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn directories_are_not_created_through_a_symbolic_link() {
        let root = std::env::temp_dir().join(format!("proscenio-files-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let elsewhere = root.join("elsewhere");
        fs::create_dir_all(&elsewhere).unwrap();
        symlink(&elsewhere, root.join("link")).unwrap();
        assert!(create_dirs(&root.join("link/sub")).is_err());
        assert!(!elsewhere.join("sub").exists());
        create_dirs(&root.join("real/sub")).unwrap();
        assert!(root.join("real/sub").is_dir());
        fs::create_dir(elsewhere.join("sub")).unwrap();
        assert!(remove_dir(&root.join("link/sub")).is_err());
        assert!(elsewhere.join("sub").is_dir());
        fs::remove_dir_all(&root).unwrap();
    }
}
