use alpm::{Alpm, CommitData, CommitError, Error, TransFlag};
use proscenio_protocol::{Change, Event, Stage};
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

pub fn plan_changes(alpm: &mut Alpm) -> Result<Vec<Change>, String> {
    alpm.trans_init(TransFlag::NONE).map_err(format_error)?;
    let result = alpm
        .sync_sysupgrade(false)
        .map_err(format_error)
        .and_then(|()| {
            alpm.trans_prepare()
                .map_err(|error| format!("{error}: {:?}", error.data()))
        })
        .map(|()| collect_changes(alpm));
    let _ = alpm.trans_release();
    result
}

pub fn upgrade(confirmed: &[Change], private_path: &Path, uid: u32) -> Result<(), String> {
    let system_path = find_system_database_path()?;
    let system_sync = system_path.join(SYNC_DIR);
    let mut alpm = open_alpm(None)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Prepare));
    alpm.trans_init(TransFlag::NONE).map_err(format_error)?;
    let result = prepare_stage(&system_path)
        .and_then(|stage| {
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
            install_staged_databases(&staged_sync, &system_sync)
        })
        .and_then(|()| upgrade_packages(&mut alpm, confirmed));
    let _ = alpm.trans_release();
    result
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
        .map_err(|error| format!("{error}: {:?}", error.data()))?;
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
        Some(data) => return format!("{error}: {data:?}"),
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

pub fn install(files: &[&str], flags: TransFlag) -> Result<(), String> {
    let mut alpm = open_alpm(None)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Prepare));
    alpm.trans_init(flags).map_err(format_error)?;
    let mut result = Ok(());
    for file in files {
        let load_result = alpm
            .pkg_load(*file, true, alpm.local_file_siglevel())
            .map_err(|error| format!("{file}: {error}"));
        if let Err(error) = load_result.and_then(|package| {
            alpm.trans_add_pkg(package)
                .map_err(|error| format!("{file}: {}", error.error))
        }) {
            result = Err(error);
            break;
        }
    }
    let result = result
        .and_then(|()| commit(&mut alpm, None))
        .and_then(|result| result.map_err(|error| describe_commit_error(&alpm, &error)));
    let _ = alpm.trans_release();
    result
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
}
