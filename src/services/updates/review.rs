use gtk4::{gio, glib};
use proscenio_protocol::Change;
use std::path::Path;
use std::rc::Rc;

use super::packages::{LOCAL_DATABASE, version_change};
use super::reboot::find_reboot_packages;
use super::{Job, Session, Updates};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    UpToDate,
    CheckFailed,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Review {
    pub changes: Vec<Change>,
    pub aur_packages: Vec<String>,
    pub reboot_packages: Vec<String>,
}

impl Review {
    pub fn count(&self) -> usize {
        self.changes.len() + self.aur_packages.len()
    }
}

fn choose_commit_notice(is_loaded: bool, commit_count: usize) -> Option<Notice> {
    if !is_loaded {
        return Some(Notice::CheckFailed);
    }
    (commit_count == 0).then_some(Notice::UpToDate)
}

impl Updates {
    pub(super) fn review_shell(&self) {
        let updates = self.clone();
        glib::spawn_future_local(async move {
            let is_loaded = updates.load_commits().await;
            if updates.job.get() != Some(Job::Shell) || updates.is_installing.get() {
                return;
            }
            let commit_count = updates.commits.borrow().len();
            match choose_commit_notice(is_loaded, commit_count) {
                Some(notice) => {
                    updates.notice.set(Some(notice));
                    updates.abandon();
                }
                None => {
                    updates.review.replace(Some(Review::default()));
                    updates.listeners.notify();
                }
            }
        });
    }

    pub(super) fn open_review(
        &self,
        session: &Rc<Session>,
        changes: Vec<Change>,
        aur_packages: Vec<String>,
    ) {
        let installed_packages: Vec<(String, String)> = changes
            .iter()
            .filter(|change| !change.to.is_empty())
            .map(|change| (change.name.clone(), change.from.clone()))
            .chain(
                aur_packages
                    .iter()
                    .filter_map(|line| version_change(line))
                    .map(|(name, from, _)| (name.to_owned(), from.to_owned())),
            )
            .collect();
        let updates = self.clone();
        let session = session.clone();
        glib::spawn_future_local(async move {
            let reboot_packages = gio::spawn_blocking(move || {
                find_reboot_packages(Path::new(LOCAL_DATABASE), &installed_packages)
            })
            .await
            .unwrap_or_default();
            let is_current = updates
                .session
                .borrow()
                .as_ref()
                .is_some_and(|current| Rc::ptr_eq(current, &session));
            if !is_current || !session.is_open() {
                return;
            }
            updates.review.replace(Some(Review {
                changes,
                aur_packages,
                reboot_packages,
            }));
            updates.listeners.notify();
        });
    }

    pub fn confirm(&self) {
        let Some(review) = self.review.take() else {
            return;
        };
        self.is_installing.set(true);
        self.start_time_micros.set(glib::monotonic_time());
        let job = self.job.get();
        if job == Some(Job::Shell) && !self.start_session(Job::Shell) {
            self.push_log("Could not start the update");
            self.last_job_succeeded.set(Some(false));
            self.abandon();
            return;
        }
        let session = self.session.borrow().clone();
        if let Some(session) = session.filter(|_| job == Some(Job::System)) {
            self.install_plan(&session, review.changes, &review.aur_packages);
        }
        self.listeners.notify();
        self.output_listeners.notify();
    }

    pub fn cancel(&self) {
        if self.job.get().is_none() || self.is_installing.get() {
            return;
        }
        self.review.take();
        let session = self.session.borrow().clone();
        match session {
            Some(session) => session.stop_early(),
            None => self.abandon(),
        }
        self.listeners.notify();
    }

    pub(super) fn abandon(&self) {
        self.session.take();
        self.review.take();
        self.is_installing.set(false);
        self.job.set(None);
        self.listeners.notify();
        self.output_listeners.notify();
        if self.is_package_refresh_queued.get() {
            self.refresh_packages();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shell_refresh_opens_no_dialog_when_it_failed_or_found_no_commit() {
        assert_eq!(choose_commit_notice(false, 0), Some(Notice::CheckFailed));
        assert_eq!(choose_commit_notice(false, 5), Some(Notice::CheckFailed));
        assert_eq!(choose_commit_notice(true, 0), Some(Notice::UpToDate));
        assert_eq!(choose_commit_notice(true, 3), None);
    }
}
