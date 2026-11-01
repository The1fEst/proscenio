use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::{tr, trf, trn};
use crate::services::session::notes::storage::MAX_NOTE_BYTES;
use crate::services::session::notes::{Notes, Status};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;

const MARGIN: i32 = 10;
const FOOTER_MARGIN: i32 = 6;
const MEGABYTE: u64 = 1024 * 1024;
const WARNING_DURATION: Duration = Duration::from_millis(1500);

struct Binding {
    id: u64,
    status: Status,
    connection: Option<(gtk4::TextBuffer, glib::SignalHandlerId)>,
}

pub struct Editor {
    pub widget: gtk4::Widget,
    view: gtk4::TextView,
    message: gtk4::Label,
    counts: gtk4::Label,
    notes: Rc<Notes>,
    blank: gtk4::TextBuffer,
    binding: RefCell<Option<Binding>>,
    warning_generation: Cell<u32>,
}

impl Editor {
    pub fn new(notes: &Rc<Notes>) -> Rc<Self> {
        let blank = gtk4::TextBuffer::new(None);
        let view = gtk4::TextView::with_buffer(&blank);
        view.add_css_class("notepad-editor");
        view.set_wrap_mode(gtk4::WrapMode::WordChar);
        view.set_left_margin(MARGIN);
        view.set_right_margin(MARGIN);
        view.set_top_margin(MARGIN);
        view.set_bottom_margin(MARGIN);

        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
        scroll.set_child(Some(&view));
        scroll.set_vexpand(true);
        crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);

        let message = text::styled_sized("", pixel_size::NORMAL);
        text::set_color(&message, "m3outline");
        message.set_wrap(true);
        message.set_justify(gtk4::Justification::Center);
        message.set_halign(gtk4::Align::Center);
        message.set_valign(gtk4::Align::Center);
        message.set_margin_start(MARGIN * 2);
        message.set_margin_end(MARGIN * 2);
        message.set_can_target(false);
        message.set_visible(false);

        let overlay = gtk4::Overlay::new();
        overlay.set_child(Some(&scroll));
        overlay.add_overlay(&message);

        let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        page.add_css_class("notepad-page");
        page.set_overflow(gtk4::Overflow::Hidden);
        page.set_vexpand(true);
        page.append(&overlay);

        let counts = text::styled_sized("", pixel_size::SMALLER);
        text::set_color(&counts, "colSubtext");
        counts.set_xalign(0.0);
        counts.set_margin_start(FOOTER_MARGIN);

        let column = gtk4::Box::new(gtk4::Orientation::Vertical, FOOTER_MARGIN);
        column.append(&page);
        column.append(&counts);

        Rc::new(Editor {
            widget: column.upcast(),
            view,
            message,
            counts,
            notes: notes.clone(),
            blank,
            binding: RefCell::new(None),
            warning_generation: Cell::new(0),
        })
    }

    pub fn refresh(self: &Rc<Self>) {
        let id = self.notes.active_id();
        let Some(status) = self.notes.status(id) else {
            return;
        };
        let is_bound = self
            .binding
            .borrow()
            .as_ref()
            .is_some_and(|binding| binding.id == id && binding.status == status);
        if is_bound {
            return;
        }
        self.unbind();
        let buffer = self.notes.buffer(id).filter(|_| status == Status::Text);
        let Some(buffer) = buffer else {
            self.view.set_buffer(Some(&self.blank));
            self.view.set_editable(false);
            self.view.set_cursor_visible(false);
            self.counts.set_text("");
            self.show_message(status);
            self.binding.replace(Some(Binding {
                id,
                status,
                connection: None,
            }));
            return;
        };
        self.view.set_buffer(Some(&buffer));
        self.view.set_editable(true);
        self.view.set_cursor_visible(true);
        self.message.set_visible(false);
        let handler = buffer.connect_changed({
            let editor = Rc::downgrade(self);
            move |_| {
                if let Some(editor) = editor.upgrade() {
                    editor.update_counts();
                }
            }
        });
        self.binding.replace(Some(Binding {
            id,
            status,
            connection: Some((buffer, handler)),
        }));
        self.update_counts();
        self.scroll_to_cursor();
    }

    pub fn warn_full(self: &Rc<Self>) {
        text::set_color(&self.counts, "colError");
        let generation = self.warning_generation.get() + 1;
        self.warning_generation.set(generation);
        let editor = Rc::downgrade(self);
        glib::timeout_add_local_once(WARNING_DURATION, move || {
            if let Some(editor) = editor.upgrade()
                && editor.warning_generation.get() == generation
            {
                text::set_color(&editor.counts, "colSubtext");
            }
        });
    }

    pub fn focus(&self) {
        self.view.grab_focus();
        self.scroll_to_cursor();
    }

    fn unbind(&self) {
        let binding = self.binding.take();
        if let Some((buffer, handler)) = binding.and_then(|binding| binding.connection) {
            buffer.disconnect(handler);
        }
    }

    fn show_message(&self, status: Status) {
        let message = match status {
            Status::Overflow => trf(
                "This note is larger than %1 MB and was not opened.",
                &[&(MAX_NOTE_BYTES / MEGABYTE).to_string()],
            ),
            Status::Failure => tr("This note could not be read."),
            _ => String::new(),
        };
        self.message.set_text(&message);
        self.message.set_visible(!message.is_empty());
    }

    fn update_counts(&self) {
        let buffer = self.view.buffer();
        self.counts.set_text(&format!(
            "{} · {}",
            trn("%1 line", "%1 lines", buffer.line_count() as u64),
            trn("%1 character", "%1 characters", buffer.char_count() as u64),
        ));
    }

    fn scroll_to_cursor(&self) {
        let view = self.view.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(view) = view.upgrade() {
                view.scroll_to_mark(&view.buffer().get_insert(), 0.1, false, 0.0, 0.0);
            }
        });
    }
}

impl Drop for Editor {
    fn drop(&mut self) {
        self.unbind();
    }
}
