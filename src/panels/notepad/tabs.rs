use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::symbolbutton;
use crate::core::i18n::tr;
use crate::services::session::notes::{Notes, Summary};
use crate::ui::theme::{SharedTheme, pixel_size, rounding, transparentize};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::ripple::{Look, RippleButton};

const HEIGHT: i32 = 34;
const PADDING: i32 = 10;
const SPACING: i32 = 4;
const ADD_SIZE: i32 = 32;
const CLOSE_SIZE: i32 = 22;
const LABEL_CHARS: i32 = 16;
const ENTRY_CHARS: i32 = 14;
const WHEEL_STEP: f64 = 40.0;

struct Tab {
    id: u64,
    button: RippleButton,
    label: gtk4::Label,
    stack: gtk4::Stack,
    entry: gtk4::Entry,
}

pub struct Tabs {
    pub widget: gtk4::Widget,
    row: gtk4::Box,
    scroll: gtk4::ScrolledWindow,
    theme: SharedTheme,
    notes: Rc<Notes>,
    items: RefCell<Vec<Tab>>,
    rename_id: Cell<Option<u64>>,
    original_title: RefCell<String>,
    last_active_id: Cell<Option<u64>>,
    on_close: Rc<dyn Fn(u64)>,
    on_renamed: Rc<dyn Fn()>,
}

impl Tabs {
    pub fn new(
        theme: &SharedTheme,
        notes: &Rc<Notes>,
        on_close: Rc<dyn Fn(u64)>,
        on_renamed: Rc<dyn Fn()>,
    ) -> Rc<Self> {
        let row = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::External, gtk4::PolicyType::Never);
        scroll.set_hexpand(true);
        scroll.set_child(Some(&row));
        let wheel = gtk4::EventControllerScroll::new(gtk4::EventControllerScrollFlags::VERTICAL);
        wheel.connect_scroll({
            let scroll = scroll.downgrade();
            move |_, _, delta| {
                let Some(scroll) = scroll.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let adjustment = scroll.hadjustment();
                adjustment.set_value(adjustment.value() + delta * WHEEL_STEP);
                glib::Propagation::Stop
            }
        });
        scroll.add_controller(wheel);

        let add_button = symbolbutton::build(theme, "add", pixel_size::HUGE, ADD_SIZE);
        add_button.connect_clicked({
            let notes = notes.clone();
            move |_| notes.create()
        });

        let header = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        header.append(&scroll);
        header.append(&add_button);

        let tabs = Rc::new(Tabs {
            widget: header.upcast(),
            row,
            scroll,
            theme: theme.clone(),
            notes: notes.clone(),
            items: RefCell::new(Vec::new()),
            rename_id: Cell::new(None),
            original_title: RefCell::default(),
            last_active_id: Cell::new(None),
            on_close,
            on_renamed,
        });
        tabs.refresh();
        tabs
    }

    pub fn is_renaming(&self) -> bool {
        self.rename_id.get().is_some()
    }

    pub fn cancel_rename(self: &Rc<Self>) {
        self.finish_rename(false);
    }

    pub fn refresh(self: &Rc<Self>) {
        let summaries = self.notes.summaries();
        let active = self.notes.active_id();
        let is_unchanged = {
            let items = self.items.borrow();
            items.len() == summaries.len()
                && items
                    .iter()
                    .zip(&summaries)
                    .all(|(tab, summary)| tab.id == summary.id)
        };
        if !is_unchanged {
            self.rebuild(&summaries);
        }
        for (tab, summary) in self.items.borrow().iter().zip(&summaries) {
            let title = summary
                .title()
                .map_or_else(|| tr("Untitled"), str::to_owned);
            tab.label.set_text(&title);
            let is_active = summary.id == active;
            tab.button.set_toggled(is_active);
            text::set_color(
                &tab.label,
                if is_active {
                    "colOnSecondaryContainer"
                } else {
                    "colOnLayer1"
                },
            );
        }
        if self.last_active_id.replace(Some(active)) != Some(active) {
            self.reveal(active);
        }
    }

    fn rebuild(self: &Rc<Self>, summaries: &[Summary]) {
        self.rename_id.set(None);
        while let Some(child) = self.row.first_child() {
            self.row.remove(&child);
        }
        let items: Vec<Tab> = summaries
            .iter()
            .map(|summary| self.build_tab(summary.id))
            .collect();
        for tab in &items {
            self.row.append(&tab.button);
        }
        self.items.replace(items);
    }

    fn build_tab(self: &Rc<Self>, id: u64) -> Tab {
        let label = text::styled("");
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        label.set_max_width_chars(LABEL_CHARS);
        label.add_css_class("color-fade");

        let entry = gtk4::Entry::new();
        entry.add_css_class("notepad-field");
        entry.set_width_chars(ENTRY_CHARS);
        entry.set_max_width_chars(ENTRY_CHARS);
        entry.set_valign(gtk4::Align::Center);
        entry.connect_activate({
            let tabs = Rc::downgrade(self);
            move |_| {
                if let Some(tabs) = tabs.upgrade() {
                    tabs.finish_rename(true);
                }
            }
        });
        let focus = gtk4::EventControllerFocus::new();
        focus.connect_leave({
            let tabs = Rc::downgrade(self);
            move |_| {
                if let Some(tabs) = tabs.upgrade() {
                    tabs.finish_rename(true);
                }
            }
        });
        entry.add_controller(focus);

        let stack = gtk4::Stack::new();
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);
        stack.add_named(&label, Some("label"));
        stack.add_named(&entry, Some("entry"));
        stack.set_visible_child_name("label");

        let close_button = symbolbutton::build(&self.theme, "close", pixel_size::LARGE, CLOSE_SIZE);
        close_button.connect_clicked({
            let on_close = self.on_close.clone();
            move |_| on_close(id)
        });

        let content = gtk4::Box::new(gtk4::Orientation::Horizontal, SPACING);
        content.append(&stack);
        content.append(&close_button);

        let button = RippleButton::new(&self.theme);
        button.set_look(Look {
            toggled: |theme| theme.colors.col_secondary_container,
            toggled_hover: |theme| theme.colors.col_secondary_container_hover,
            ripple_toggled: |theme| theme.colors.col_secondary_container_active,
            background: |theme| transparentize(theme.colors.col_layer1_hover, 1.0),
            ..Look::default()
        });
        button.set_radius(rounding::SMALL as f64);
        button.set_size_request(-1, HEIGHT);
        button.set_content(&content, PADDING, 0);
        button.connect_down({
            let notes = self.notes.clone();
            move || notes.select(id)
        });
        let double_click = gtk4::GestureClick::new();
        double_click.connect_pressed({
            let tabs = Rc::downgrade(self);
            move |_, press_count, _, _| {
                if let (2, Some(tabs)) = (press_count, tabs.upgrade()) {
                    tabs.start_rename(id);
                }
            }
        });
        button.add_controller(double_click);

        Tab {
            id,
            button,
            label,
            stack,
            entry,
        }
    }

    fn start_rename(self: &Rc<Self>, id: u64) {
        self.finish_rename(true);
        let title = self
            .notes
            .summaries()
            .into_iter()
            .find(|summary| summary.id == id)
            .and_then(|summary| summary.title().map(str::to_owned))
            .unwrap_or_default();
        let rename_widgets = self
            .items
            .borrow()
            .iter()
            .find(|tab| tab.id == id)
            .map(|tab| (tab.stack.clone(), tab.entry.clone()));
        let Some((stack, entry)) = rename_widgets else {
            return;
        };
        self.rename_id.set(Some(id));
        self.original_title.replace(title.clone());
        entry.set_text(&title);
        stack.set_visible_child_name("entry");
        entry.grab_focus();
    }

    fn finish_rename(self: &Rc<Self>, should_apply: bool) {
        let Some(id) = self.rename_id.take() else {
            return;
        };
        let title = {
            let items = self.items.borrow();
            let Some(tab) = items.iter().find(|tab| tab.id == id) else {
                return;
            };
            tab.stack.set_visible_child_name("label");
            tab.entry.text().to_string()
        };
        if should_apply && title != *self.original_title.borrow() {
            self.notes.rename(id, &title);
        }
        (self.on_renamed)();
    }

    fn reveal(self: &Rc<Self>, id: u64) {
        let tabs = Rc::downgrade(self);
        glib::idle_add_local_once(move || {
            let Some(tabs) = tabs.upgrade() else {
                return;
            };
            let items = tabs.items.borrow();
            let Some(bounds) = items
                .iter()
                .find(|tab| tab.id == id)
                .and_then(|tab| tab.button.compute_bounds(&tabs.row))
            else {
                return;
            };
            let adjustment = tabs.scroll.hadjustment();
            let (left, right) = (bounds.x() as f64, (bounds.x() + bounds.width()) as f64);
            if left < adjustment.value() {
                adjustment.set_value(left);
            } else if right > adjustment.value() + adjustment.page_size() {
                adjustment.set_value(right - adjustment.page_size());
            }
        });
    }
}
