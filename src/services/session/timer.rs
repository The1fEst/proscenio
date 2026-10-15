use gtk4::glib;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::core::config;
use crate::core::i18n::{tr, trn};
use crate::core::listeners::{Listeners, Subscription};
use crate::core::persistent;
use crate::core::watch;
use crate::platform::dbus::notify::{self, Notification};
use crate::services::media::audio;

const POMODORO_INTERVAL: Duration = Duration::from_millis(200);
const STOPWATCH_INTERVAL: Duration = Duration::from_millis(10);
const DAY_SECONDS: i64 = 24 * 60 * 60;

struct State {
    lap: Cell<i64>,
    is_pomodoro_running: Cell<bool>,
    is_pomodoro_break: Cell<bool>,
    pomodoro_start: Cell<i64>,
    pomodoro_cycle: Cell<i64>,
    seconds_left: Cell<i64>,
    is_stopwatch_running: Cell<bool>,
    stopwatch_start: Cell<i64>,
    stopwatch_time: Cell<i64>,
    laps: RefCell<Vec<i64>>,
    pomodoro_ticker: RefCell<Option<glib::SourceId>>,
    stopwatch_ticker: RefCell<Option<glib::SourceId>>,
    listeners: Listeners,
}

#[derive(Clone)]
pub struct Timer {
    state: Rc<State>,
}

impl Timer {
    pub fn new() -> Self {
        let pomodoro = persistent::read(&["timer", "pomodoro"]).unwrap_or(Value::Null);
        let stopwatch = persistent::read(&["timer", "stopwatch"]).unwrap_or(Value::Null);
        let integer = |node: &Value, key: &str| node.get(key).and_then(Value::as_i64).unwrap_or(0);
        let flag =
            |node: &Value, key: &str| node.get(key).and_then(Value::as_bool).unwrap_or(false);
        let timer = Timer {
            state: Rc::new(State {
                lap: Cell::new(0),
                is_pomodoro_running: Cell::new(flag(&pomodoro, "running")),
                is_pomodoro_break: Cell::new(flag(&pomodoro, "isBreak")),
                pomodoro_start: Cell::new(clamp_saved(
                    integer(&pomodoro, "start"),
                    seconds(),
                    DAY_SECONDS,
                )),
                pomodoro_cycle: Cell::new(clamp_cycle(
                    integer(&pomodoro, "cycle"),
                    config::current().pomodoro_cycles,
                )),
                seconds_left: Cell::new(0),
                is_stopwatch_running: Cell::new(flag(&stopwatch, "running")),
                stopwatch_start: Cell::new(clamp_saved(
                    integer(&stopwatch, "start"),
                    centiseconds(),
                    0,
                )),
                stopwatch_time: Cell::new(0),
                laps: RefCell::new(read_laps(&stopwatch, centiseconds())),
                pomodoro_ticker: RefCell::new(None),
                stopwatch_ticker: RefCell::new(None),
                listeners: Listeners::default(),
            }),
        };
        timer.state.lap.set(timer.lap_duration());
        timer.state.seconds_left.set(timer.lap_duration());
        std::mem::forget(watch::config("/time/pomodoro", {
            let timer = timer.clone();
            move || timer.follow_config()
        }));
        if timer.is_stopwatch_running() {
            timer.tick_stopwatch();
        } else {
            timer.reset_stopwatch();
        }
        if timer.is_pomodoro_running() {
            timer.tick_pomodoro();
        }
        timer
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.state.listeners.add(listener)
    }

    pub fn focus_time(&self) -> i64 {
        config::current().pomodoro_focus
    }

    pub fn is_pomodoro_running(&self) -> bool {
        self.state.is_pomodoro_running.get()
    }

    pub fn is_pomodoro_break(&self) -> bool {
        self.state.is_pomodoro_break.get()
    }

    pub fn is_pomodoro_long_break(&self) -> bool {
        self.is_pomodoro_break() && self.pomodoro_cycle() + 1 == config::current().pomodoro_cycles
    }

    pub fn pomodoro_cycle(&self) -> i64 {
        self.state.pomodoro_cycle.get()
    }

    pub fn lap_duration(&self) -> i64 {
        let config = config::current();
        if self.is_pomodoro_long_break() {
            config.pomodoro_long_break
        } else if self.is_pomodoro_break() {
            config.pomodoro_break
        } else {
            config.pomodoro_focus
        }
    }

    fn follow_config(&self) {
        let state = &self.state;
        let is_lap_fresh =
            !self.is_pomodoro_running() && state.seconds_left.get() == state.lap.get();
        state.lap.set(self.lap_duration());
        if is_lap_fresh {
            state.seconds_left.set(state.lap.get());
            self.announce();
        }
    }

    pub fn seconds_left(&self) -> i64 {
        self.state.seconds_left.get()
    }

    pub fn is_stopwatch_running(&self) -> bool {
        self.state.is_stopwatch_running.get()
    }

    pub fn stopwatch_time(&self) -> i64 {
        self.state.stopwatch_time.get()
    }

    pub fn laps(&self) -> Vec<i64> {
        self.state.laps.borrow().clone()
    }

    pub fn toggle_pomodoro(&self) {
        let state = &self.state;
        let is_running = !state.is_pomodoro_running.get();
        state.is_pomodoro_running.set(is_running);
        if is_running {
            state
                .pomodoro_start
                .set(seconds() + self.seconds_left() - self.lap_duration());
            self.tick_pomodoro();
        } else {
            stop(&state.pomodoro_ticker);
        }
        self.save_pomodoro();
        self.announce();
    }

    pub fn reset_pomodoro(&self) {
        let state = &self.state;
        state.is_pomodoro_running.set(false);
        state.is_pomodoro_break.set(false);
        state.pomodoro_start.set(seconds());
        state.pomodoro_cycle.set(0);
        stop(&state.pomodoro_ticker);
        self.save_pomodoro();
        self.refresh_pomodoro();
    }

    pub fn toggle_stopwatch(&self) {
        if self.is_stopwatch_running() {
            self.pause_stopwatch();
        } else {
            self.resume_stopwatch();
        }
    }

    pub fn reset_stopwatch(&self) {
        let state = &self.state;
        state.stopwatch_time.set(0);
        state.laps.borrow_mut().clear();
        state.is_stopwatch_running.set(false);
        stop(&state.stopwatch_ticker);
        self.save_stopwatch();
        self.announce();
    }

    pub fn record_lap(&self) {
        self.state.laps.borrow_mut().push(self.stopwatch_time());
        self.save_stopwatch();
        self.announce();
    }

    fn pause_stopwatch(&self) {
        self.state.is_stopwatch_running.set(false);
        stop(&self.state.stopwatch_ticker);
        self.save_stopwatch();
        self.announce();
    }

    fn resume_stopwatch(&self) {
        let state = &self.state;
        if state.stopwatch_time.get() == 0 {
            state.laps.borrow_mut().clear();
        }
        state.is_stopwatch_running.set(true);
        state
            .stopwatch_start
            .set(centiseconds() - state.stopwatch_time.get());
        self.save_stopwatch();
        self.tick_stopwatch();
        self.announce();
    }

    fn refresh_pomodoro(&self) {
        let state = &self.state;
        let now = seconds();
        if now >= state.pomodoro_start.get() + self.lap_duration() {
            state.is_pomodoro_break.set(!state.is_pomodoro_break.get());
            state.pomodoro_start.set(now);

            let config = config::current();
            let message = if self.is_pomodoro_long_break() {
                trn(
                    "🌿 Long break: %1 minute",
                    "🌿 Long break: %1 minutes",
                    (config.pomodoro_long_break / 60) as u64,
                )
            } else if self.is_pomodoro_break() {
                trn(
                    "☕ Break: %1 minute",
                    "☕ Break: %1 minutes",
                    (config.pomodoro_break / 60) as u64,
                )
            } else {
                trn(
                    "🔴 Focus: %1 minute",
                    "🔴 Focus: %1 minutes",
                    (config.pomodoro_focus / 60) as u64,
                )
            };
            notify::send(&Notification {
                app: "Shell",
                summary: &tr("Pomodoro"),
                body: &message,
                ..Default::default()
            });
            if config.sounds_pomodoro {
                audio::play_system_sound(&config.sounds_theme, "alarm-clock-elapsed");
            }

            if !self.is_pomodoro_break() {
                state
                    .pomodoro_cycle
                    .set((state.pomodoro_cycle.get() + 1) % config.pomodoro_cycles);
            }
            self.save_pomodoro();
        }
        state.lap.set(self.lap_duration());
        state
            .seconds_left
            .set(state.lap.get() - (now - state.pomodoro_start.get()));
        self.announce();
    }

    fn tick_pomodoro(&self) {
        let state = Rc::downgrade(&self.state);
        let source = glib::timeout_add_local(POMODORO_INTERVAL, move || {
            with(&state, Timer::refresh_pomodoro)
        });
        replace(&self.state.pomodoro_ticker, source);
    }

    fn tick_stopwatch(&self) {
        let state = Rc::downgrade(&self.state);
        let source = glib::timeout_add_local(STOPWATCH_INTERVAL, move || {
            with(&state, |timer| {
                let state = &timer.state;
                state
                    .stopwatch_time
                    .set(centiseconds() - state.stopwatch_start.get());
                timer.announce();
            })
        });
        replace(&self.state.stopwatch_ticker, source);
    }

    fn save_pomodoro(&self) {
        let state = &self.state;
        persistent::write(
            &["timer", "pomodoro"],
            json!({
                "running": state.is_pomodoro_running.get(),
                "start": state.pomodoro_start.get(),
                "isBreak": state.is_pomodoro_break.get(),
                "cycle": state.pomodoro_cycle.get(),
            }),
        );
    }

    fn save_stopwatch(&self) {
        let state = &self.state;
        persistent::write(
            &["timer", "stopwatch"],
            json!({
                "running": state.is_stopwatch_running.get(),
                "start": state.stopwatch_start.get(),
                "laps": *state.laps.borrow(),
            }),
        );
    }

    fn announce(&self) {
        self.state.listeners.notify();
    }
}

fn with(state: &Weak<State>, action: impl Fn(&Timer)) -> glib::ControlFlow {
    let Some(state) = state.upgrade() else {
        return glib::ControlFlow::Break;
    };
    action(&Timer { state });
    glib::ControlFlow::Continue
}

fn replace(ticker: &RefCell<Option<glib::SourceId>>, source: glib::SourceId) {
    if let Some(previous) = ticker.replace(Some(source)) {
        previous.remove();
    }
}

fn stop(ticker: &RefCell<Option<glib::SourceId>>) {
    if let Some(source) = ticker.take() {
        source.remove();
    }
}

fn clamp_cycle(cycle: i64, cycles: i64) -> i64 {
    cycle.clamp(0, cycles - 1)
}

fn clamp_saved(time: i64, now: i64, window: i64) -> i64 {
    time.clamp(0, now + window)
}

fn read_laps(stopwatch: &Value, now: i64) -> Vec<i64> {
    stopwatch
        .get("laps")
        .and_then(Value::as_array)
        .map(|laps| {
            laps.iter()
                .filter_map(Value::as_i64)
                .map(|lap| clamp_saved(lap, now, 0))
                .collect()
        })
        .unwrap_or_default()
}

fn millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as i64)
        .unwrap_or(0)
}

fn seconds() -> i64 {
    millis() / 1000
}

fn centiseconds() -> i64 {
    millis() / 10
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_cycle_outside_the_configured_cycles_is_clamped() {
        assert_eq!(clamp_cycle(2, 4), 2);
        assert_eq!(clamp_cycle(7, 4), 3);
        assert_eq!(clamp_cycle(i64::MAX, 4), 3);
        assert_eq!(clamp_cycle(-6, 4), 0);
        assert_eq!(clamp_cycle(5, 1), 0);
    }

    #[test]
    fn a_saved_start_more_than_a_day_ahead_or_before_the_epoch_is_clamped() {
        let now = 1_760_000_000;
        assert_eq!(clamp_saved(now - 300, now, DAY_SECONDS), now - 300);
        assert_eq!(clamp_saved(now + 300, now, DAY_SECONDS), now + 300);
        assert_eq!(clamp_saved(i64::MAX, now, DAY_SECONDS), now + DAY_SECONDS);
        assert_eq!(clamp_saved(i64::MIN, now, DAY_SECONDS), 0);
    }

    #[test]
    fn saved_laps_are_held_between_zero_and_now() {
        let now = 176_000_000_000;
        let stopwatch = json!({ "laps": [i64::MIN, 1500, 4200, i64::MAX] });
        assert_eq!(read_laps(&stopwatch, now), [0, 1500, 4200, now]);
    }
}
