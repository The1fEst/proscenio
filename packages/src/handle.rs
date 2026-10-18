use alpm::{
    Alpm, AnyDownloadEvent, AnyEvent, AnyQuestion, DownloadEvent, Event as AlpmEvent, HookWhen,
    LogLevel, Progress, Question,
};
use proscenio_protocol::{Event, Stage};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::errors::format_error;
use crate::output::emit;

pub const ALPM_LOCK: &str = "db.lck";
const DOWNLOAD_STEPS: u64 = 200;
const SYSTEM_HOOK_DIR: &str = "/usr/share/libalpm/hooks/";

#[derive(Default)]
struct Downloads {
    files: HashMap<String, (u64, u64)>,
    expected_bytes: u64,
    reported_bytes: u64,
}

pub fn open_alpm(db_path: Option<&str>) -> Result<Alpm, String> {
    let pacman_config = pacmanconf::Config::new().map_err(format_error)?;
    let db_path = db_path.unwrap_or(&pacman_config.db_path);
    let mut alpm = Alpm::new(&*pacman_config.root_dir, db_path).map_err(format_error)?;
    alpm_utils::configure_alpm(&mut alpm, &pacman_config).map_err(format_error)?;
    alpm.set_hookdirs(list_hook_dirs(&pacman_config.hook_dir).iter())
        .map_err(format_error)?;
    Ok(alpm)
}

fn list_hook_dirs(configured_dirs: &[String]) -> Vec<String> {
    let mut hook_dirs = vec![SYSTEM_HOOK_DIR.to_owned()];
    hook_dirs.extend(
        configured_dirs
            .iter()
            .filter(|dir| dir.as_str() != SYSTEM_HOOK_DIR)
            .cloned(),
    );
    hook_dirs
}

pub fn watch(alpm: &Alpm) {
    let downloads = Rc::new(RefCell::new(Downloads::default()));
    alpm.set_dl_cb(
        downloads.clone(),
        |file: &str, event: AnyDownloadEvent, downloads| {
            let mut downloads = downloads.borrow_mut();
            let entry = downloads.files.entry(file.to_owned()).or_default();
            match event.event() {
                DownloadEvent::Init(_) => *entry = (0, 0),
                DownloadEvent::Progress(progress) => {
                    *entry = (
                        progress.downloaded.max(0) as u64,
                        progress.total.max(0) as u64,
                    );
                }
                DownloadEvent::Completed(completion) => {
                    let total = completion.total.max(0) as u64;
                    *entry = (total, total);
                }
                DownloadEvent::Retry(_) => {}
            }
            let received_bytes: u64 = downloads.files.values().map(|(received, _)| received).sum();
            let file_total_bytes: u64 = downloads.files.values().map(|(_, total)| total).sum();
            let total_bytes = file_total_bytes.max(downloads.expected_bytes);
            let step = (total_bytes / DOWNLOAD_STEPS).max(1);
            if received_bytes < downloads.reported_bytes + step && received_bytes < total_bytes {
                return;
            }
            downloads.reported_bytes = received_bytes;
            emit(&Event::Download {
                received_bytes,
                total_bytes,
            });
        },
    );
    alpm.set_event_cb(downloads, |event: AnyEvent, downloads| {
        match event.event() {
            AlpmEvent::RetrieveStart => *downloads.borrow_mut() = Downloads::default(),
            AlpmEvent::PkgRetrieveStart(retrieval) => {
                *downloads.borrow_mut() = Downloads {
                    expected_bytes: retrieval.total_size().max(0) as u64,
                    ..Downloads::default()
                };
                emit(&Event::Stage(Stage::Download));
            }
            AlpmEvent::TransactionStart => emit(&Event::Stage(Stage::Install)),
            AlpmEvent::HookStart(hook) if hook.when() == HookWhen::PostTransaction => {
                emit(&Event::Stage(Stage::Hooks));
            }
            AlpmEvent::HookRunStart(hook) => emit(&Event::Hook {
                index: hook.position() as u32,
                count: hook.total() as u32,
                name: hook.desc().unwrap_or(hook.name()).to_owned(),
            }),
            AlpmEvent::ScriptletInfo(info) => emit(&Event::Line(info.line().trim_end().to_owned())),
            AlpmEvent::PacnewCreated(pacnew_event) => emit(&Event::Line(format!(
                "{} installed as {}.pacnew",
                pacnew_event.file(),
                pacnew_event.file()
            ))),
            AlpmEvent::PacsaveCreated(pacsave_event) => emit(&Event::Line(format!(
                "{} saved as {}.pacsave",
                pacsave_event.file(),
                pacsave_event.file()
            ))),
            _ => {}
        }
    });
    alpm.set_progress_cb(
        (),
        |progress: Progress, name: &str, percent: i32, count: usize, index: usize, _| {
            if matches!(
                progress,
                Progress::AddStart
                    | Progress::UpgradeStart
                    | Progress::DowngradeStart
                    | Progress::ReinstallStart
                    | Progress::RemoveStart
            ) {
                emit(&Event::Package {
                    index: index as u32,
                    count: count as u32,
                    name: name.to_owned(),
                    percent: percent.clamp(0, 100) as u32,
                });
            }
        },
    );
    alpm.set_log_cb((), |level: LogLevel, message: &str, _| {
        let prefix = if level.contains(LogLevel::ERROR) {
            "error"
        } else if level.contains(LogLevel::WARNING) {
            "warning"
        } else {
            return;
        };
        emit(&Event::Line(format!("{prefix}: {}", message.trim_end())));
    });
    alpm.set_question_cb((), |question: AnyQuestion, _| match question.question() {
        Question::InstallIgnorepkg(mut ignore_question) => ignore_question.set_install(false),
        Question::Replace(replace_question) => replace_question.set_replace(true),
        Question::Conflict(mut conflict_question) => conflict_question.set_remove(false),
        Question::Corrupted(mut corruption_question) => corruption_question.set_remove(true),
        Question::RemovePkgs(mut removal_question) => removal_question.set_skip(false),
        Question::SelectProvider(mut provider_question) => provider_question.set_index(0),
        Question::ImportKey(mut key_question) => key_question.set_import(true),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| item.to_string()).collect()
    }

    #[test]
    fn transactions_run_the_system_hooks_before_the_configured_ones_as_pacman_does() {
        assert_eq!(
            list_hook_dirs(&strings(&["/etc/pacman.d/hooks/"])),
            strings(&[SYSTEM_HOOK_DIR, "/etc/pacman.d/hooks/"])
        );
        assert_eq!(
            list_hook_dirs(&strings(&[SYSTEM_HOOK_DIR, "/etc/pacman.d/hooks/"])),
            strings(&[SYSTEM_HOOK_DIR, "/etc/pacman.d/hooks/"])
        );
    }
}
