mod commits;
pub mod manager;
mod output;
pub mod packages;
mod reboot;
mod review;
mod runner;
pub mod updateprogress;

use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use crate::core::{config, watch};
use crate::services::updates::updateprogress::UpdateProgress;

pub use commits::Commit;
pub use review::{Notice, Review};
use runner::Session;

pub const REPOSITORY: &str = "The1fEst/proscenio";
const BRANCH: &str = "main";
const HELPER: &str = "/usr/lib/proscenio/proscenio-packages";

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Job {
    System,
    Shell,
}

#[derive(Clone)]
pub struct Updates {
    pub repository_packages: Rc<RefCell<Vec<String>>>,
    pub aur_packages: Rc<RefCell<Vec<String>>>,
    pub has_package_manager: Rc<Cell<bool>>,
    pub has_package_check_failed: Rc<Cell<bool>>,
    pub commits_behind: Rc<Cell<i32>>,
    pub commits: Rc<RefCell<Vec<Commit>>>,
    pub job: Rc<Cell<Option<Job>>>,
    pub is_installing: Rc<Cell<bool>>,
    pub review: Rc<RefCell<Option<Review>>>,
    pub notice: Rc<Cell<Option<Notice>>>,
    session: Rc<RefCell<Option<Rc<Session>>>>,
    start_time_micros: Rc<Cell<i64>>,
    pub last_job_succeeded: Rc<Cell<Option<bool>>>,
    pub log: Rc<RefCell<String>>,
    pub progress: Rc<RefCell<Option<UpdateProgress>>>,
    listeners: Rc<Listeners>,
    output_listeners: Rc<Listeners>,
    config_watch: Rc<RefCell<Option<watch::Watch>>>,
    pub is_refreshing_packages: Rc<Cell<bool>>,
    is_package_refresh_queued: Rc<Cell<bool>>,
    pub is_refreshing_shell: Rc<Cell<bool>>,
    is_shell_refresh_queued: Rc<Cell<bool>>,
}

impl Updates {
    pub fn new() -> Self {
        let updates = Updates {
            repository_packages: Rc::default(),
            aur_packages: Rc::default(),
            has_package_manager: Rc::new(Cell::new(false)),
            has_package_check_failed: Rc::new(Cell::new(false)),
            commits_behind: Rc::new(Cell::new(0)),
            commits: Rc::default(),
            job: Rc::default(),
            is_installing: Rc::default(),
            review: Rc::default(),
            notice: Rc::default(),
            session: Rc::default(),
            start_time_micros: Rc::default(),
            last_job_succeeded: Rc::default(),
            log: Rc::default(),
            progress: Rc::default(),
            listeners: Rc::default(),
            output_listeners: Rc::default(),
            config_watch: Rc::default(),
            is_refreshing_packages: Rc::default(),
            is_package_refresh_queued: Rc::default(),
            is_refreshing_shell: Rc::default(),
            is_shell_refresh_queued: Rc::default(),
        };
        let checker = updates.clone();
        updates
            .config_watch
            .replace(Some(watch::config("/updates/enableCheck", move || {
                if config::current().updates_enable_check {
                    checker.refresh();
                }
            })));
        updates
    }

    pub fn period() -> Duration {
        let config = config::current();
        if !config.updates_enable_check {
            return Duration::ZERO;
        }
        Duration::from_secs((config.updates_interval as u64).max(1) * 60)
    }

    pub fn count(&self) -> i32 {
        (self.repository_packages.borrow().len() + self.aur_packages.borrow().len()) as i32
    }

    pub fn is_upgrade_advised(&self) -> bool {
        self.has_package_manager.get() && self.count() >= config::current().updates_advise
    }

    pub fn is_upgrade_strongly_advised(&self) -> bool {
        self.has_package_manager.get() && self.count() >= config::current().updates_strongly_advise
    }

    pub fn is_shell_behind(&self) -> bool {
        self.commits_behind.get() > 0
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn subscribe_output(&self, listener: impl Fn() + 'static) -> Subscription {
        self.output_listeners.add(listener)
    }

    pub fn refresh(&self) {
        self.refresh_packages();
        self.refresh_shell();
    }

    pub fn check(&self, job: Job) {
        match job {
            Job::System => self.refresh_packages(),
            Job::Shell => self.refresh_shell(),
        }
    }

    pub fn start(&self, job: Job) {
        if self.job.get().is_some() {
            return;
        }
        self.log.borrow_mut().clear();
        self.progress.replace(None);
        self.last_job_succeeded.set(None);
        self.notice.set(None);
        self.review.take();
        self.is_installing.set(false);
        self.start_time_micros.set(glib::monotonic_time());
        if job == Job::System && !self.start_session(job) {
            return;
        }
        self.job.set(Some(job));
        if job == Job::Shell {
            self.review_shell();
        }
        self.listeners.notify();
        self.output_listeners.notify();
    }

    fn push_log(&self, line: &str) {
        {
            let mut log = self.log.borrow_mut();
            log.push_str(line);
            log.push('\n');
        }
        self.output_listeners.notify();
    }
}
