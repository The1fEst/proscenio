pub mod debounce;
pub mod storage;
mod writer;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use crate::core::listeners::{Listeners, Subscription};
use debounce::Debouncer;
use storage::{Content, Entry, Index};
use writer::{Job, Writer};

const DEBOUNCER: Debouncer = Debouncer::new(500, 5000);
const TITLE_SCAN_CHARS: i32 = 256;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Status {
    Stub,
    Fetch,
    Text,
    Overflow,
    Failure,
}

#[derive(Clone)]
pub struct Summary {
    pub id: u64,
    pub custom_title: Option<String>,
    pub derived_title: Option<String>,
}

impl Summary {
    pub fn title(&self) -> Option<&str> {
        self.custom_title
            .as_deref()
            .or(self.derived_title.as_deref())
    }
}

struct Note {
    id: u64,
    custom_title: Option<String>,
    derived_title: Option<String>,
    cursor: usize,
    status: Status,
    buffer: gtk4::TextBuffer,
}

pub struct Notes {
    directory: PathBuf,
    notes: RefCell<Vec<Note>>,
    current_id: Cell<u64>,
    next_id: Cell<u64>,
    changed_ids: RefCell<HashSet<u64>>,
    first_change_millis: Cell<Option<i64>>,
    save_timer: RefCell<Option<glib::SourceId>>,
    writer: Rc<Writer>,
    last_index: RefCell<String>,
    listeners: Listeners,
    rejections: Rc<Listeners>,
}

impl Notes {
    pub fn new() -> Rc<Self> {
        let directory = storage::directory();
        let index = storage::load_index(&directory);
        let notes = Rc::new(Notes {
            directory,
            notes: RefCell::new(Vec::new()),
            current_id: Cell::new(index.active_id),
            next_id: Cell::new(index.next_id),
            changed_ids: RefCell::default(),
            first_change_millis: Cell::new(None),
            save_timer: RefCell::new(None),
            writer: Rc::default(),
            last_index: RefCell::default(),
            listeners: Listeners::default(),
            rejections: Rc::default(),
        });
        for entry in index.entries {
            let note = notes.build_note(entry, Status::Stub);
            notes.notes.borrow_mut().push(note);
        }
        if notes.notes.borrow().is_empty() {
            notes.add_empty();
            notes.queue_index();
        }
        notes.request(notes.current_id.get());
        notes
    }

    pub fn subscribe(&self, listener: impl Fn() + 'static) -> Subscription {
        self.listeners.add(listener)
    }

    pub fn subscribe_rejection(&self, listener: impl Fn() + 'static) -> Subscription {
        self.rejections.add(listener)
    }

    pub fn summaries(&self) -> Vec<Summary> {
        self.notes
            .borrow()
            .iter()
            .map(|note| Summary {
                id: note.id,
                custom_title: note.custom_title.clone(),
                derived_title: note.derived_title.clone(),
            })
            .collect()
    }

    pub fn active_id(&self) -> u64 {
        self.current_id.get()
    }

    pub fn status(&self, id: u64) -> Option<Status> {
        self.notes
            .borrow()
            .iter()
            .find(|note| note.id == id)
            .map(|note| note.status)
    }

    pub fn buffer(&self, id: u64) -> Option<gtk4::TextBuffer> {
        self.notes
            .borrow()
            .iter()
            .find(|note| note.id == id)
            .map(|note| note.buffer.clone())
    }

    pub fn is_empty(&self, id: u64) -> bool {
        self.notes
            .borrow()
            .iter()
            .find(|note| note.id == id)
            .is_some_and(|note| note.status == Status::Text && note.buffer.char_count() == 0)
    }

    pub fn select(self: &Rc<Self>, id: u64) {
        if self.status(id).is_none() {
            return;
        }
        self.request(id);
        if self.current_id.get() == id {
            return;
        }
        self.current_id.set(id);
        self.queue_index();
        self.listeners.notify();
    }

    pub fn create(self: &Rc<Self>) {
        self.add_empty();
        self.queue_index();
        self.listeners.notify();
    }

    pub fn remove(self: &Rc<Self>, id: u64) {
        let position = {
            let mut notes = self.notes.borrow_mut();
            let Some(position) = notes.iter().position(|note| note.id == id) else {
                return;
            };
            notes.remove(position);
            position
        };
        self.changed_ids.borrow_mut().remove(&id);
        self.writer.submit(Job::Remove {
            path: storage::note_path(&self.directory, id),
        });
        if self.notes.borrow().is_empty() {
            self.add_empty();
        } else if self.current_id.get() == id {
            let neighbor = {
                let notes = self.notes.borrow();
                notes[position.min(notes.len() - 1)].id
            };
            self.current_id.set(neighbor);
            self.request(neighbor);
        }
        self.queue_index();
        self.listeners.notify();
    }

    pub fn rename(self: &Rc<Self>, id: u64, title: &str) {
        let title = Some(storage::clamp_title(title)).filter(|title| !title.is_empty());
        {
            let mut notes = self.notes.borrow_mut();
            let Some(note) = notes.iter_mut().find(|note| note.id == id) else {
                return;
            };
            if note.custom_title == title {
                return;
            }
            note.custom_title = title;
        }
        self.queue_index();
        self.listeners.notify();
    }

    pub fn flush(self: &Rc<Self>) {
        if let Some(timer) = self.save_timer.take() {
            timer.remove();
        }
        self.save_now();
    }

    fn build_note(self: &Rc<Self>, entry: Entry, status: Status) -> Note {
        let buffer = gtk4::TextBuffer::new(None);
        limit_size(&buffer, self.rejections.clone());
        let notes = Rc::downgrade(self);
        let id = entry.id;
        buffer.connect_changed(move |_| {
            if let Some(notes) = notes.upgrade() {
                notes.handle_change(id);
            }
        });
        Note {
            id,
            custom_title: entry.custom_title,
            derived_title: entry.derived_title,
            cursor: entry.cursor,
            status,
            buffer,
        }
    }

    fn add_empty(self: &Rc<Self>) {
        let id = self.next_id.get();
        self.next_id.set(id.saturating_add(1));
        let entry = Entry {
            id,
            custom_title: None,
            derived_title: None,
            cursor: 0,
        };
        let note = self.build_note(entry, Status::Text);
        self.notes.borrow_mut().push(note);
        self.current_id.set(id);
    }

    fn request(self: &Rc<Self>, id: u64) {
        {
            let mut notes = self.notes.borrow_mut();
            let Some(note) = notes.iter_mut().find(|note| note.id == id) else {
                return;
            };
            if !matches!(note.status, Status::Stub | Status::Failure) {
                return;
            }
            note.status = Status::Fetch;
        }
        let directory = self.directory.clone();
        let notes = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let content = gio::spawn_blocking(move || storage::read_note(&directory, id))
                .await
                .unwrap_or(Content::Failure);
            if let Some(notes) = notes.upgrade() {
                notes.apply(id, content);
            }
        });
    }

    fn apply(&self, id: u64, content: Content) {
        let (status, text) = match content {
            Content::Text(text) => (Status::Text, text),
            Content::Absence => (Status::Text, String::new()),
            Content::Overflow => (Status::Overflow, String::new()),
            Content::Failure => (Status::Failure, String::new()),
        };
        let (buffer, cursor) = {
            let notes = self.notes.borrow();
            match notes.iter().find(|note| note.id == id) {
                Some(note) if note.status == Status::Fetch => (note.buffer.clone(), note.cursor),
                _ => return,
            }
        };
        let derived_title = (status == Status::Text).then(|| {
            buffer.begin_irreversible_action();
            buffer.set_text(&text);
            buffer.end_irreversible_action();
            buffer.place_cursor(&buffer.iter_at_offset(cursor.min(i32::MAX as usize) as i32));
            derive_buffer_title(&buffer)
        });
        if let Some(note) = self
            .notes
            .borrow_mut()
            .iter_mut()
            .find(|note| note.id == id)
        {
            note.status = status;
            if let Some(derived_title) = derived_title {
                note.derived_title = derived_title;
            }
        }
        self.listeners.notify();
    }

    fn handle_change(self: &Rc<Self>, id: u64) {
        let is_title_changed = {
            let mut notes = self.notes.borrow_mut();
            let Some(note) = notes.iter_mut().find(|note| note.id == id) else {
                return;
            };
            if note.status != Status::Text {
                return;
            }
            let derived_title = derive_buffer_title(&note.buffer);
            let is_changed = note.derived_title != derived_title;
            note.derived_title = derived_title;
            is_changed
        };
        self.changed_ids.borrow_mut().insert(id);
        self.schedule_save();
        if is_title_changed {
            self.listeners.notify();
        }
    }

    fn schedule_save(self: &Rc<Self>) {
        let now_millis = glib::monotonic_time() / 1000;
        let first_change_millis = self.first_change_millis.get().unwrap_or(now_millis);
        self.first_change_millis.set(Some(first_change_millis));
        if let Some(timer) = self.save_timer.take() {
            timer.remove();
        }
        let delay = DEBOUNCER.delay_millis(first_change_millis, now_millis);
        let notes = Rc::downgrade(self);
        let timer = glib::timeout_add_local_once(Duration::from_millis(delay), move || {
            let Some(notes) = notes.upgrade() else {
                return;
            };
            notes.save_timer.take();
            notes.save_now();
        });
        self.save_timer.replace(Some(timer));
    }

    fn save_now(self: &Rc<Self>) {
        self.first_change_millis.set(None);
        let ids: Vec<u64> = self.changed_ids.borrow_mut().drain().collect();
        for id in ids {
            self.queue_note(id);
        }
        self.queue_index();
    }

    fn queue_note(&self, id: u64) {
        let Some(buffer) = self
            .notes
            .borrow()
            .iter()
            .find(|note| note.id == id && note.status == Status::Text)
            .map(|note| note.buffer.clone())
        else {
            return;
        };
        let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
        self.writer.submit(Job::Write {
            path: storage::note_path(&self.directory, id),
            bytes: text.as_str().as_bytes().to_vec(),
        });
    }

    fn queue_index(&self) {
        let entries = self
            .notes
            .borrow()
            .iter()
            .map(|note| Entry {
                id: note.id,
                custom_title: note.custom_title.clone(),
                derived_title: note.derived_title.clone(),
                cursor: if note.status == Status::Text {
                    note.buffer.cursor_position().max(0) as usize
                } else {
                    note.cursor
                },
            })
            .collect();
        let index = Index {
            active_id: self.current_id.get(),
            next_id: self.next_id.get(),
            entries,
        };
        let text = storage::serialize_index(&index);
        if *self.last_index.borrow() == text {
            return;
        }
        self.writer.submit(Job::Write {
            path: storage::index_path(&self.directory),
            bytes: text.clone().into_bytes(),
        });
        self.last_index.replace(text);
    }
}

fn derive_buffer_title(buffer: &gtk4::TextBuffer) -> Option<String> {
    let start = buffer.start_iter();
    let mut end = start;
    end.forward_chars(TITLE_SCAN_CHARS);
    storage::derive_title(&buffer.text(&start, &end, false))
}

fn limit_size(buffer: &gtk4::TextBuffer, rejections: Rc<Listeners>) {
    let size_bytes = Rc::new(Cell::new(0usize));
    buffer.connect_insert_text({
        let size_bytes = size_bytes.clone();
        move |buffer, _, text| {
            if size_bytes.get() + text.len() > storage::MAX_NOTE_BYTES as usize {
                buffer.stop_signal_emission_by_name("insert-text");
                rejections.notify();
                return;
            }
            size_bytes.set(size_bytes.get() + text.len());
        }
    });
    buffer.connect_delete_range(move |buffer, start, end| {
        let removed_bytes = buffer.text(start, end, true).len();
        size_bytes.set(size_bytes.get().saturating_sub(removed_bytes));
    });
}
