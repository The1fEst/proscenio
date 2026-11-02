use gtk4::gdk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::ops::Range;
use std::rc::Rc;

use super::matches::{Direction, find_matches, locate, step_index};
use super::symbolbutton;
use crate::core::i18n::{tr, trf};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;

const SPACING: i32 = 4;
const BUTTON_SIZE: i32 = 28;
const COUNT_MARGIN: i32 = 6;
const MARGIN_BELOW: i32 = 6;

pub struct FindBar {
    pub widget: gtk4::Revealer,
    entry: gtk4::Entry,
    count: gtk4::Label,
    buffer: RefCell<Option<gtk4::TextBuffer>>,
    handler: RefCell<Option<glib::SignalHandlerId>>,
    matches: RefCell<Vec<Range<usize>>>,
    current_index: Cell<Option<usize>>,
    is_refresh_pending: Cell<bool>,
    on_select: Rc<dyn Fn(Range<usize>)>,
    on_close: Rc<dyn Fn()>,
}

impl FindBar {
    pub fn new(
        theme: &SharedTheme,
        on_select: Rc<dyn Fn(Range<usize>)>,
        on_close: Rc<dyn Fn()>,
    ) -> Rc<Self> {
        let entry = gtk4::Entry::new();
        entry.add_css_class("notepad-field");
        entry.set_placeholder_text(Some(&tr("Find")));
        entry.set_hexpand(true);
        entry.set_valign(gtk4::Align::Center);

        let count = text::styled_sized("", pixel_size::SMALLER);
        text::set_color(&count, "colSubtext");
        count.set_margin_start(COUNT_MARGIN);
        count.set_margin_end(COUNT_MARGIN);

        let glyph_size = pixel_size::HUGE;
        let previous_button =
            symbolbutton::build(theme, "keyboard_arrow_up", glyph_size, BUTTON_SIZE);
        let next_button =
            symbolbutton::build(theme, "keyboard_arrow_down", glyph_size, BUTTON_SIZE);
        let close_button = symbolbutton::build(theme, "close", glyph_size, BUTTON_SIZE);

        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        row.set_margin_bottom(MARGIN_BELOW);
        row.append(&entry);
        row.append(&count);
        row.append(&previous_button);
        row.append(&next_button);
        row.append(&close_button);

        let widget = gtk4::Revealer::new();
        widget.set_transition_type(gtk4::RevealerTransitionType::SlideDown);
        widget.set_child(Some(&row));

        let bar = Rc::new(FindBar {
            widget,
            entry: entry.clone(),
            count,
            buffer: RefCell::new(None),
            handler: RefCell::new(None),
            matches: RefCell::new(Vec::new()),
            current_index: Cell::new(None),
            is_refresh_pending: Cell::new(false),
            on_select,
            on_close,
        });

        entry.connect_changed({
            let bar = Rc::downgrade(&bar);
            move |_| {
                if let Some(bar) = bar.upgrade() {
                    bar.search();
                }
            }
        });
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        keys.connect_key_pressed({
            let bar = Rc::downgrade(&bar);
            move |_, key, _, modifiers| {
                if !matches!(key, gdk::Key::Return | gdk::Key::KP_Enter) {
                    return glib::Propagation::Proceed;
                }
                if let Some(bar) = bar.upgrade() {
                    let direction = if modifiers.contains(gdk::ModifierType::SHIFT_MASK) {
                        Direction::Backward
                    } else {
                        Direction::Forward
                    };
                    bar.step(direction);
                }
                glib::Propagation::Stop
            }
        });
        entry.add_controller(keys);
        for (button, direction) in [
            (&previous_button, Direction::Backward),
            (&next_button, Direction::Forward),
        ] {
            button.connect_clicked({
                let bar = Rc::downgrade(&bar);
                move |_| {
                    if let Some(bar) = bar.upgrade() {
                        bar.step(direction);
                    }
                }
            });
        }
        close_button.connect_clicked({
            let bar = Rc::downgrade(&bar);
            move |_| {
                if let Some(bar) = bar.upgrade() {
                    bar.close();
                }
            }
        });
        bar
    }

    pub fn is_open(&self) -> bool {
        self.widget.reveals_child()
    }

    pub fn open(self: &Rc<Self>, prefill: Option<&str>) {
        self.widget.set_reveal_child(true);
        self.watch_buffer();
        if let Some(prefill) = prefill {
            self.entry.set_text(prefill);
        }
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
        self.search();
    }

    pub fn close(&self) {
        if self.is_open() {
            self.hide();
            (self.on_close)();
        }
    }

    pub fn hide(&self) {
        self.widget.set_reveal_child(false);
        self.unwatch_buffer();
    }

    pub fn set_buffer(self: &Rc<Self>, buffer: Option<gtk4::TextBuffer>) {
        if *self.buffer.borrow() == buffer {
            return;
        }
        self.unwatch_buffer();
        self.buffer.replace(buffer);
        if self.is_open() {
            self.watch_buffer();
            self.search();
        }
    }

    fn watch_buffer(self: &Rc<Self>) {
        let Some(buffer) = self.buffer.borrow().clone() else {
            return;
        };
        if self.handler.borrow().is_some() {
            return;
        }
        let bar = Rc::downgrade(self);
        let handler = buffer.connect_changed(move |_| {
            if let Some(bar) = bar.upgrade() {
                bar.refresh_later();
            }
        });
        self.handler.replace(Some(handler));
    }

    fn unwatch_buffer(&self) {
        let handler = self.handler.take();
        let buffer = self.buffer.borrow().clone();
        if let (Some(handler), Some(buffer)) = (handler, buffer) {
            buffer.disconnect(handler);
        }
    }

    fn refresh_later(self: &Rc<Self>) {
        if self.is_refresh_pending.replace(true) {
            return;
        }
        let bar = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(bar) = bar.upgrade() else {
                return;
            };
            bar.is_refresh_pending.set(false);
            bar.collect_matches();
            bar.current_index.set(locate(
                &bar.matches.borrow(),
                bar.cursor(),
                Direction::Forward,
            ));
            bar.show_count();
        });
    }

    fn search(&self) {
        self.collect_matches();
        let current_index = locate(&self.matches.borrow(), self.cursor(), Direction::Forward);
        self.current_index.set(current_index);
        self.show_count();
        self.select_current();
    }

    fn step(&self, direction: Direction) {
        let count = self.matches.borrow().len();
        if count == 0 {
            return;
        }
        let next_index = match self.selected_match() {
            Some(selected) => step_index(count, selected, direction),
            None => locate(&self.matches.borrow(), self.cursor(), direction).unwrap_or(0),
        };
        self.current_index.set(Some(next_index));
        self.show_count();
        self.select_current();
    }

    fn collect_matches(&self) {
        let query = self.entry.text();
        let matches = match self.buffer.borrow().as_ref() {
            Some(buffer) if !query.is_empty() => {
                let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), true);
                find_matches(&text, &query)
            }
            _ => Vec::new(),
        };
        self.matches.replace(matches);
    }

    fn cursor(&self) -> usize {
        let buffer = self.buffer.borrow();
        let Some(buffer) = buffer.as_ref() else {
            return 0;
        };
        match buffer.selection_bounds() {
            Some((start, _)) => start.offset().max(0) as usize,
            None => buffer.cursor_position().max(0) as usize,
        }
    }

    fn selected_match(&self) -> Option<usize> {
        let buffer = self.buffer.borrow();
        let (start, end) = buffer.as_ref()?.selection_bounds()?;
        let selection = start.offset().max(0) as usize..end.offset().max(0) as usize;
        self.matches
            .borrow()
            .iter()
            .position(|found| *found == selection)
    }

    fn select_current(&self) {
        let range = self
            .current_index
            .get()
            .and_then(|index| self.matches.borrow().get(index).cloned());
        if let Some(range) = range {
            (self.on_select)(range);
        }
    }

    fn show_count(&self) {
        let count = self.matches.borrow().len();
        let text = match self.current_index.get() {
            _ if self.entry.text().is_empty() => String::new(),
            Some(index) => trf("%1 of %2", &[&(index + 1).to_string(), &count.to_string()]),
            None => tr("No matches"),
        };
        self.count.set_text(&text);
    }
}

impl Drop for FindBar {
    fn drop(&mut self) {
        self.unwatch_buffer();
    }
}
