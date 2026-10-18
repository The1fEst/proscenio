use alpm::{Alpm, TransFlag};
use proscenio_protocol::{Change, Event, Stage};
use std::path::Path;

use crate::changes::{collect_changes, compare_changes};
use crate::errors::format_error;
use crate::handle::{open_alpm, watch};
use crate::output::emit;
use crate::stage::{
    STAGE_DIR, SYNC_DIR, find_system_database_path, install_staged_databases, path_to_str,
    prepare_stage,
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

pub fn plan() -> Result<(), String> {
    let stage = prepare_stage(&find_system_database_path()?)?;
    let mut alpm = open_alpm(Some(path_to_str(&stage)?))?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Synchronize));
    alpm.syncdbs_mut().update(false).map_err(format_error)?;
    emit(&Event::Stage(Stage::Prepare));
    emit(&Event::Planned(plan_changes(&mut alpm)?));
    Ok(())
}

pub fn upgrade(confirmed: &[Change]) -> Result<(), String> {
    let database_path = find_system_database_path()?;
    let stage = database_path.join(STAGE_DIR);
    let mut alpm = open_alpm(None)?;
    watch(&alpm);
    emit(&Event::Stage(Stage::Prepare));
    alpm.trans_init(TransFlag::NONE).map_err(format_error)?;
    let result = verify_stage(&stage, confirmed)
        .and_then(|()| {
            install_staged_databases(&stage.join(SYNC_DIR), &database_path.join(SYNC_DIR))
        })
        .and_then(|()| alpm.sync_sysupgrade(false).map_err(format_error))
        .and_then(|()| commit(&mut alpm, Some(confirmed)));
    let _ = alpm.trans_release();
    result
}

fn verify_stage(stage: &Path, confirmed: &[Change]) -> Result<(), String> {
    let mut staged = open_alpm(Some(path_to_str(stage)?))?;
    watch(&staged);
    compare_changes(confirmed, &plan_changes(&mut staged)?)
}

fn commit(alpm: &mut Alpm, confirmed: Option<&[Change]>) -> Result<(), String> {
    alpm.trans_prepare()
        .map_err(|error| format!("{error}: {:?}", error.data()))?;
    if let Some(confirmed) = confirmed {
        compare_changes(confirmed, &collect_changes(alpm))?;
    }
    let package_count = alpm.trans_add().len() as u32;
    let download_bytes = alpm
        .trans_add()
        .iter()
        .map(|package| package.download_size().max(0) as u64)
        .sum();
    emit(&Event::Transaction {
        package_count,
        download_bytes,
    });
    if package_count == 0 && alpm.trans_remove().is_empty() {
        return Ok(());
    }
    alpm.trans_commit()
        .map_err(|error| format!("{error}: {:?}", error.data()))
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
    let result = result.and_then(|()| commit(&mut alpm, None));
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
