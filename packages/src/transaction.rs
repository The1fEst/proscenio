use alpm::{
    Alpm, AlpmList, CommitData, CommitError, DepMod, Error, FileConflict, PrepareData,
    PrepareError, TransFlag,
};
use alpm_sys::{alpm_fileconflict_t, alpm_fileconflicttype_t};
use proscenio_protocol::{Change, Event, Stage};
use std::ffi::{CStr, c_char};
use std::fmt::Display;
use std::mem::transmute;
use std::path::Path;

use crate::changes::{collect_changes, compare_changes};
use crate::errors::format_error;
use crate::handle::{open_alpm, watch};
use crate::output::emit;
use crate::package_cache::{
    clean_cache, count_download_bytes, find_uncached, list_cache_dirs, list_package_files,
};
use crate::private_database::{copy_databases, find_home};
use crate::stage::{
    SYNC_DIR, find_repository_names, find_system_database_path, install_staged_databases,
    path_to_str, prepare_stage, require_signatures,
};

const MAX_CAUSE_LINES: usize = 100;

fn run_transaction<T>(
    alpm: &mut Alpm,
    flags: TransFlag,
    run: impl FnOnce(&mut Alpm) -> Result<T, String>,
) -> Result<T, String> {
    alpm.trans_init(flags).map_err(format_error)?;
    let result = run(alpm);
    let _ = alpm.trans_release();
    result
}

pub fn plan_changes(alpm: &mut Alpm) -> Result<Vec<Change>, String> {
    run_transaction(alpm, TransFlag::NONE, |alpm| {
        alpm.sync_sysupgrade(false).map_err(format_error)?;
        alpm.trans_prepare()
            .map_err(|error| describe_prepare_error(&error))?;
        Ok(collect_changes(alpm))
    })
}

pub fn upgrade(confirmed: &[Change], private_path: &Path, uid: u32) -> Result<(), String> {
    let system_path = find_system_database_path()?;
    let system_sync = system_path.join(SYNC_DIR);
    let mut alpm = open_alpm(None)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Prepare));
    run_transaction(&mut alpm, TransFlag::NONE, |alpm| {
        let stage = prepare_stage(&system_path)?;
        let staged_sync = stage.join(SYNC_DIR);
        let repositories = find_repository_names()?;
        copy_databases(
            private_path,
            &find_home(uid)?,
            uid,
            &repositories,
            &staged_sync,
        )?;
        require_signatures(&system_sync, &staged_sync)?;
        verify_stage(&stage, confirmed)?;
        install_staged_databases(&staged_sync, &system_sync)?;
        upgrade_packages(alpm, confirmed)
    })
}

fn upgrade_packages(alpm: &mut Alpm, confirmed: &[Change]) -> Result<(), String> {
    alpm.sync_sysupgrade(false).map_err(format_error)?;
    let error = match commit(alpm, Some(confirmed))? {
        Err(error) if is_download_error(&error) => error,
        result => return result.map_err(|error| describe_commit_error(alpm, &error)),
    };
    emit(&Event::Line(format!(
        "{}, downloading the packages again",
        describe_commit_error(alpm, &error)
    )));
    alpm.trans_release().map_err(format_error)?;
    alpm.trans_init(TransFlag::NONE).map_err(format_error)?;
    alpm.sync_sysupgrade(false).map_err(format_error)?;
    commit(alpm, Some(confirmed))?.map_err(|error| describe_commit_error(alpm, &error))
}

fn verify_stage(stage: &Path, confirmed: &[Change]) -> Result<(), String> {
    let mut staged = open_alpm(Some(path_to_str(stage)?))?;
    watch(&staged);
    compare_changes(confirmed, &plan_changes(&mut staged)?)
}

fn commit(
    alpm: &mut Alpm,
    confirmed: Option<&[Change]>,
) -> Result<Result<(), CommitError>, String> {
    alpm.trans_prepare()
        .map_err(|error| describe_prepare_error(&error))?;
    if let Some(confirmed) = confirmed {
        compare_changes(confirmed, &collect_changes(alpm))?;
    }
    let cache_dirs = list_cache_dirs(alpm);
    let package_files = list_package_files(alpm);
    clean_cache(&cache_dirs, &package_files);
    let package_count = alpm.trans_add().len() as u32;
    emit(&Event::Transaction {
        package_count,
        download_bytes: count_download_bytes(&cache_dirs, &package_files),
    });
    if package_count == 0 && alpm.trans_remove().is_empty() {
        return Ok(Ok(()));
    }
    Ok(alpm.trans_commit())
}

fn is_download_error(error: &CommitError) -> bool {
    matches!(
        error.error(),
        Error::PkgNotFound | Error::PkgInvalid | Error::PkgInvalidChecksum | Error::PkgInvalidSig
    )
}

fn describe_commit_error(alpm: &Alpm, error: &CommitError) -> String {
    let mut files: Vec<String> = match error.data() {
        Some(CommitData::PkgInvalid(files)) => files.iter().map(str::to_owned).collect(),
        Some(CommitData::FileConflict(conflicts)) => {
            // alpm 5.0.2 types this list as Conflict; libalpm fills it with alpm_fileconflict_t.
            // FileConflict::conflicting_target reads `target`, so the raw struct is read instead.
            let conflicts: AlpmList<&FileConflict> = unsafe { transmute(conflicts) };
            return describe_lines(
                error,
                conflicts.len(),
                conflicts.iter().map(|conflict| unsafe {
                    describe_file_conflict(&*(conflict as *const FileConflict).cast())
                }),
            );
        }
        None => Vec::new(),
    };
    if files.is_empty() && is_download_error(error) {
        let package_files = list_package_files(alpm);
        files = find_uncached(&list_cache_dirs(alpm), &package_files)
            .into_iter()
            .map(|package_file| package_file.filename.clone())
            .collect();
    }
    if files.is_empty() {
        return error.to_string();
    }
    format!("{error}: {}", files.join(", "))
}

unsafe fn read_c_str(text: *const c_char) -> String {
    if text.is_null() {
        return String::new();
    }
    let mut escaped_text = String::new();
    for character in unsafe { CStr::from_ptr(text) }.to_string_lossy().chars() {
        if character.is_control() {
            escaped_text.extend(character.escape_debug());
        } else {
            escaped_text.push(character);
        }
    }
    escaped_text
}

unsafe fn describe_file_conflict(conflict: &alpm_fileconflict_t) -> String {
    let (target, file, conflicting_target) = unsafe {
        (
            read_c_str(conflict.target),
            read_c_str(conflict.file),
            read_c_str(conflict.ctarget),
        )
    };
    if conflict.type_ == alpm_fileconflicttype_t::ALPM_FILECONFLICT_TARGET {
        format!("{file} exists in both '{target}' and '{conflicting_target}'")
    } else if conflicting_target.is_empty() {
        format!("{target}: {file} exists in filesystem")
    } else {
        format!("{target}: {file} exists in filesystem (owned by {conflicting_target})")
    }
}

fn describe_lines(
    error: impl Display,
    count: usize,
    lines: impl Iterator<Item = String>,
) -> String {
    let mut description = error.to_string();
    for line in lines.take(MAX_CAUSE_LINES) {
        description.push('\n');
        description.push_str(&line);
    }
    if count > MAX_CAUSE_LINES {
        description.push_str(&format!("\nand {} more", count - MAX_CAUSE_LINES));
    }
    description
}

fn describe_prepare_error(error: &PrepareError) -> String {
    match error.data() {
        Some(PrepareData::PkgInvalidArch(packages)) => {
            // alpm 5.0.2 types this list as Package; libalpm fills it with strings.
            let names: AlpmList<&str> = unsafe { transmute(packages) };
            describe_lines(
                error,
                names.len(),
                names
                    .iter()
                    .map(|name| format!("package {name} does not have a valid architecture")),
            )
        }
        Some(PrepareData::UnsatisfiedDeps(missing)) => describe_lines(
            error,
            missing.len(),
            missing.iter().map(|missing| match missing.causing_pkg() {
                Some(causing) => format!(
                    "removing {causing} breaks dependency '{}' required by {}",
                    missing.depend(),
                    missing.target()
                ),
                None => format!(
                    "unable to satisfy dependency '{}' required by {}",
                    missing.depend(),
                    missing.target()
                ),
            }),
        ),
        Some(PrepareData::ConflictingDeps(conflicts)) => describe_lines(
            error,
            conflicts.len(),
            conflicts.iter().map(|conflict| {
                let (first, second) = (conflict.package1(), conflict.package2());
                let packages = format!(
                    "{}-{} and {}-{} are in conflict",
                    first.name(),
                    first.version(),
                    second.name(),
                    second.version()
                );
                match conflict.reason().depmod() {
                    DepMod::Any => packages,
                    _ => format!("{packages} ({})", conflict.reason()),
                }
            }),
        ),
        None => error.to_string(),
    }
}

pub fn install(files: &[&str], flags: TransFlag) -> Result<(), String> {
    let mut alpm = open_alpm(None)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Prepare));
    run_transaction(&mut alpm, flags, |alpm| {
        for file in files {
            let package = alpm
                .pkg_load(*file, true, alpm.local_file_siglevel())
                .map_err(|error| format!("{file}: {error}"))?;
            alpm.trans_add_pkg(package)
                .map_err(|error| format!("{file}: {}", error.error))?;
        }
        commit(alpm, None)?.map_err(|error| describe_commit_error(alpm, &error))
    })
}

pub fn parse_native_install(args: &[String]) -> Option<(Vec<&str>, TransFlag)> {
    let mut is_upgrade = false;
    let mut flags = TransFlag::NONE;
    let mut files = Vec::new();
    let mut in_operands = false;
    for argument in args {
        match argument.as_str() {
            _ if in_operands => files.push(argument.as_str()),
            "--" => in_operands = true,
            "-U" | "--upgrade" => is_upgrade = true,
            "--noconfirm" => {}
            "--needed" => flags |= TransFlag::NEEDED,
            "--asdeps" => flags |= TransFlag::ALL_DEPS,
            "--asexplicit" => flags |= TransFlag::ALL_EXPLICIT,
            flag if flag.starts_with('-') => return None,
            file => files.push(file),
        }
    }
    (is_upgrade && !files.is_empty()).then_some((files, flags))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alpm_sys::alpm_fileconflicttype_t::{
        ALPM_FILECONFLICT_FILESYSTEM, ALPM_FILECONFLICT_TARGET,
    };
    use std::ffi::CString;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn only_a_plain_upgrade_of_files_is_installed_natively() {
        let paru = strings(&[
            "--upgrade",
            "--noconfirm",
            "--noconfirm",
            "--",
            "/a.pkg",
            "/b.pkg",
        ]);
        assert_eq!(
            parse_native_install(&paru),
            Some((vec!["/a.pkg", "/b.pkg"], TransFlag::NONE))
        );
        let dependencies = strings(&["-U", "--asdeps", "--needed", "/c.pkg"]);
        assert_eq!(
            parse_native_install(&dependencies),
            Some((vec!["/c.pkg"], TransFlag::ALL_DEPS | TransFlag::NEEDED))
        );
        assert_eq!(
            parse_native_install(&strings(&["-S", "--asdeps", "cargo"])),
            None
        );
        assert_eq!(
            parse_native_install(&strings(&["-U", "--overwrite", "*", "/a.pkg"])),
            None
        );
        assert_eq!(parse_native_install(&strings(&["-U"])), None);
        assert_eq!(
            parse_native_install(&strings(&["-D", "--asexplicit", "a"])),
            None
        );
    }

    #[test]
    fn file_conflicts_read_as_pacman_prints_them() {
        let nvidia = CString::new("nvidia-utils").unwrap();
        let owner = CString::new("nvidia-open-utils").unwrap();
        let library = CString::new("/usr/lib/libnvidia-eglcore.so.620.70.06").unwrap();
        let no_owner = CString::new("").unwrap();
        let describe = |kind: alpm_fileconflicttype_t, conflicting_target: &CString| unsafe {
            describe_file_conflict(&alpm_fileconflict_t {
                target: nvidia.as_ptr().cast_mut(),
                type_: kind,
                file: library.as_ptr().cast_mut(),
                ctarget: conflicting_target.as_ptr().cast_mut(),
            })
        };
        assert_eq!(
            describe(ALPM_FILECONFLICT_FILESYSTEM, &no_owner),
            "nvidia-utils: /usr/lib/libnvidia-eglcore.so.620.70.06 exists in filesystem"
        );
        assert_eq!(
            describe(ALPM_FILECONFLICT_FILESYSTEM, &owner),
            "nvidia-utils: /usr/lib/libnvidia-eglcore.so.620.70.06 exists in filesystem \
             (owned by nvidia-open-utils)"
        );
        assert_eq!(
            describe(ALPM_FILECONFLICT_TARGET, &owner),
            "/usr/lib/libnvidia-eglcore.so.620.70.06 exists in both 'nvidia-utils' and \
             'nvidia-open-utils'"
        );
        assert_eq!(
            describe_lines(
                Error::FileConflicts,
                1,
                [describe(ALPM_FILECONFLICT_FILESYSTEM, &no_owner)].into_iter()
            ),
            "conflicting files\n\
             nvidia-utils: /usr/lib/libnvidia-eglcore.so.620.70.06 exists in filesystem"
        );
        assert_eq!(
            describe_lines(Error::FileConflicts, 0, std::iter::empty()),
            "conflicting files"
        );
        let forged_path = CString::new("/usr/a\nerror: \u{1b}[1mb").unwrap();
        assert_eq!(
            unsafe { read_c_str(forged_path.as_ptr()) },
            "/usr/a\\nerror: \\u{1b}[1mb"
        );
        let causes = std::iter::repeat_n(String::from("cause"), MAX_CAUSE_LINES + 2);
        let description = describe_lines(Error::FileConflicts, MAX_CAUSE_LINES + 2, causes);
        assert_eq!(description.lines().count(), MAX_CAUSE_LINES + 2);
        assert!(description.ends_with("\ncause\nand 2 more"));
    }
}
