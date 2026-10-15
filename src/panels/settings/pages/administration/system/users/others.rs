use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::platform::security::pam;
use crate::services::session::accounts::{self, User};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::{ConfigSwitch, icon_button};
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;
use crate::ui::widgets::overlay::windowdialog::{self, Place, WindowDialog};

const NOTE_START: i32 = 8;
const CARD_HEIGHT: i32 = 56;
const CARD_START: i32 = 12;
const CARD_END: i32 = 8;
const CARD_SPACING: i32 = 10;
const BUTTON_TOP: i32 = 4;
const DIALOG_WIDTH: f64 = 420.0;

type Present = Rc<dyn Fn(Rc<WindowDialog>)>;

struct Others {
    page: Weak<Page>,
    present: Present,
    list: gtk4::Box,
    status_label: gtk4::Label,
    row_items: RefCell<Vec<Box<dyn Any>>>,
}

impl Others {
    fn reload(self: &Rc<Self>) {
        let others = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let users = accounts::others().await;
            if let Some(others) = others.upgrade() {
                others.fill(users);
            }
        });
    }

    fn report(&self, result: Result<(), String>, done_message: &str) {
        let (message, color) = match result {
            Ok(()) => (done_message.to_owned(), "colSubtext"),
            Err(message) => (message, "colError"),
        };
        self.status_label.set_text(&message);
        self.status_label.set_visible(!message.is_empty());
        text::set_color(&self.status_label, color);
    }

    fn run(
        self: &Rc<Self>,
        done_message: String,
        call: impl Future<Output = Result<(), String>> + 'static,
    ) {
        let others = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = call.await;
            if let Some(others) = others.upgrade() {
                others.report(result, &done_message);
                others.reload();
            }
        });
    }

    fn fill(self: &Rc<Self>, users: Vec<User>) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let mut row_items: Vec<Box<dyn Any>> = Vec::new();
        if users.is_empty() {
            let empty_label = text::styled(&tr("Nobody else has an account here"));
            text::set_color(&empty_label, "colSubtext");
            empty_label.set_xalign(0.0);
            empty_label.set_margin_start(NOTE_START);
            self.list.append(&empty_label);
        }
        for user in users {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, CARD_HEIGHT);
            let content = Row::new(CARD_SPACING);
            content.set_margin_start(CARD_START);
            content.set_margin_end(CARD_END);
            content.set_hexpand(true);
            let symbol = text::symbol("person", pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnLayer2");
            content.append(&Centered::integral(&symbol));
            let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            lines.set_hexpand(true);
            lines.set_valign(gtk4::Align::Center);
            let name_label = text::styled(user.display_name());
            text::set_color(&name_label, "colOnLayer2");
            name_label.set_xalign(0.0);
            name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centered::filling_width(&name_label));
            let detail_label = text::styled_sized(
                &if user.has_password {
                    user.user_name.clone()
                } else {
                    trf("%1 · password asked at first login", &[&user.user_name])
                },
                pixel_size::SMALLER,
            );
            text::set_color(&detail_label, "colSubtext");
            detail_label.set_xalign(0.0);
            detail_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centered::filling_width(&detail_label));
            content.append(&lines);

            let administrator_switch = ConfigSwitch::new(&page.theme, "", &tr("Administrator"), {
                let others = Rc::downgrade(self);
                let path = user.path.clone();
                move |is_on| {
                    if let Some(others) = others.upgrade() {
                        let path = path.clone();
                        others.run(String::new(), async move {
                            accounts::set_administrator(&path, is_on).await
                        });
                    }
                }
            });
            administrator_switch.set(user.is_administrator);
            content.append(&administrator_switch.button);
            row_items.push(Box::new(administrator_switch));

            let (password_button, _) = icon_button(&page.theme, "password", false, "");
            password_button.connect_clicked({
                let others = Rc::downgrade(self);
                let user = user.clone();
                move |_| {
                    if let Some(others) = others.upgrade() {
                        others.ask_password(&user);
                    }
                }
            });
            row_items.push(Box::new(
                page.unkept_tip(&password_button, &tr("Set password…")),
            ));
            content.append(&password_button);

            let (delete_button, _) = icon_button(&page.theme, "person_remove", false, "");
            delete_button.connect_clicked({
                let others = Rc::downgrade(self);
                let user = user.clone();
                move |_| {
                    if let Some(others) = others.upgrade() {
                        others.confirm_delete(&user);
                    }
                }
            });
            row_items.push(Box::new(
                page.unkept_tip(&delete_button, &tr("Delete account…")),
            ));
            content.append(&delete_button);
            card.append(&content);
            self.list.append(&card);
        }
        self.row_items.replace(row_items);
    }

    fn ask_password(self: &Rc<Self>, user: &User) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let theme = &page.theme;
        let dialog = WindowDialog::new(theme, None);
        dialog.set_background_width(DIALOG_WIDTH);
        dialog.column.add(
            &windowdialog::title(&trf("Password for %1", &[user.display_name()])),
            Place::wide(),
        );
        let password_field = TextField::secret(theme, Style::Outlined, &tr("New password"));
        dialog.column.add(&password_field.root, Place::wide());
        let repeat_field = TextField::secret(theme, Style::Outlined, &tr("Repeat new password"));
        dialog.column.add(&repeat_field.root, Place::wide());
        let mismatch_label = text::styled(&tr("The new passwords do not match"));
        text::set_color(&mismatch_label, "m3error");
        mismatch_label.set_xalign(0.0);
        mismatch_label.set_visible(false);
        dialog.column.add(&mismatch_label, Place::wide());
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
        let set_button = windowdialog::button(theme, &tr("Set password"));
        set_button.set_sensitive(false);
        set_button.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let others = Rc::downgrade(self);
            let password_field = Rc::downgrade(&password_field);
            let path = user.path.clone();
            move |_| {
                let (Some(dialog), Some(others), Some(password_field)) =
                    (dialog.upgrade(), others.upgrade(), password_field.upgrade())
                else {
                    return;
                };
                let password = password_field.text();
                let path = path.clone();
                dialog.dismiss();
                others.run(tr("Password set"), async move {
                    let result = accounts::set_password(&path, &password).await;
                    pam::wipe(&mut password.into_bytes());
                    result
                });
            }
        });
        buttons.append(&set_button);
        dialog.column.add(&buttons, place);
        let update_set_button = {
            let (password_field, repeat_field) =
                (Rc::downgrade(&password_field), Rc::downgrade(&repeat_field));
            let set_button = set_button.clone();
            move || {
                let (Some(password_field), Some(repeat_field)) =
                    (password_field.upgrade(), repeat_field.upgrade())
                else {
                    return;
                };
                let (password, repeated_password) = (password_field.text(), repeat_field.text());
                mismatch_label
                    .set_visible(!repeated_password.is_empty() && password != repeated_password);
                set_button.set_sensitive(!password.is_empty() && password == repeated_password);
                pam::wipe(&mut password.into_bytes());
                pam::wipe(&mut repeated_password.into_bytes());
            }
        };
        let update_set_button = Rc::new(update_set_button);
        for field in [&password_field, &repeat_field] {
            let update_set_button = update_set_button.clone();
            field.connect_changed(move || update_set_button());
        }
        glib::idle_add_local_once({
            let password_field = Rc::downgrade(&password_field);
            move || {
                if let Some(password_field) = password_field.upgrade() {
                    password_field.grab_focus();
                }
            }
        });
        dialog.keep(password_field);
        dialog.keep(repeat_field);
        (self.present)(dialog);
    }

    fn confirm_delete(self: &Rc<Self>, user: &User) {
        let Some(page) = self.page.upgrade() else {
            return;
        };
        let theme = &page.theme;
        let dialog = WindowDialog::new(theme, None);
        dialog.set_background_width(DIALOG_WIDTH);
        dialog.column.add(
            &windowdialog::title(&trf("Delete %1?", &[user.display_name()])),
            Place::wide(),
        );
        let description = text::styled(&tr(
            "The account goes away for good. Its home folder can stay, for whoever needs the files",
        ));
        text::set_color(&description, "colOnSurfaceVariant");
        description.set_wrap(true);
        description.set_xalign(0.0);
        dialog.column.add(&description, Place::wide());
        let (buttons, place) = windowdialog::button_row();
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
        buttons.append(&windowdialog::spacer());
        for (label, should_remove_files) in [(tr("Keep files"), false), (tr("Delete files"), true)]
        {
            let button = windowdialog::button(theme, &label);
            button.connect_clicked({
                let dialog = Rc::downgrade(&dialog);
                let others = Rc::downgrade(self);
                let (uid, name) = (user.uid, user.display_name().to_owned());
                move |_| {
                    let (Some(dialog), Some(others)) = (dialog.upgrade(), others.upgrade()) else {
                        return;
                    };
                    dialog.dismiss();
                    others.run(trf("%1 was deleted", &[&name]), async move {
                        accounts::delete(uid, should_remove_files).await
                    });
                }
            });
            buttons.append(&button);
        }
        dialog.column.add(&buttons, place);
        (self.present)(dialog);
    }
}

struct Adding {
    real_name_field: Rc<TextField>,
    user_name_field: Rc<TextField>,
    password_field: Rc<TextField>,
    repeat_field: Rc<TextField>,
    is_administrator: Cell<bool>,
    is_user_name_typed: Cell<bool>,
    is_filling: Cell<bool>,
    is_busy: Cell<bool>,
    add_button: gtk4::Widget,
    problem_label: gtk4::Label,
}

impl Adding {
    fn find_problem(&self) -> Option<String> {
        let user_name = self.user_name_field.text();
        let (password, repeated_password) = (self.password_field.text(), self.repeat_field.text());
        let (passwords_differ, is_repeat_empty) =
            (password != repeated_password, repeated_password.is_empty());
        pam::wipe(&mut password.into_bytes());
        pam::wipe(&mut repeated_password.into_bytes());
        if user_name.is_empty() {
            return Some(String::new());
        }
        if !accounts::is_valid_user_name(&user_name) {
            return Some(tr(
                "A user name is up to 32 lower-case letters, digits, - and _, and starts with a letter",
            ));
        }
        if passwords_differ && !is_repeat_empty {
            return Some(tr("The new passwords do not match"));
        }
        if passwords_differ {
            return Some(String::new());
        }
        None
    }

    fn update(&self) {
        let problem = self.find_problem();
        let message = problem.clone().unwrap_or_default();
        self.problem_label.set_text(&message);
        self.problem_label.set_visible(!message.is_empty());
        self.add_button
            .set_sensitive(problem.is_none() && !self.is_busy.get());
    }

    fn clear(&self) {
        self.is_filling.set(true);
        for field in [
            &self.real_name_field,
            &self.user_name_field,
            &self.password_field,
            &self.repeat_field,
        ] {
            field.set_text("");
        }
        self.is_filling.set(false);
        self.is_user_name_typed.set(false);
        self.update();
    }
}

pub fn build(page: &Rc<Page>, context: &Context) {
    let section = page.section("group", &tr("Other users"));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    section.append(&list);
    let status_label = text::styled("");
    status_label.set_xalign(0.0);
    status_label.set_wrap(true);
    status_label.set_margin_start(NOTE_START);
    status_label.set_visible(false);
    section.append(&status_label);
    let others = Rc::new(Others {
        page: Rc::downgrade(page),
        present: Rc::new(context.dialog_presenter()),
        list,
        status_label,
        row_items: RefCell::new(Vec::new()),
    });
    others.reload();

    let add_section = page.section("person_add", &tr("Add a user"));
    let append_field = |parent: &gtk4::Box, theme: &SharedTheme, label: &str, is_secret: bool| {
        let field = if is_secret {
            TextField::secret(theme, Style::Outlined, label)
        } else {
            TextField::new(theme, Style::Outlined, label)
        };
        field.root.set_hexpand(true);
        parent.append(&field.root);
        field
    };
    let real_name_field = append_field(&add_section, &page.theme, &tr("Full name"), false);
    let user_name_field = append_field(&add_section, &page.theme, &tr("User name"), false);
    let kind_section = page.subsection(&add_section, &tr("Account type"), "");
    let passwords_section = page.subsection(
        &add_section,
        &tr("Password"),
        &tr("Leave both empty to have one chosen at the first login"),
    );
    let password_field = append_field(&passwords_section, &page.theme, &tr("Password"), true);
    let repeat_field = append_field(
        &passwords_section,
        &page.theme,
        &tr("Repeat password"),
        true,
    );
    let problem_label = text::styled("");
    text::set_color(&problem_label, "m3error");
    problem_label.set_xalign(0.0);
    problem_label.set_wrap(true);
    problem_label.set_margin_start(NOTE_START);
    let (add_button, add_label) = page.icon_button("person_add", true, &tr("Add user"), || {});
    add_button.set_margin_top(BUTTON_TOP);
    add_section.append(&add_button);
    add_section.append(&problem_label);
    let form = Rc::new(Adding {
        real_name_field,
        user_name_field,
        password_field,
        repeat_field,
        is_administrator: Cell::new(false),
        is_user_name_typed: Cell::new(false),
        is_filling: Cell::new(false),
        is_busy: Cell::new(false),
        add_button: add_button.clone().upcast(),
        problem_label,
    });

    let selection_slot: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
    let kind_selection = Selection::new(
        &page.theme,
        vec![
            Choice {
                label: tr("Standard"),
                icon: "",
                value: Value::from(false),
            },
            Choice {
                label: tr("Administrator"),
                icon: "",
                value: Value::from(true),
            },
        ],
        {
            let form = Rc::downgrade(&form);
            let selection_slot = selection_slot.clone();
            move |value| {
                if let Some(form) = form.upgrade() {
                    form.is_administrator.set(value.as_bool().unwrap_or(false));
                }
                if let Some(selection) = selection_slot.borrow().as_ref().and_then(Weak::upgrade) {
                    selection.set_current(&value);
                }
            }
        },
    );
    kind_selection.set_current(&Value::from(false));
    kind_section.append(&kind_selection.root);
    selection_slot.replace(Some(Rc::downgrade(&kind_selection)));

    form.real_name_field.connect_changed({
        let form = Rc::downgrade(&form);
        move || {
            let Some(form) = form.upgrade() else {
                return;
            };
            if !form.is_user_name_typed.get() && !form.is_filling.get() {
                form.is_filling.set(true);
                form.user_name_field
                    .set_text(&accounts::user_name_from(&form.real_name_field.text()));
                form.is_filling.set(false);
            }
            form.update();
        }
    });
    form.user_name_field.connect_changed({
        let form = Rc::downgrade(&form);
        move || {
            if let Some(form) = form.upgrade() {
                if !form.is_filling.get() {
                    form.is_user_name_typed.set(true);
                }
                form.update();
            }
        }
    });
    for field in [&form.password_field, &form.repeat_field] {
        let form = Rc::downgrade(&form);
        field.connect_changed(move || {
            if let Some(form) = form.upgrade() {
                form.update();
            }
        });
    }
    add_button.connect_clicked({
        let form = Rc::downgrade(&form);
        let others = Rc::downgrade(&others);
        move |_| {
            let (Some(form), Some(others)) = (form.upgrade(), others.upgrade()) else {
                return;
            };
            if form.find_problem().is_some() || form.is_busy.replace(true) {
                return;
            }
            add_label.set_text(&tr("Adding…"));
            form.update();
            let user_name = form.user_name_field.text();
            let real_name = form.real_name_field.text().trim().to_owned();
            let password = form.password_field.text();
            let is_administrator = form.is_administrator.get();
            let done_message = trf("%1 was added", &[&user_name]);
            let form = Rc::downgrade(&form);
            let label = add_label.clone();
            others.run(done_message, async move {
                let path = accounts::create(&user_name, &real_name, is_administrator).await;
                let result = match path {
                    Ok(path) if password.is_empty() => accounts::ask_password_at_login(&path).await,
                    Ok(path) => accounts::set_password(&path, &password).await,
                    Err(message) => Err(message),
                };
                pam::wipe(&mut password.into_bytes());
                if let Some(form) = form.upgrade() {
                    form.is_busy.set(false);
                    label.set_text(&tr("Add user"));
                    if result.is_ok() {
                        form.clear();
                    } else {
                        form.update();
                    }
                }
                result
            });
        }
    });
    form.update();
    page.keep(kind_selection);
    page.keep(selection_slot);
    page.keep(form);
    page.keep(others);
}
