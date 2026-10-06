use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::core::{config, tools};
use crate::panels::settings::content::{Context, Page, Style};
use crate::platform::defaultapps::{self, Role};
use crate::ui::theme::SharedTheme;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::ComboBox;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

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
    picked: impl Fn(String) + 'static,
) -> Rc<WindowDialog> {
    let dialog = WindowDialog::new(theme, Some(DIALOG_HEIGHT));
    dialog.set_background_width(DIALOG_WIDTH);
    dialog
        .column
        .add(&windowdialog::title(prompt), Place::default());
    let search = TextField::new(theme, Style::Outlined, &tr("Search applications"));
    dialog.column.add(&search.root, Place::wide());
    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());

    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let empty = text::styled(&tr("No matches"));
    text::set_color(&empty, "colSubtext");
    empty.set_xalign(0.0);
    empty.set_margin_start(windowdialog::PADDING as i32);
    empty.set_visible(false);
    list.append(&empty);
    let picked = Rc::new(picked);
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
        let label = text::styled(&name);
        text::set_color(&label, "colOnLayer3");
        label.set_xalign(0.0);
        label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        let label = Centred::filling_width(&label);
        label.set_hexpand(true);
        row.append(&label);
        item.set_content(&row, windowdialog::PADDING as i32, DIALOG_ITEM_VERTICAL);
        item.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let picked = picked.clone();
            let id = id.clone();
            move |_| {
                picked(id.clone());
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
    crate::ui::widgets::flickable::follow_scroll_settings(&scroll);
    dialog.column.add(
        &scroll,
        Place {
            left: -windowdialog::PADDING,
            right: -windowdialog::PADDING,
            fill_width: true,
            fill_height: true,
            ..Place::default()
        },
    );
    search.connect_changed({
        let search = Rc::downgrade(&search);
        move || {
            let Some(search) = search.upgrade() else {
                return;
            };
            let query = search.text().trim().to_lowercase();
            let mut shown = false;
            for (words, item) in &rows {
                let matches = words.contains(&query);
                item.set_visible(matches);
                shown |= matches;
            }
            empty.set_visible(!shown);
        }
    });

    dialog
        .column
        .add(&windowdialog::separator(), windowdialog::separator_place());
    let (buttons, place) = windowdialog::button_row();
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
    dialog.column.add(&buttons, place);
    glib::idle_add_local_once({
        let search = Rc::downgrade(&search);
        move || {
            if let Some(search) = search.upgrade() {
                search.grab_focus();
            }
        }
    });
    dialog.keep(search);
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
    holder: gtk4::Box,
    combos: RefCell<Vec<Rc<ComboBox>>>,
    present: Rc<dyn Fn(Rc<WindowDialog>)>,
}

impl Roles {
    fn reload(self: &Rc<Self>) {
        let roles = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let Ok(found) = gio::spawn_blocking(defaultapps::read).await else {
                return;
            };
            if let Some(roles) = roles.upgrade() {
                roles.show(found);
            }
        });
    }

    fn show(self: &Rc<Self>, found: Vec<Role>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        while let Some(child) = self.holder.first_child() {
            self.holder.remove(&child);
        }
        let mut combos = Vec::new();
        for role in found {
            let (group, _) = page.unkept_subsection(&self.holder, &tr(role.title), "");
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
            let index = entries
                .iter()
                .position(|entry| entry.as_deref().unwrap_or_default() == role.default)
                .unwrap_or(0);
            combo.set_items(&names, index as i32);
            let key = role.key;
            combo.connect_activated({
                let roles = Rc::downgrade(self);
                let combo = Rc::downgrade(&combo);
                let theme = page.theme.clone();
                move |chosen| {
                    let Some(roles) = roles.upgrade() else {
                        return;
                    };
                    match entries.get(chosen) {
                        Some(Some(entry)) => {
                            defaultapps::set(key, entry);
                            roles.reload();
                        }
                        Some(None) => {}
                        None => {
                            if let Some(combo) = combo.upgrade() {
                                combo.set_items(&names, index as i32);
                            }
                            let dialog = app_dialog(&theme, &tr(role_prompt(key)), {
                                let roles = Rc::downgrade(&roles);
                                move |entry| {
                                    defaultapps::set(key, &entry);
                                    if let Some(roles) = roles.upgrade() {
                                        roles.reload();
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

    let defaults = page.section("apps", &tr("Default Apps"));
    let roles = Rc::new(Roles {
        page: Rc::downgrade(&page),
        holder: defaults.clone(),
        combos: RefCell::new(Vec::new()),
        present: Rc::new(context.dialog_presenter()),
    });
    roles.reload();
    let rescan = Rc::new(Cell::new(None::<glib::SourceId>));
    let monitor = gio::AppInfoMonitor::get();
    let watched = monitor.connect_changed({
        let roles = Rc::downgrade(&roles);
        let rescan = rescan.clone();
        move |_| {
            if let Some(source) = rescan.take() {
                source.remove();
            }
            let roles = roles.clone();
            let pending = rescan.clone();
            rescan.set(Some(glib::timeout_add_local_once(
                RESCAN_DELAY,
                move || {
                    pending.set(None);
                    if let Some(roles) = roles.upgrade() {
                        roles.reload();
                    }
                },
            )));
        }
    });
    page.keep(roles);
    page.keep(Disconnect(monitor, Some(watched), rescan));

    let types = page.section("description", &tr("File types"));
    page.link_row(
        &types,
        "",
        &tr("Every file type"),
        &tr("Which app opens each kind of file and link"),
        context.subpage_opener("filetypes"),
    );

    let windows = page.section("select_window", &tr("Window rules"));
    page.link_row(
        &windows,
        "",
        &tr("Every window rule"),
        &tr("What each application's windows do"),
        context.subpage_opener("windowrules"),
    );

    let commands = page.section("terminal", &tr("Commands"));
    let opened = page.subsection(&commands, &tr("What the shell's own buttons open"), "");
    for (placeholder, pointer, default) in COMMANDS {
        page.config_text(&opened, Style::Outlined, &tr(placeholder), pointer, default);
    }
    let shown: Rc<RefCell<Option<gtk4::Box>>> = Rc::new(RefCell::new(None));
    let follow = {
        let page = Rc::downgrade(&page);
        move || {
            let Some(page) = page.upgrade() else {
                return;
            };
            if let Some(notice) = shown.take() {
                opened.remove(&notice);
            }
            let missing = missing_commands();
            if !missing.is_empty() {
                let message = trf(
                    "Not installed: %1. The buttons that run these do nothing until the command names an installed program.",
                    &[&missing.join(", ")],
                );
                shown.replace(Some(page.notice(&opened, "info", &message)));
            }
        }
    };
    follow();
    for (_, pointer, _) in COMMANDS {
        page.watch(pointer, follow.clone());
    }
    page
}

fn missing_commands() -> Vec<String> {
    COMMANDS
        .iter()
        .filter_map(|(name, pointer, default)| {
            let command = config::value_str(pointer).unwrap_or_else(|| (*default).to_owned());
            let program = tools::command_missing(&command)?;
            Some(format!("{program} ({})", tr(name)))
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
        if let Some(handler) = self.1.take() {
            self.0.disconnect(handler);
        }
        if let Some(source) = self.2.take() {
            source.remove();
        }
    }
}
