use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page, Style};
use crate::panels::settings::pages::apps::app_dialog;
use crate::platform::autostart::{self, Entry};
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::Switch;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

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
const BUTTON_ROW_BOTTOM: f64 = 10.0;

type Present = Rc<dyn Fn(Rc<WindowDialog>)>;

struct Autostart {
    page: Weak<Page>,
    present: Present,
    list: gtk4::Box,
    status: gtk4::Label,
    rows: RefCell<Vec<Box<dyn Any>>>,
}

impl Autostart {
    fn report(&self, result: Result<(), String>) {
        let message = result.err().unwrap_or_default();
        self.status.set_visible(!message.is_empty());
        self.status.set_text(&message);
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
        let mut rows: Vec<Box<dyn Any>> = Vec::new();
        if entries.is_empty() {
            let empty = text::styled(&tr("Nothing starts with the session yet"));
            text::set_color(&empty, "colSubtext");
            empty.set_xalign(0.0);
            empty.set_margin_start(NOTE_START);
            self.list.append(&empty);
        }
        for entry in entries {
            rows.extend(self.card(&page, &entry));
        }
        self.rows.replace(rows);
    }

    fn card(self: &Rc<Self>, page: &Page, entry: &Entry) -> Vec<Box<dyn Any>> {
        let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        card.add_css_class("settings-row-card");
        card.set_size_request(-1, CARD_HEIGHT);
        let inside = Row::new(CARD_SPACING);
        inside.set_margin_start(CARD_START);
        inside.set_margin_end(CARD_END);
        inside.set_hexpand(true);
        if entry.icon.is_empty() {
            let symbol = text::symbol("terminal", pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnLayer2");
            inside.append(&Centred::integral(&symbol));
        } else {
            let image = if entry.icon.starts_with('/') {
                gtk4::Image::from_file(&entry.icon)
            } else {
                gtk4::Image::from_gicon(&gio::ThemedIcon::new(&entry.icon))
            };
            image.set_pixel_size(ICON);
            inside.append(&image);
        }
        let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
        lines.set_hexpand(true);
        lines.set_valign(gtk4::Align::Center);
        let name = text::styled(&entry.name);
        text::set_color(&name, "colOnLayer2");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centred::filling_width(&name));
        let command = text::styled_sized(&entry.command, pixel_size::SMALLER);
        text::set_color(&command, "colSubtext");
        command.set_xalign(0.0);
        command.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        lines.append(&Centred::filling_width(&command));
        inside.append(&lines);

        let switch = Switch::new(&page.theme);
        switch.set(entry.enabled);
        switch.connect_clicked({
            let autostart = Rc::downgrade(self);
            let (path, enabled) = (entry.path.clone(), entry.enabled);
            move || {
                if let Some(autostart) = autostart.upgrade() {
                    autostart.change(autostart::set_enabled(&path, !enabled));
                }
            }
        });
        inside.append(&switch.area);

        let remove = RippleButton::new(&page.theme);
        remove.set_radius(REMOVE_SIZE as f64 / 2.0);
        remove.set_size_request(REMOVE_SIZE, REMOVE_SIZE);
        remove.set_valign(gtk4::Align::Center);
        let symbol = text::symbol("remove", REMOVE_ICON);
        text::set_color(&symbol, "colOnLayer2");
        remove.set_content(&Centred::integral(&symbol), 0, 0);
        remove.connect_clicked({
            let autostart = Rc::downgrade(self);
            let path = entry.path.clone();
            move |_| {
                if let Some(autostart) = autostart.upgrade() {
                    autostart.change(autostart::remove(&path));
                }
            }
        });
        let tip = page.unkept_tip(&remove, &tr("Remove"));
        inside.append(&remove);
        card.append(&inside);
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
        let name = TextField::new(theme, Style::Outlined, &tr("Name"));
        dialog.column.add(&name.root, Place::wide());
        let command = TextField::new(theme, Style::Outlined, &tr("Command"));
        dialog.column.add(&command.root, Place::wide());
        let (buttons, mut place) = windowdialog::button_row();
        place.bottom = BUTTON_ROW_BOTTOM;
        buttons.append(&windowdialog::spacer());
        let cancel = windowdialog::button(theme, &tr("Cancel"));
        cancel.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            move |_| {
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        buttons.append(&cancel);
        let add = windowdialog::button(theme, &tr("Add"));
        add.set_sensitive(false);
        add.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let autostart = Rc::downgrade(self);
            let (name, command) = (Rc::downgrade(&name), Rc::downgrade(&command));
            move |_| {
                let (Some(dialog), Some(autostart), Some(name), Some(command)) = (
                    dialog.upgrade(),
                    autostart.upgrade(),
                    name.upgrade(),
                    command.upgrade(),
                ) else {
                    return;
                };
                dialog.dismiss();
                autostart.change(autostart::add_command(
                    name.text().trim(),
                    command.text().trim(),
                ));
            }
        });
        buttons.append(&add);
        dialog.column.add(&buttons, place);
        command.connect_changed({
            let command = Rc::downgrade(&command);
            let add = add.clone();
            move || {
                if let Some(command) = command.upgrade() {
                    add.set_sensitive(!command.text().trim().is_empty());
                }
            }
        });
        glib::idle_add_local_once({
            let name = Rc::downgrade(&name);
            move || {
                if let Some(name) = name.upgrade() {
                    name.grab_focus();
                }
            }
        });
        dialog.keep(name);
        dialog.keep(command);
        (self.present)(dialog);
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let section = page.section("", "");
    let explanation = text::styled(&tr(
        "The shell starts these once per session, as it comes up. Entries come from ~/.config/autostart",
    ));
    text::set_color(&explanation, "colSubtext");
    explanation.set_xalign(0.0);
    explanation.set_wrap(true);
    explanation.set_margin_start(NOTE_START);
    section.append(&explanation);
    let section = page.section("start", &tr("Starts with the session"));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    section.append(&list);
    let status = text::styled("");
    text::set_color(&status, "colError");
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_margin_start(NOTE_START);
    status.set_visible(false);
    let autostart = Rc::new(Autostart {
        page: Rc::downgrade(&page),
        present: Rc::new(context.dialog_presenter()),
        list,
        status: status.clone(),
        rows: RefCell::new(Vec::new()),
    });
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
    buttons.set_margin_top(BUTTON_TOP);
    let (application, _) = page.icon_button("apps", true, &tr("Add application…"), {
        let autostart = Rc::downgrade(&autostart);
        move || {
            if let Some(autostart) = autostart.upgrade() {
                autostart.add_application();
            }
        }
    });
    buttons.append(&application);
    let (command, _) = page.icon_button("terminal", false, &tr("Add command…"), {
        let autostart = Rc::downgrade(&autostart);
        move || {
            if let Some(autostart) = autostart.upgrade() {
                autostart.add_command();
            }
        }
    });
    buttons.append(&command);
    section.append(&buttons);
    section.append(&status);
    autostart.reload();
    page.keep(autostart);
    page
}
