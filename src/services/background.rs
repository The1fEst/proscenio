use gtk4::glib;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, Instant};

use crate::core::config;
use crate::core::listeners::{Subscription, Unsubscribe};

type Run = Box<dyn Fn() -> Result<(), String>>;
type Period = Box<dyn Fn() -> Duration>;

struct Task {
    id: u64,
    name: &'static str,
    period: Option<Period>,
    last: Cell<Option<Instant>>,
    run: Run,
}

impl Task {
    fn due(&self, now: Instant) -> bool {
        let (Some(period), Some(last)) = (self.period.as_ref(), self.last.get()) else {
            return true;
        };
        now.duration_since(last) >= period()
    }
}

#[derive(Default)]
pub struct BackgroundTasks {
    tasks: RefCell<Vec<Rc<Task>>>,
    next: Cell<u64>,
}

impl Unsubscribe for BackgroundTasks {
    fn remove(&self, id: u64) {
        self.tasks.borrow_mut().retain(|task| task.id != id);
    }
}

impl BackgroundTasks {
    pub fn start() -> Rc<Self> {
        let tasks = Rc::new(BackgroundTasks::default());
        let first = Rc::downgrade(&tasks);
        glib::idle_add_local_once(move || {
            if let Some(tasks) = first.upgrade() {
                tasks.tick();
            }
        });
        tasks
    }

    pub fn add(&self, name: &'static str, run: impl Fn() -> Result<(), String> + 'static) {
        self.push(name, None, Box::new(run));
    }

    pub fn add_every(
        &self,
        name: &'static str,
        period: impl Fn() -> Duration + 'static,
        run: impl Fn() -> Result<(), String> + 'static,
    ) {
        self.push(name, Some(Box::new(period)), Box::new(run));
    }

    pub fn add_scoped(
        self: &Rc<Self>,
        name: &'static str,
        run: impl Fn() -> Result<(), String> + 'static,
    ) -> Subscription {
        let id = self.push(name, None, Box::new(run));
        let tasks: Weak<BackgroundTasks> = Rc::downgrade(self);
        Subscription::new(tasks, id)
    }

    fn push(&self, name: &'static str, period: Option<Period>, run: Run) -> u64 {
        let id = self.next.get();
        self.next.set(id + 1);
        self.tasks.borrow_mut().push(Rc::new(Task {
            id,
            name,
            period,
            last: Cell::new(None),
            run,
        }));
        id
    }

    fn tick(self: &Rc<Self>) {
        self.run_due(Instant::now());
        let pause = Duration::from_millis(config::current().update_interval.max(100) as u64);
        let next = Rc::downgrade(self);
        glib::timeout_add_local_once(pause, move || {
            if let Some(tasks) = next.upgrade() {
                tasks.tick();
            }
        });
    }

    fn run_due(&self, now: Instant) {
        let tasks: Vec<Rc<Task>> = self.tasks.borrow().clone();
        for task in tasks.iter().filter(|task| task.due(now)) {
            task.last.set(Some(now));
            if let Err(error) = (task.run)() {
                eprintln!("background: {} failed: {error}", task.name);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_failing_task_does_not_keep_the_others_from_running() {
        let tasks = BackgroundTasks::default();
        let ran = Rc::new(Cell::new(0));
        tasks.add("broken", || Err("no /proc".to_owned()));
        tasks.add("counter", {
            let ran = ran.clone();
            move || {
                ran.set(ran.get() + 1);
                Ok(())
            }
        });
        let start = Instant::now();
        tasks.run_due(start);
        tasks.run_due(start + Duration::from_secs(3));
        assert_eq!(ran.get(), 2);
    }

    #[test]
    fn a_task_with_a_period_runs_at_once_and_then_only_when_the_period_is_over() {
        let tasks = BackgroundTasks::default();
        let ran = Rc::new(Cell::new(0));
        tasks.add_every("weather", || Duration::from_secs(600), {
            let ran = ran.clone();
            move || {
                ran.set(ran.get() + 1);
                Ok(())
            }
        });
        let start = Instant::now();
        let runs: Vec<i32> = [0, 3, 597, 600, 603, 1200]
            .iter()
            .map(|seconds| {
                tasks.run_due(start + Duration::from_secs(*seconds));
                ran.get()
            })
            .collect();
        assert_eq!(runs, vec![1, 1, 1, 2, 2, 3]);
    }

    #[test]
    fn a_scoped_task_stops_with_its_subscription_and_lets_go_of_what_it_holds() {
        let tasks = Rc::new(BackgroundTasks::default());
        let ran = Rc::new(Cell::new(0));
        let held = Rc::new(());
        let subscription = tasks.add_scoped("widget", {
            let ran = ran.clone();
            let held = held.clone();
            move || {
                let _ = &held;
                ran.set(ran.get() + 1);
                Ok(())
            }
        });
        let start = Instant::now();
        tasks.run_due(start);
        drop(subscription);
        tasks.run_due(start + Duration::from_secs(3));
        assert_eq!((ran.get(), Rc::strong_count(&held)), (1, 1));
    }

    #[test]
    fn a_task_may_add_another_while_the_loop_runs() {
        let tasks = Rc::new(BackgroundTasks::default());
        let ran = Rc::new(Cell::new(false));
        tasks.add("adder", {
            let weak = Rc::downgrade(&tasks);
            let ran = ran.clone();
            move || {
                if let Some(tasks) = weak.upgrade() {
                    let ran = ran.clone();
                    tasks.add("late", move || {
                        ran.set(true);
                        Ok(())
                    });
                }
                Ok(())
            }
        });
        let start = Instant::now();
        tasks.run_due(start);
        assert!(!ran.get());
        tasks.run_due(start + Duration::from_secs(3));
        assert!(ran.get());
    }
}
