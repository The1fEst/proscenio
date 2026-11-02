pub mod civil;
mod days;
mod eds;
mod event;
mod recurrence;
mod sources;
mod time;
mod vevent;

use gtk4::gio;
use gtk4::glib;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use crate::core::config;
use crate::core::i18n::tr;
use crate::core::listeners::{Listeners, Subscription};
use civil::SECONDS_PER_DAY;
pub use days::{Occurrence, Window};
use sources::Source;
pub use sources::Target;
use time::Zones;
pub use vevent::{Draft, Slot, make_excerpt};

const REFRESH_DELAY_MILLIS: u64 = 300;

struct Connection {
    uid: String,
    color: Option<String>,
    calendar: Option<eds::Calendar>,
    watch: Option<eds::Watch>,
    occurrences: Vec<Occurrence>,
    is_refresh_pending: bool,
}

struct Request {
    calendar: eds::Calendar,
    color: Option<String>,
    window: Window,
    expression: String,
}

struct State {
    viewer_count: usize,
    is_running: bool,
    generation: u64,
    window: Window,
    is_available: bool,
    connections: Vec<Connection>,
    cached_occurrences: HashMap<String, Vec<Occurrence>>,
    by_day: HashMap<i64, Vec<Occurrence>>,
}

impl State {
    fn rebuild(&mut self) {
        let all_occurrences: Vec<Occurrence> = self
            .connections
            .iter()
            .flat_map(|connection| connection.occurrences.iter().cloned())
            .collect();
        self.by_day = days::bucket(&all_occurrences, self.window);
    }
}

pub struct Agenda {
    bus: Option<gio::DBusConnection>,
    state: RefCell<State>,
    listeners: Listeners,
}

impl Agenda {
    pub fn new(bus: Option<gio::DBusConnection>) -> Rc<Agenda> {
        Rc::new(Agenda {
            bus,
            state: RefCell::new(State {
                viewer_count: 0,
                is_running: false,
                generation: 0,
                window: Window {
                    first_day: 0,
                    end_day: 0,
                },
                is_available: false,
                connections: Vec::new(),
                cached_occurrences: HashMap::new(),
                by_day: HashMap::new(),
            }),
            listeners: Listeners::default(),
        })
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn is_available(&self) -> bool {
        config::current().should_show_calendar_events && self.state.borrow().is_available
    }

    pub async fn read_targets(&self) -> Vec<Target> {
        let Some(bus) = &self.bus else {
            return Vec::new();
        };
        eds::read_targets(bus).await.unwrap_or_default()
    }

    pub async fn add_event(&self, calendar_uid: &str, draft: &Draft) -> Result<(), String> {
        let Some(bus) = &self.bus else {
            return Err(tr("No calendar can take events"));
        };
        let component = vevent::render(
            draft,
            &Zones::local(),
            &glib::uuid_string_random(),
            glib::real_time() / 1_000_000,
        );
        eds::create_object(bus, calendar_uid, &component).await
    }

    pub fn occurrences_on(&self, day: i64) -> Vec<Occurrence> {
        self.state
            .borrow()
            .by_day
            .get(&day)
            .cloned()
            .unwrap_or_default()
    }

    pub fn activate(self: &Rc<Self>, window: Window) {
        let mut state = self.state.borrow_mut();
        state.viewer_count += 1;
        if state.is_running {
            drop(state);
            self.set_window(window);
            return;
        }
        state.window = window;
        if self.bus.is_none() || !config::current().should_show_calendar_events {
            state.cached_occurrences.clear();
            state.by_day.clear();
            return;
        }
        state.is_running = true;
        state.generation += 1;
        let generation = state.generation;
        drop(state);
        let agenda = self.clone();
        glib::spawn_future_local(async move { agenda.load(generation).await });
    }

    pub fn deactivate(&self) {
        let mut state = self.state.borrow_mut();
        state.viewer_count = state.viewer_count.saturating_sub(1);
        if state.viewer_count > 0 {
            return;
        }
        state.is_running = false;
        state.generation += 1;
        let mut connections = std::mem::take(&mut state.connections);
        if !connections.is_empty() {
            state.cached_occurrences = connections
                .iter_mut()
                .map(|connection| {
                    (
                        connection.uid.clone(),
                        std::mem::take(&mut connection.occurrences),
                    )
                })
                .collect();
        }
        drop(state);
        let Some(bus) = &self.bus else {
            return;
        };
        for connection in connections {
            if let Some(calendar) = connection.calendar {
                eds::release(bus, &calendar, connection.watch);
            }
        }
    }

    pub fn set_window(self: &Rc<Self>, window: Window) {
        let mut state = self.state.borrow_mut();
        if state.window == window {
            return;
        }
        state.window = window;
        let generation = state.generation;
        let opened_indices: Vec<usize> = state
            .connections
            .iter()
            .enumerate()
            .filter(|(_, connection)| connection.calendar.is_some())
            .map(|(index, _)| index)
            .collect();
        drop(state);
        for index in opened_indices {
            let agenda = self.clone();
            glib::spawn_future_local(async move {
                agenda.refresh(index, generation).await;
                agenda.watch(index, generation).await;
            });
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.state.borrow().generation == generation
    }

    async fn load(self: Rc<Self>, generation: u64) {
        let Some(bus) = self.bus.clone() else {
            return;
        };
        let sources: Vec<Source> = eds::read_sources(&bus).await.unwrap_or_default();
        if !self.is_current(generation) {
            return;
        }
        let count = sources.len();
        {
            let mut state = self.state.borrow_mut();
            let mut cached_occurrences = std::mem::take(&mut state.cached_occurrences);
            state.is_available = count > 0;
            state.connections = sources
                .into_iter()
                .map(|source| Connection {
                    occurrences: cached_occurrences.remove(&source.uid).unwrap_or_default(),
                    uid: source.uid,
                    color: source.color,
                    calendar: None,
                    watch: None,
                    is_refresh_pending: false,
                })
                .collect();
            state.rebuild();
        }
        self.listeners.notify();
        for index in 0..count {
            let agenda = self.clone();
            glib::spawn_future_local(async move { agenda.open(index, generation).await });
        }
    }

    async fn open(self: Rc<Self>, index: usize, generation: u64) {
        let Some(bus) = self.bus.clone() else {
            return;
        };
        let uid = {
            let state = self.state.borrow();
            if state.generation != generation {
                return;
            }
            let Some(connection) = state.connections.get(index) else {
                return;
            };
            connection.uid.clone()
        };
        let Some(calendar) = eds::open(&bus, &uid).await else {
            return;
        };
        if !self.is_current(generation) {
            eds::release(&bus, &calendar, None);
            return;
        }
        self.state.borrow_mut().connections[index].calendar = Some(calendar);
        self.refresh(index, generation).await;
        self.watch(index, generation).await;
    }

    fn build_request(&self, index: usize, generation: u64, zones: &Zones) -> Option<Request> {
        let state = self.state.borrow();
        if state.generation != generation {
            return None;
        }
        let connection = state.connections.get(index)?;
        let window = state.window;
        let first = zones.local_to_unix((window.first_day - 1) * SECONDS_PER_DAY);
        let end = zones.local_to_unix((window.end_day + 1) * SECONDS_PER_DAY);
        Some(Request {
            calendar: connection.calendar.clone()?,
            color: connection.color.clone(),
            window,
            expression: eds::range_query(first, end, &zones.local_identifier()),
        })
    }

    async fn refresh(self: &Rc<Self>, index: usize, generation: u64) {
        let Some(bus) = self.bus.clone() else {
            return;
        };
        let zones = Zones::local();
        let Some(request) = self.build_request(index, generation, &zones) else {
            return;
        };
        let Some(texts) = eds::query(&bus, &request.calendar, &request.expression).await else {
            return;
        };
        let events: Vec<event::Event> = texts
            .iter()
            .flat_map(|text| event::parse_events(text))
            .collect();
        let window = request.window;
        let occurrences = days::expand_events(&events, window, &zones, request.color.as_deref());
        {
            let mut state = self.state.borrow_mut();
            if state.generation != generation || state.window != window {
                return;
            }
            state.connections[index].occurrences = occurrences;
            state.rebuild();
        }
        self.listeners.notify();
    }

    async fn watch(self: &Rc<Self>, index: usize, generation: u64) {
        let Some(bus) = self.bus.clone() else {
            return;
        };
        let Some(request) = self.build_request(index, generation, &Zones::local()) else {
            return;
        };
        let agenda = Rc::downgrade(self);
        let Some(watch) = eds::watch(&bus, &request.calendar, &request.expression, move || {
            if let Some(agenda) = agenda.upgrade() {
                agenda.schedule_refresh(index, generation);
            }
        })
        .await
        else {
            return;
        };
        let mut state = self.state.borrow_mut();
        if state.generation != generation || state.window != request.window {
            drop(state);
            eds::dispose_view(&bus, &request.calendar, watch);
            return;
        }
        let replaced_watch = state.connections[index].watch.replace(watch);
        drop(state);
        if let Some(replaced_watch) = replaced_watch {
            eds::dispose_view(&bus, &request.calendar, replaced_watch);
        }
    }

    fn schedule_refresh(self: &Rc<Self>, index: usize, generation: u64) {
        {
            let mut state = self.state.borrow_mut();
            if state.generation != generation {
                return;
            }
            let Some(connection) = state.connections.get_mut(index) else {
                return;
            };
            if connection.is_refresh_pending {
                return;
            }
            connection.is_refresh_pending = true;
        }
        let agenda = self.clone();
        glib::timeout_add_local_once(Duration::from_millis(REFRESH_DELAY_MILLIS), move || {
            {
                let mut state = agenda.state.borrow_mut();
                if state.generation != generation {
                    return;
                }
                if let Some(connection) = state.connections.get_mut(index) {
                    connection.is_refresh_pending = false;
                }
            }
            glib::spawn_future_local(async move { agenda.refresh(index, generation).await });
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_from_an_earlier_generation_does_not_mark_a_refresh_pending() {
        let agenda = Agenda::new(None);
        {
            let mut state = agenda.state.borrow_mut();
            state.generation = 2;
            state.connections.push(Connection {
                uid: "calendar".into(),
                color: None,
                calendar: None,
                watch: None,
                occurrences: Vec::new(),
                is_refresh_pending: false,
            });
        }
        agenda.schedule_refresh(0, 1);
        assert!(!agenda.state.borrow().connections[0].is_refresh_pending);
    }
}
