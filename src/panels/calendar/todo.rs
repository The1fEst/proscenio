use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::scope::Scope;
use crate::services::todo::Todo;
use crate::ui::anim::{EXPRESSIVE_DEFAULT, EXPRESSIVE_EFFECTS, Fader};
use crate::ui::theme::{SharedTheme, pixel_size, rounding};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::ripple::{Look, RippleButton};
use crate::ui::widgets::secondarytabs::SecondaryTabs;
use crate::ui::widgets::swipe::Swipe;
use crate::ui::widgets::text;
use crate::ui::widgets::windowdialog;

const TABS: [(&str, &str); 2] = [("checklist", "Unfinished"), ("check_circle", "Done")];
const ITEM_SPACING: i32 = 5;
const ITEM_PADDING: i32 = 8;
const ITEM_MARGIN: i32 = 10;
const ACTION_SIZE: i32 = 30;
const PAGE_GAP: i32 = 10;
const FAB_SIZE: i32 = 56;
const FAB_MARGIN: i32 = 14;
const FAB_ICON: f64 = 26.0;
const DIALOG_MARGIN: i32 = 20;
const DIALOG_PADDING: i32 = 16;
const DIALOG_SPACING: i32 = 16;
const PLACEHOLDER_ICON: f64 = 55.0;
const FADE_MILLIS: f64 = 200.0;
const PLACEHOLDER_MILLIS: f64 = 500.0;

pub struct TodoPage {
    pub widget: gtk4::Widget,
    tabs: Rc<SecondaryTabs>,
    swipe: Swipe,
    dialog: Rc<Fader>,
    entry: gtk4::Entry,
}

impl TodoPage {
    pub fn reset(self: &Rc<Self>) {
        self.tabs.show(0, false);
        self.swipe.show(0, false);
        self.close_dialog();
    }

    pub fn step(self: &Rc<Self>, delta: i32) {
        self.tabs.step(delta);
    }

    pub fn adding(&self) -> bool {
        self.dialog.shown()
    }

    pub fn open_dialog(&self) {
        self.dialog.show(true);
        self.entry.grab_focus();
    }

    pub fn close_dialog(&self) {
        self.dialog.show(false);
        self.entry.set_text("");
    }
}

pub fn build(theme: &SharedTheme, todo: &Todo, scope: &Scope) -> Rc<TodoPage> {
    let tabs = SecondaryTabs::new(theme, &TABS);
    let swipe = Swipe::new(PAGE_GAP);
    swipe.set_vexpand(true);
    swipe.set_margin_top(PAGE_GAP);
    let unfinished = TaskList::new(
        theme,
        todo,
        scope,
        false,
        "check_circle",
        &tr("Nothing here!"),
    );
    let finished = TaskList::new(
        theme,
        todo,
        scope,
        true,
        "checklist",
        &tr("Finished tasks will go here"),
    );
    swipe.append(&unfinished.widget);
    swipe.append(&finished.widget);
    tabs.connect_changed({
        let swipe = swipe.clone();
        move |index| swipe.show(index, true)
    });
    swipe.connect_changed({
        let tabs = Rc::downgrade(&tabs);
        move |index| {
            if let Some(tabs) = tabs.upgrade() {
                tabs.show(index, true);
            }
        }
    });

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    column.append(&tabs.widget);
    column.append(&swipe);

    let fab = fab(theme);
    let (sheet, entry, cancel, accept) = dialog(theme);
    let dialog = Fader::new(&sheet, false, FADE_MILLIS, EXPRESSIVE_EFFECTS);

    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&column));
    overlay.add_overlay(&fab);
    overlay.add_overlay(&sheet);

    let page = Rc::new(TodoPage {
        widget: overlay.upcast(),
        tabs,
        swipe,
        dialog,
        entry: entry.clone(),
    });

    fab.connect_clicked({
        let page = Rc::downgrade(&page);
        move |_| {
            if let Some(page) = page.upgrade() {
                page.open_dialog();
            }
        }
    });
    cancel.connect_clicked({
        let page = Rc::downgrade(&page);
        move |_| {
            if let Some(page) = page.upgrade() {
                page.close_dialog();
            }
        }
    });
    let commit: Rc<dyn Fn()> = {
        let page = Rc::downgrade(&page);
        let todo = todo.clone();
        Rc::new(move || {
            let Some(page) = page.upgrade() else {
                return;
            };
            let content = page.entry.text();
            if content.is_empty() {
                return;
            }
            todo.add(&content);
            page.close_dialog();
            page.tabs.select(0);
        })
    };
    entry.connect_activate({
        let commit = commit.clone();
        move |_| commit()
    });
    accept.connect_clicked(move |_| commit());
    entry.connect_changed({
        let accept = accept.clone();
        move |entry| set_enabled(&accept, !entry.text().is_empty())
    });
    set_enabled(&accept, false);

    page
}

struct TaskList {
    widget: gtk4::Overlay,
}

impl TaskList {
    fn new(
        theme: &SharedTheme,
        todo: &Todo,
        scope: &Scope,
        done: bool,
        icon: &str,
        note: &str,
    ) -> Self {
        let list = gtk4::Box::new(gtk4::Orientation::Vertical, ITEM_SPACING);
        list.set_valign(gtk4::Align::Start);
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
        scroll.set_child(Some(&list));
        crate::ui::widgets::flickable::follow_scroll_settings(&scroll);

        let symbol = text::symbol(icon, PLACEHOLDER_ICON);
        text::set_color(&symbol, "m3outline");
        let label = text::styled_sized(note, pixel_size::NORMAL);
        text::set_color(&label, "m3outline");
        label.set_justify(gtk4::Justification::Center);
        let placeholder_column = gtk4::Box::new(gtk4::Orientation::Vertical, ITEM_SPACING);
        placeholder_column.append(&Centred::integral(&symbol));
        placeholder_column.append(&Centred::new(&label));
        let placeholder = Centred::integral(&placeholder_column);
        placeholder.set_can_target(false);

        let widget = gtk4::Overlay::new();
        widget.set_child(Some(&scroll));
        widget.add_overlay(&placeholder);

        let empty = Fader::new(&placeholder, true, PLACEHOLDER_MILLIS, EXPRESSIVE_DEFAULT);
        let show: Rc<dyn Fn()> = {
            let todo = todo.clone();
            let theme = theme.clone();
            Rc::new(move || {
                while let Some(child) = list.first_child() {
                    list.remove(&child);
                }
                let mut shown = 0;
                for (index, task) in todo.list.borrow().iter().enumerate() {
                    if task.done != done {
                        continue;
                    }
                    list.append(&card(&theme, &todo, index, &task.content, task.done));
                    shown += 1;
                }
                empty.show(shown == 0);
            })
        };
        show();
        scope.keep(todo.subscribe(move || show()));

        TaskList { widget }
    }
}

fn card(theme: &SharedTheme, todo: &Todo, index: usize, content: &str, done: bool) -> gtk4::Widget {
    let words = text::styled(content);
    words.set_xalign(0.0);
    words.set_wrap(true);
    words.set_wrap_mode(gtk4::pango::WrapMode::WordChar);
    words.set_margin_start(ITEM_MARGIN);
    words.set_margin_end(ITEM_MARGIN);
    words.set_margin_top(ITEM_PADDING);

    let toggle = action(theme, if done { "remove_done" } else { "check" });
    toggle.connect_clicked({
        let todo = todo.clone();
        move |_| todo.mark(index, !done)
    });
    let discard = action(theme, "delete_forever");
    discard.connect_clicked({
        let todo = todo.clone();
        move |_| todo.remove(index)
    });

    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, ITEM_SPACING);
    buttons.set_halign(gtk4::Align::End);
    buttons.set_margin_start(ITEM_MARGIN);
    buttons.set_margin_end(ITEM_MARGIN);
    buttons.set_margin_bottom(ITEM_PADDING);
    buttons.append(&toggle);
    buttons.append(&discard);

    let column = gtk4::Box::new(gtk4::Orientation::Vertical, ITEM_SPACING);
    column.add_css_class("todo-item");
    column.append(&words);
    column.append(&buttons);
    column.upcast()
}

fn action(theme: &SharedTheme, icon: &str) -> RippleButton {
    let symbol = text::symbol(icon, pixel_size::LARGER as f64);
    text::set_color(&symbol, "colOnLayer1");
    let button = RippleButton::new(theme);
    button.set_radius(rounding::SMALL as f64);
    button.set_size_request(ACTION_SIZE, ACTION_SIZE);
    button.set_content(&Centred::new(&symbol), 0, 0);
    button
}

fn fab(theme: &SharedTheme) -> RippleButton {
    let symbol = text::symbol("add", FAB_ICON);
    text::set_color(&symbol, "colOnPrimaryContainer");
    let button = RippleButton::new(theme);
    button.add_css_class("todo-fab");
    button.set_look(Look {
        background: |theme| theme.colors.col_primary_container,
        hover: |theme| theme.colors.col_primary_container_hover,
        ripple: |theme| theme.colors.col_primary_container_active,
        ..Look::default()
    });
    button.set_radius(FAB_SIZE as f64 / 14.0 * 4.0);
    button.set_size_request(FAB_SIZE, FAB_SIZE);
    button.set_halign(gtk4::Align::End);
    button.set_valign(gtk4::Align::End);
    button.set_margin_end(FAB_MARGIN);
    button.set_margin_bottom(FAB_MARGIN);
    button.set_content(&Centred::integral(&symbol), 0, 0);
    button
}

fn dialog(theme: &SharedTheme) -> (gtk4::Box, gtk4::Entry, RippleButton, RippleButton) {
    let title = text::styled_sized(&tr("Add task"), pixel_size::LARGER);
    text::set_color(&title, "m3onSurface");
    title.set_xalign(0.0);
    title.set_margin_top(DIALOG_PADDING);
    title.set_margin_start(DIALOG_PADDING);
    title.set_margin_end(DIALOG_PADDING);

    let entry = gtk4::Entry::new();
    entry.add_css_class("todo-field");
    entry.set_placeholder_text(Some(&tr("Task description")));
    entry.set_margin_start(DIALOG_PADDING);
    entry.set_margin_end(DIALOG_PADDING);
    let (family, weight) = text::application_font();
    let mut font = gtk4::pango::FontDescription::new();
    font.set_family(&family);
    font.set_absolute_size(application_font_pixels() * gtk4::pango::SCALE as f64);
    font.set_variations(Some(&format!("wght={weight}")));
    let attributes = gtk4::pango::AttrList::new();
    attributes.insert(gtk4::pango::AttrFontDesc::new(&font));
    entry.set_attributes(&attributes);

    let cancel = windowdialog::button(theme, &tr("Cancel"));
    let accept = windowdialog::button(theme, &tr("Add"));
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, ITEM_SPACING);
    buttons.set_halign(gtk4::Align::End);
    buttons.set_margin_bottom(DIALOG_PADDING);
    buttons.set_margin_start(DIALOG_PADDING);
    buttons.set_margin_end(DIALOG_PADDING);
    buttons.append(&cancel);
    buttons.append(&accept);

    let body = gtk4::Box::new(gtk4::Orientation::Vertical, DIALOG_SPACING);
    body.add_css_class("todo-dialog");
    body.set_valign(gtk4::Align::Center);
    body.set_vexpand(true);
    body.set_margin_start(DIALOG_MARGIN);
    body.set_margin_end(DIALOG_MARGIN);
    body.append(&title);
    body.append(&entry);
    body.append(&buttons);

    let scrim = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    scrim.add_css_class("todo-scrim");
    scrim.append(&body);
    (scrim, entry, cancel, accept)
}

fn application_font_pixels() -> f64 {
    text::kdeglobals("General", "font")
        .and_then(|value| {
            value
                .split(',')
                .nth(1)
                .and_then(|size| size.parse::<f64>().ok())
        })
        .map(|points| points * 96.0 / 72.0)
        .unwrap_or(13.0)
}

fn set_enabled(button: &RippleButton, enabled: bool) {
    button.set_sensitive(enabled);
    let label = button
        .child()
        .and_then(|centred| centred.first_child())
        .and_then(|label| label.downcast::<gtk4::Label>().ok());
    if let Some(label) = label {
        text::set_color(&label, if enabled { "colPrimary" } else { "m3outline" });
    }
}
