use gtk4::gio;
use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::storage;

pub enum Job {
    Write { path: PathBuf, bytes: Vec<u8> },
    Remove { path: PathBuf },
}

impl Job {
    fn path(&self) -> &Path {
        match self {
            Job::Write { path, .. } | Job::Remove { path } => path,
        }
    }

    fn run(self) -> io::Result<()> {
        match self {
            Job::Write { path, bytes } => storage::write_atomic(&path, &bytes),
            Job::Remove { path } => match std::fs::remove_file(path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            },
        }
    }
}

#[derive(Default)]
struct JobQueue {
    jobs: Vec<Job>,
}

impl JobQueue {
    fn push(&mut self, job: Job) {
        self.jobs.retain(|queued| queued.path() != job.path());
        self.jobs.push(job);
    }

    fn take(&mut self) -> Vec<Job> {
        std::mem::take(&mut self.jobs)
    }
}

#[derive(Default)]
pub struct Writer {
    queue: RefCell<JobQueue>,
    is_busy: Cell<bool>,
}

impl Writer {
    pub fn submit(self: &Rc<Self>, job: Job) {
        self.queue.borrow_mut().push(job);
        self.start();
    }

    fn start(self: &Rc<Self>) {
        if self.is_busy.get() {
            return;
        }
        let jobs = self.queue.borrow_mut().take();
        if jobs.is_empty() {
            return;
        }
        self.is_busy.set(true);
        let writer = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(move || {
                for job in jobs {
                    let path = job.path().to_owned();
                    if let Err(error) = job.run() {
                        eprintln!("ERROR: {}: {error}", path.display());
                    }
                }
            })
            .await;
            if let Some(writer) = writer.upgrade() {
                writer.is_busy.set(false);
                writer.start();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &str, text: &str) -> Job {
        Job::Write {
            path: PathBuf::from(path),
            bytes: text.as_bytes().to_vec(),
        }
    }

    fn remove(path: &str) -> Job {
        Job::Remove {
            path: PathBuf::from(path),
        }
    }

    fn describe(jobs: Vec<Job>) -> Vec<String> {
        jobs.into_iter()
            .map(|job| match job {
                Job::Write { path, bytes } => {
                    format!(
                        "write {} {}",
                        path.display(),
                        String::from_utf8(bytes).unwrap()
                    )
                }
                Job::Remove { path } => format!("remove {}", path.display()),
            })
            .collect()
    }

    #[test]
    fn a_job_replaces_the_queued_one_for_the_same_file_only() {
        let mut queue = JobQueue::default();
        queue.push(write("a", "one"));
        queue.push(write("b", "two"));
        queue.push(write("a", "three"));
        queue.push(write("c", "four"));
        queue.push(remove("c"));
        assert_eq!(
            describe(queue.take()),
            ["write b two", "write a three", "remove c"]
        );
        assert!(queue.take().is_empty());
    }

    #[test]
    fn jobs_write_and_remove_files_and_removing_a_missing_file_is_fine() {
        let folder =
            std::env::temp_dir().join(format!("proscenio-notes-jobs-{}", std::process::id()));
        let path = folder.join("1.txt");
        write(path.to_str().unwrap(), "kept").run().unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "kept");
        remove(path.to_str().unwrap()).run().unwrap();
        assert!(!path.exists());
        remove(path.to_str().unwrap()).run().unwrap();
        let _ = std::fs::remove_dir_all(&folder);
    }
}
