pub mod filetypes;
pub mod windowrules;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::core::{config, tools};
use crate::panels::settings::content::{Context, Page, Style};
use crate::platform::system::defaultapps::{self, Role};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ComboBox;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const ITEM_SPACING: i32 = 10;
const OTHER: &str = "Other…";
const NOT_SET: &str = "Not set";
const DIALOG_WIDTH: f64 = 460.0;
const DIALOG_HEIGHT: f64 = 600.0;
const DIALOG_ICON: i32 = 24;
const DIALOG_ITEM_VERTICAL: i32 = 12;
const RESCAN_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

const COMMANDS: [(&str, &str, &str); 1] = [("Terminal", "/apps/terminal", "kitty -1")];

fn role_prompt(key: &str) -> &'static str {
    match key {
        "web" => "Select default web browser",
        "mail" => "Select default e-mail client",
        "calendar" => "Select default calendar application",
        "music" => "Select default music player",
        "video" => "Select default video player",
        "photos" => "Select default image viewer",
        "text" => "Select default text editor",
        "files" => "Select default file manager",
        _ => "Select default application",
    }
}

pub fn app_dialog(
    theme: &SharedTheme,
    prompt: &str,
    on_picked: impl Fn(String) + 'static,
) -> Rc<WindowDialog> {
    let dialog = WindowDialog::new(theme, Some(DIALOG_HEIGHT));
    dialog.set_background_width(DIALOG_WIDTH);
    dialog
        .column
        .add(&windowdialog::title(prompt), Place::default());
    let search_field = TextField::new(theme, Style::Outlined, &tr("Search applications"));
    dialog.column.add(&search_field.root, Place::wide());
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());

    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let empty_label = text::styled(&tr("No matches"));
    text::set_color(&empty_label, "colSubtext");
    empty_label.set_xalign(0.0);
    empty_label.set_margin_start(windowdialog::PADDING as i32);
    empty_label.set_visible(false);
    list.append(&empty_label);
    let on_picked = Rc::new(on_picked);
    let mut rows: Vec<(String, RippleButton)> = Vec::new();
    for app in defaultapps::applications() {
        let Some(id) = app.id().map(String::from) else {
            continue;
        };
        let name = app.display_name().to_string();
        let item = windowdialog::list_item(theme, false);
        let row = Row::new(ITEM_SPACING);
        if let Some(icon) = app.icon() {
            let image = gtk4::Image::from_gicon(&icon);
            image.set_pixel_size(DIALOG_ICON);
            row.append(&image);
        }
        let name_label = text::styled(&name);
        text::set_color(&name_label, "colOnLayer3");
        name_label.set_xalign(0.0);
        name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let name_box = Centered::filling_width(&name_label);
        name_box.set_hexpand(true);
        row.append(&name_box);
        item.set_content(&row, windowdialog::PADDING as i32, DIALOG_ITEM_VERTICAL);
        item.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let on_picked = on_picked.clone();
            let id = id.clone();
            move |_| {
                on_picked(id.clone());
                if let Some(dialog) = dialog.upgrade() {
                    dialog.dismiss();
                }
            }
        });
        list.append(&item);
        let words = format!("{name}\n{id}\n{}", app.executable().display()).to_lowercase();
        rows.push((words, item));
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::External);
    scroll.set_child(Some(&list));
    crate::ui::widgets::input::flickable::follow_scroll_settings(&scroll);
    dialog.column.add(
        &scroll,
        Place {
            left: -windowdialog::PADDING,
            right: -windowdialog::PADDING,
            fills_width: true,
            fills_height: true,
            ..Place::default()
        },
    );
    search_field.connect_changed({
        let search_field = Rc::downgrade(&search_field);
        move || {
            let Some(search_field) = search_field.upgrade() else {
                return;
            };
            let query = search_field.text().trim().to_lowercase();
            let mut has_matches = false;
            for (words, item) in &rows {
                let is_match = words.contains(&query);
                item.set_visible(is_match);
                has_matches |= is_match;
            }
            empty_label.set_visible(!has_matches);
        }
    });

    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
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
    dialog.column.add(&buttons, place);
    glib::idle_add_local_once({
        let search_field = Rc::downgrade(&search_field);
        move || {
            if let Some(search_field) = search_field.upgrade() {
                search_field.grab_focus();
            }
        }
    });
    dialog.keep(search_field);
    dialog
}

fn role_icon(key: &str) -> &'static str {
    match key {
        "web" => "language",
        "mail" => "mail",
        "calendar" => "calendar_month",
        "music" => "music_note",
        "video" => "movie",
        "photos" => "image",
        "text" => "description",
        "files" => "folder",
        _ => "apps",
    }
}

struct Roles {
    page: Weak<Page>,
    section: gtk4::Box,
    combos: RefCell<Vec<Rc<ComboBox>>>,
    present: Rc<dyn Fn(Rc<WindowDialog>)>,
}

impl Roles {
    fn reload(self: &Rc<Self>) {
        let roles = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(role_list) = gio::spawn_blocking(defaultapps::read).await else {
                return;
            };
            if let Some(roles) = roles.upgrade() {
                roles.show(role_list);
            }
        });
    }

    fn assign(self: &Rc<Self>, key: &'static str, entry: String) {
        let roles = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let _ = gio::spawn_blocking(move || defaultapps::set(key, &entry)).await;
            if let Some(roles) = roles.upgrade() {
                roles.reload();
            }
        });
    }

    fn show(self: &Rc<Self>, role_list: Vec<Role>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        while let Some(child) = self.section.first_child() {
            self.section.remove(&child);
        }
        let mut combos = Vec::new();
        for role in role_list {
            let (group, _) = page.unkept_subsection(&self.section, &tr(role.title), "");
            let combo = ComboBox::new(&page.theme);
            combo.set_icon(role_icon(role.key));
            combo.button.set_hexpand(true);
            group.append(&combo.button);
            let mut names: Vec<String> = Vec::new();
            let mut entries: Vec<Option<String>> = Vec::new();
            if role.default.is_empty() {
                names.push(tr(NOT_SET));
                entries.push(None);
            }
            for candidate in role.candidates {
                names.push(candidate.name);
                entries.push(Some(candidate.entry));
            }
            names.push(tr(OTHER));
            let default_index = entries
                .iter()
                .position(|entry| entry.as_deref().unwrap_or_default() == role.default)
                .unwrap_or(0);
            combo.set_items(&names, default_index as i32);
            let key = role.key;
            combo.connect_activated({
                let roles = Rc::downgrade(self);
                let combo = Rc::downgrade(&combo);
                let theme = page.theme.clone();
                move |selected_index| {
                    let Some(roles) = roles.upgrade() else {
                        return;
                    };
                    match entries.get(selected_index) {
                        Some(Some(entry)) => roles.assign(key, entry.clone()),
                        Some(None) => {}
                        None => {
                            if let Some(combo) = combo.upgrade() {
                                combo.set_items(&names, default_index as i32);
                            }
                            let dialog = app_dialog(&theme, &tr(role_prompt(key)), {
                                let roles = Rc::downgrade(&roles);
                                move |entry| {
                                    if let Some(roles) = roles.upgrade() {
                                        roles.assign(key, entry);
                                    }
                                }
                            });
                            (roles.present)(dialog);
                        }
                    }
                }
            });
            combos.push(combo);
        }
        self.combos.replace(combos);
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let defaults_section = page.section("apps", &tr("Default Apps"));
    let roles = Rc::new(Roles {
        page: Rc::downgrade(&page),
        section: defaults_section.clone(),
        combos: RefCell::new(Vec::new()),
        present: Rc::new(context.dialog_presenter()),
    });
    roles.reload();
    let rescan_timer = Rc::new(Cell::new(None::<glib::SourceId>));
    let monitor = gio::AppInfoMonitor::get();
    let handler_id = monitor.connect_changed({
        let roles = Rc::downgrade(&roles);
        let rescan_timer = rescan_timer.clone();
        move |_| {
            if let Some(source) = rescan_timer.take() {
                source.remove();
            }
            let roles = roles.clone();
            let timer_slot = rescan_timer.clone();
            rescan_timer.set(Some(glib::timeout_add_local_once(
                RESCAN_DELAY,
                move || {
                    timer_slot.set(None);
                    if let Some(roles) = roles.upgrade() {
                        roles.reload();
                    }
                },
            )));
        }
    });
    page.keep(roles);
    page.keep(Disconnect(monitor, Some(handler_id), rescan_timer));

    let types_section = page.section("description", &tr("File types"));
    page.link_row(
        &types_section,
        "",
        &tr("Every file type"),
        &tr("Which app opens each kind of file and link"),
        context.subpage_opener("filetypes"),
    );

    let windows_section = page.section("select_window", &tr("Window rules"));
    page.link_row(
        &windows_section,
        "",
        &tr("Every window rule"),
        &tr("What each application's windows do"),
        context.subpage_opener("windowrules"),
    );

    let commands_section = page.section("terminal", &tr("Commands"));
    let button_commands_section = page.subsection(
        &commands_section,
        &tr("What the shell's own buttons open"),
        "",
    );
    for (title, pointer, default) in COMMANDS {
        page.config_text(
            &button_commands_section,
            Style::Outlined,
            &tr(title),
            pointer,
            default,
        );
    }
    let current_notice: Rc<RefCell<Option<gtk4::Box>>> = Rc::new(RefCell::new(None));
    let update_notice = {
        let page = Rc::downgrade(&page);
        move || {
            let Some(page) = page.upgrade() else {
                return;
            };
            if let Some(notice) = current_notice.take() {
                button_commands_section.remove(&notice);
            }
            let missing_programs = missing_commands();
            if !missing_programs.is_empty() {
                let message = trf(
                    "Not installed: %1. The buttons that run these do nothing until the command names an installed program.",
                    &[&missing_programs.join(", ")],
                );
                current_notice.replace(Some(page.notice(
                    &button_commands_section,
                    "info",
                    &message,
                )));
            }
        }
    };
    update_notice();
    for (_, pointer, _) in COMMANDS {
        page.watch(pointer, update_notice.clone());
    }
    page
}

fn missing_commands() -> Vec<String> {
    COMMANDS
        .iter()
        .filter_map(|(title, pointer, default)| {
            let command = config::value_str(pointer).unwrap_or_else(|| (*default).to_owned());
            let program = tools::find_missing_program(&command)?;
            Some(format!("{program} ({})", tr(title)))
        })
        .collect()
}

struct Disconnect(
    gio::AppInfoMonitor,
    Option<glib::SignalHandlerId>,
    Rc<Cell<Option<glib::SourceId>>>,
);

impl Drop for Disconnect {
    fn drop(&mut self) {
        if let Some(handler_id) = self.1.take() {
            self.0.disconnect(handler_id);
        }
        if let Some(source) = self.2.take() {
            source.remove();
        }
    }
}
