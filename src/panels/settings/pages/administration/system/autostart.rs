use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::panels::settings::pages::software::apps::app_dialog;
use crate::platform::system::autostart::{self, Entry};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::Switch;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NOTE_START: i32 = 8;
const CARD_HEIGHT: i32 = 56;
const CARD_START: i32 = 12;
const CARD_END: i32 = 8;
const CARD_SPACING: i32 = 10;
const ICON: i32 = 24;
const REMOVE_SIZE: i32 = 32;
const REMOVE_ICON: f64 = 20.0;
const BUTTON_TOP: i32 = 4;
const BUTTON_SPACING: i32 = 5;
const DIALOG_WIDTH: f64 = 420.0;

type Present = Rc<dyn Fn(Rc<WindowDialog>)>;

struct Autostart {
    page: Weak<Page>,
    present: Present,
    list: gtk4::Box,
    status_label: gtk4::Label,
    row_items: RefCell<Vec<Box<dyn Any>>>,
}

impl Autostart {
    fn report(&self, result: Result<(), String>) {
        let message = result.err().unwrap_or_default();
        self.status_label.set_visible(!message.is_empty());
        self.status_label.set_text(&message);
    }

    fn change(self: &Rc<Self>, result: Result<(), String>) {
        self.report(result);
        self.reload();
    }

    fn reload(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let entries = autostart::list();
        let mut row_items: Vec<Box<dyn Any>> = Vec::new();
        if entries.is_empty() {
            let empty_label = text::styled(&tr("Nothing starts with the session yet"));
            text::set_color(&empty_label, "colSubtext");
            empty_label.set_xalign(0.0);
            empty_label.set_margin_start(NOTE_START);
            self.list.append(&empty_label);
        }
        for entry in entries {
            row_items.extend(self.card(&page, &entry));
        }
        self.row_items.replace(row_items);
    }

    fn card(self: &Rc<Self>, page: &Page, entry: &Entry) -> Vec<Box<dyn Any>> {
        let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        card.add_css_class("settings-row-card");
        card.set_size_request(-1, CARD_HEIGHT);
        let content = Row::new(CARD_SPACING);
        content.set_margin_start(CARD_START);
        content.set_margin_end(CARD_END);
        content.set_hexpand(true);
        if entry.icon.is_empty() {
            let symbol = text::symbol("terminal", pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnLayer2");
            content.append(&Centered::integral(&symbol));
        } else {
            let image = if entry.icon.starts_with('/') {
                gtk4::Image::from_file(&entry.icon)
            } else {
                gtk4::Image::from_gicon(&gio::ThemedIcon::new(&entry.icon))
            };
            image.set_pixel_size(ICON);
            content.append(&image);
        }
        let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        lines.set_hexpand(true);
        lines.set_valign(gtk4::Align::Center);
        let name_label = text::styled(&entry.name);
        text::set_color(&name_label, "colOnLayer2");
        name_label.set_xalign(0.0);
        name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centered::filling_width(&name_label));
        let command_label = text::styled_sized(&entry.command, pixel_size::SMALLER);
        text::set_color(&command_label, "colSubtext");
        command_label.set_xalign(0.0);
        command_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centered::filling_width(&command_label));
        content.append(&lines);

        let switch = Switch::new(&page.theme);
        switch.set(entry.is_enabled);
        switch.connect_clicked({
            let autostart = Rc::downgrade(self);
            let (path, is_enabled) = (entry.path.clone(), entry.is_enabled);
            move || {
                if let Some(autostart) = autostart.upgrade() {
                    autostart.change(autostart::set_enabled(&path, !is_enabled));
                }
            }
        });
        content.append(&switch.area);

        let remove_button = RippleButton::new(&page.theme);
        remove_button.set_radius(REMOVE_SIZE as f64 / 2.0);
        remove_button.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
        remove_button.set_valign(gtk4::Align::Center);
        let symbol = text::symbol("remove", REMOVE_ICON);
        text::set_color(&symbol, "colOnLayer2");
        remove_button.set_content(&Centered::integral(&symbol), 0, 0);
        remove_button.connect_clicked({
            let autostart = Rc::downgrade(self);
            let path = entry.path.clone();
            move |_| {
                if let Some(autostart) = autostart.upgrade() {
                    autostart.change(autostart::remove(&path));
                }
            }
        });
        let tip = page.unkept_tip(&remove_button, &tr("Remove"));
        content.append(&remove_button);
        card.append(&content);
        self.list.append(&card);
        vec![Box::new(switch), Box::new(tip)]
    }

    fn add_application(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let dialog = app_dialog(&page.theme, &tr("Start an application with the session"), {
            let autostart = Rc::downgrade(self);
            move |id| {
                if let Some(autostart) = autostart.upgrade() {
                    autostart.change(autostart::add_application(&id));
                }
            }
        });
        (self.present)(dialog);
    }

    fn add_command(self: &Rc<Self>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let theme = &page.theme;
        let dialog = WindowDialog::new(theme, None);
        dialog.set_background_width(DIALOG_WIDTH);
        dialog
            .column
            .add(&windowdialog::title(&tr("Start a command")), Place::wide());
        let name_field = TextField::new(theme, Style::Outlined, &tr("Name"));
        dialog.column.add(&name_field.root, Place::wide());
        let command_field = TextField::new(theme, Style::Outlined, &tr("Command"));
        dialog.column.add(&command_field.root, Place::wide());
        let (buttons, place) = windowdialog::button_row();
        buttons.append(&windowdialog::spacer());
        let cancel_button = windowdialog::button(theme, &tr("Cancel"));
        cancel_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        buttons.append(&cancel_button);
        let add_button = windowdialog::button(theme, &tr("Add"));
        add_button.set_sensitive(false);
        add_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let autostart = Rc::downgrade(self);
            let (name_field, command_field) =
                (Rc::downgrade(&name_field), Rc::downgrade(&command_field));
            move |_| {
                let (Some(dialog), Some(autostart), Some(name_field), Some(command_field)) = (
                    dialog.upgrade(),
                    autostart.upgrade(),
                    name_field.upgrade(),
                    command_field.upgrade(),
                ) else {
                    return;
                };
                dialog.dismiss();
                autostart.change(autostart::add_command(
                    name_field.text().trim(),
                    command_field.text().trim(),
                ));
            }
        });
        buttons.append(&add_button);
        dialog.column.add(&buttons, place);
        command_field.connect_changed({
            let command_field = Rc::downgrade(&command_field);
            let add_button = add_button.clone();
            move || {
                if let Some(command_field) = command_field.upgrade() {
                    add_button.set_sensitive(!command_field.text().trim().is_empty());
                }
            }
        });
        glib::idle_add_local_once({
            let name_field = Rc::downgrade(&name_field);
            move || {
                if let Some(name_field) = name_field.upgrade() {
                    name_field.grab_focus();
                }
            }
        });
        dialog.keep(name_field);
        dialog.keep(command_field);
        (self.present)(dialog);
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let explanation_section = page.section("", "");
    let explanation_label = text::styled(&tr(
        "The shell starts these once per session, as it comes up. Entries come from ~/.config/autostart",
    ));
    text::set_color(&explanation_label, "colSubtext");
    explanation_label.set_xalign(0.0);
    explanation_label.set_wrap(true);
    explanation_label.set_margin_start(NOTE_START);
    explanation_section.append(&explanation_label);
    let entries_section = page.section("start", &tr("Starts with the session"));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    entries_section.append(&list);
    let status_label = text::styled("");
    text::set_color(&status_label, "colError");
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.set_margin_start(NOTE_START);
    status_label.set_visible(false);
    let autostart = Rc::new(Autostart {
        page: Rc::downgrade(&page),
        present: Rc::new(context.dialog_presenter()),
        list,
        status_label: status_label.clone(),
        row_items: RefCell::new(Vec::new()),
    });
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
    buttons.set_margin_top(BUTTON_TOP);
    let (application_button, _) = page.icon_button("apps", true, &tr("Add application…"), {
        let autostart = Rc::downgrade(&autostart);
        move || {
            if let Some(autostart) = autostart.upgrade() {
                autostart.add_application();
            }
        }
    });
    buttons.append(&application_button);
    let (command_button, _) = page.icon_button("terminal", false, &tr("Add command…"), {
        let autostart = Rc::downgrade(&autostart);
        move || {
            if let Some(autostart) = autostart.upgrade() {
                autostart.add_command();
            }
        }
    });
    buttons.append(&command_button);
    entries_section.append(&buttons);
    entries_section.append(&status_label);
    autostart.reload();
    page.keep(autostart);
    page
}
