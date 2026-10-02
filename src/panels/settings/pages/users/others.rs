use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::services::accounts::{self, User};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::{ConfigSwitch, icon_button};
use crate::ui::widgets::row::Row;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;
use crate::ui::widgets::windowdialog::{self, Place, WindowDialog};

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
    status: gtk4::Label,
    rows: RefCell<Vec<Box<dyn Any>>>,
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

    fn report(&self, result: Result<(), String>, done: &str) {
        let (message, color) = match result {
            Ok(()) => (done.to_owned(), "colSubtext"),
            Err(message) => (message, "colError"),
        };
        self.status.set_text(&message);
        self.status.set_visible(!message.is_empty());
        text::set_color(&self.status, color);
    }

    fn run(
        self: &Rc<Self>,
        done: String,
        call: impl Future<Output = Result<(), String>> + 'static,
    ) {
        let others = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = call.await;
            if let Some(others) = others.upgrade() {
                others.report(result, &done);
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
        let mut rows: Vec<Box<dyn Any>> = Vec::new();
        if users.is_empty() {
            let empty = text::styled(&tr("Nobody else has an account here"));
            text::set_color(&empty, "colSubtext");
            empty.set_xalign(0.0);
            empty.set_margin_start(NOTE_START);
            self.list.append(&empty);
        }
        for user in users {
            let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
            card.add_css_class("settings-row-card");
            card.set_size_request(-1, CARD_HEIGHT);
            let inside = Row::new(CARD_SPACING);
            inside.set_margin_start(CARD_START);
            inside.set_margin_end(CARD_END);
            inside.set_hexpand(true);
            let symbol = text::symbol("person", pixel_size::LARGER as f64);
            text::set_color(&symbol, "colOnLayer2");
            inside.append(&Centred::integral(&symbol));
            let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
            lines.set_hexpand(true);
            lines.set_valign(gtk4::Align::Center);
            let name = text::styled(user.display_name());
            text::set_color(&name, "colOnLayer2");
            name.set_xalign(0.0);
            name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centred::filling_width(&name));
            let detail = text::styled_sized(
                &if user.password_set {
                    user.user_name.clone()
                } else {
                    trf("%1 · password asked at first login", &[&user.user_name])
                },
                pixel_size::SMALLER,
            );
            text::set_color(&detail, "colSubtext");
            detail.set_xalign(0.0);
            detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
            lines.append(&Centred::filling_width(&detail));
            inside.append(&lines);

            let administrator = ConfigSwitch::new(&page.theme, "", &tr("Administrator"), {
                let others = Rc::downgrade(self);
                let path = user.path.clone();
                move |on| {
                    if let Some(others) = others.upgrade() {
                        let path = path.clone();
                        others.run(String::new(), async move {
                            accounts::set_administrator(&path, on).await
                        });
                    }
                }
            });
            administrator.set(user.administrator);
            inside.append(&administrator.button);
            rows.push(Box::new(administrator));

            let (password, _) = icon_button(&page.theme, "password", false, "");
            password.connect_clicked({
                let others = Rc::downgrade(self);
                let user = user.clone();
                move |_| {
                    if let Some(others) = others.upgrade() {
                        others.ask_password(&user);
                    }
                }
            });
            rows.push(Box::new(page.unkept_tip(&password, &tr("Set password…"))));
            inside.append(&password);

            let (delete, _) = icon_button(&page.theme, "person_remove", false, "");
            delete.connect_clicked({
                let others = Rc::downgrade(self);
                let user = user.clone();
                move |_| {
                    if let Some(others) = others.upgrade() {
                        others.confirm_delete(&user);
                    }
                }
            });
            rows.push(Box::new(page.unkept_tip(&delete, &tr("Delete account…"))));
            inside.append(&delete);
            card.append(&inside);
            self.list.append(&card);
        }
        self.rows.replace(rows);
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
        let first = TextField::secret(theme, Style::Outlined, &tr("New password"));
        dialog.column.add(&first.root, Place::wide());
        let second = TextField::secret(theme, Style::Outlined, &tr("Repeat new password"));
        dialog.column.add(&second.root, Place::wide());
        let mismatch = text::styled(&tr("The new passwords do not match"));
        text::set_color(&mismatch, "m3error");
        mismatch.set_xalign(0.0);
        mismatch.set_visible(false);
        dialog.column.add(&mismatch, Place::wide());
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
        let set = windowdialog::button(theme, &tr("Set password"));
        set.set_sensitive(false);
        set.connect_clicked({
            let dialog = Rc::downgrade(&dialog);
            let others = Rc::downgrade(self);
            let first = Rc::downgrade(&first);
            let path = user.path.clone();
            move |_| {
                let (Some(dialog), Some(others), Some(first)) =
                    (dialog.upgrade(), others.upgrade(), first.upgrade())
                else {
                    return;
                };
                let password = first.text();
                let path = path.clone();
                dialog.dismiss();
                others.run(tr("Password set"), async move {
                    accounts::set_password(&path, &password).await
                });
            }
        });
        buttons.append(&set);
        dialog.column.add(&buttons, place);
        let check = {
            let (first, second) = (Rc::downgrade(&first), Rc::downgrade(&second));
            let set = set.clone();
            move || {
                let (Some(first), Some(second)) = (first.upgrade(), second.upgrade()) else {
                    return;
                };
                let (one, two) = (first.text(), second.text());
                mismatch.set_visible(!two.is_empty() && one != two);
                set.set_sensitive(!one.is_empty() && one == two);
            }
        };
        let check = Rc::new(check);
        for field in [&first, &second] {
            let check = check.clone();
            field.connect_changed(move || check());
        }
        glib::idle_add_local_once({
            let first = Rc::downgrade(&first);
            move || {
                if let Some(first) = first.upgrade() {
                    first.grab_focus();
                }
            }
        });
        dialog.keep(first);
        dialog.keep(second);
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
        buttons.append(&windowdialog::spacer());
        for (label, remove_files) in [(tr("Keep files"), false), (tr("Delete files"), true)] {
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
                        accounts::delete(uid, remove_files).await
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
    real_name: Rc<TextField>,
    user_name: Rc<TextField>,
    first: Rc<TextField>,
    second: Rc<TextField>,
    administrator: Cell<bool>,
    user_name_typed: Cell<bool>,
    filling: Cell<bool>,
    busy: Cell<bool>,
    add: gtk4::Widget,
    problem: gtk4::Label,
}

impl Adding {
    fn problem(&self) -> Option<String> {
        let user_name = self.user_name.text();
        let (one, two) = (self.first.text(), self.second.text());
        if user_name.is_empty() {
            return Some(String::new());
        }
        if !accounts::valid_user_name(&user_name) {
            return Some(tr(
                "A user name is up to 32 lower-case letters, digits, - and _, and starts with a letter",
            ));
        }
        if one != two && !two.is_empty() {
            return Some(tr("The new passwords do not match"));
        }
        if one != two {
            return Some(String::new());
        }
        None
    }

    fn update(&self) {
        let problem = self.problem();
        let message = problem.clone().unwrap_or_default();
        self.problem.set_text(&message);
        self.problem.set_visible(!message.is_empty());
        self.add
            .set_sensitive(problem.is_none() && !self.busy.get());
    }

    fn clear(&self) {
        self.filling.set(true);
        for field in [&self.real_name, &self.user_name, &self.first, &self.second] {
            field.set_text("");
        }
        self.filling.set(false);
        self.user_name_typed.set(false);
        self.update();
    }
}

pub fn build(page: &Rc<Page>, context: &Context) {
    let section = page.section("group", &tr("Other users"));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    section.append(&list);
    let status = text::styled("");
    status.set_xalign(0.0);
    status.set_wrap(true);
    status.set_margin_start(NOTE_START);
    status.set_visible(false);
    section.append(&status);
    let others = Rc::new(Others {
        page: Rc::downgrade(page),
        present: Rc::new(context.dialog_presenter()),
        list,
        status,
        rows: RefCell::new(Vec::new()),
    });
    others.reload();

    let adding = page.section("person_add", &tr("Add a user"));
    let field = |parent: &gtk4::Box, theme: &SharedTheme, label: &str, secret: bool| {
        let field = if secret {
            TextField::secret(theme, Style::Outlined, label)
        } else {
            TextField::new(theme, Style::Outlined, label)
        };
        field.root.set_hexpand(true);
        parent.append(&field.root);
        field
    };
    let real_name = field(&adding, &page.theme, &tr("Full name"), false);
    let user_name = field(&adding, &page.theme, &tr("User name"), false);
    let kind = page.subsection(&adding, &tr("Account type"), "");
    let passwords = page.subsection(
        &adding,
        &tr("Password"),
        &tr("Leave both empty to have one chosen at the first login"),
    );
    let first = field(&passwords, &page.theme, &tr("Password"), true);
    let second = field(&passwords, &page.theme, &tr("Repeat password"), true);
    let problem = text::styled("");
    text::set_color(&problem, "m3error");
    problem.set_xalign(0.0);
    problem.set_wrap(true);
    problem.set_margin_start(NOTE_START);
    let (add, add_label) = page.icon_button("person_add", true, &tr("Add user"), || {});
    add.set_margin_top(BUTTON_TOP);
    adding.append(&add);
    adding.append(&problem);
    let form = Rc::new(Adding {
        real_name,
        user_name,
        first,
        second,
        administrator: Cell::new(false),
        user_name_typed: Cell::new(false),
        filling: Cell::new(false),
        busy: Cell::new(false),
        add: add.clone().upcast(),
        problem,
    });

    let selection: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
    let created = Selection::new(
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
            let selection = selection.clone();
            move |value| {
                if let Some(form) = form.upgrade() {
                    form.administrator.set(value.as_bool().unwrap_or(false));
                }
                if let Some(selection) = selection.borrow().as_ref().and_then(Weak::upgrade) {
                    selection.set_current(&value);
                }
            }
        },
    );
    created.set_current(&Value::from(false));
    kind.append(&created.root);
    selection.replace(Some(Rc::downgrade(&created)));

    form.real_name.connect_changed({
        let form = Rc::downgrade(&form);
        move || {
            let Some(form) = form.upgrade() else {
                return;
            };
            if !form.user_name_typed.get() && !form.filling.get() {
                form.filling.set(true);
                form.user_name
                    .set_text(&accounts::user_name_from(&form.real_name.text()));
                form.filling.set(false);
            }
            form.update();
        }
    });
    form.user_name.connect_changed({
        let form = Rc::downgrade(&form);
        move || {
            if let Some(form) = form.upgrade() {
                if !form.filling.get() {
                    form.user_name_typed.set(true);
                }
                form.update();
            }
        }
    });
    for field in [&form.first, &form.second] {
        let form = Rc::downgrade(&form);
        field.connect_changed(move || {
            if let Some(form) = form.upgrade() {
                form.update();
            }
        });
    }
    add.connect_clicked({
        let form = Rc::downgrade(&form);
        let others = Rc::downgrade(&others);
        move |_| {
            let (Some(form), Some(others)) = (form.upgrade(), others.upgrade()) else {
                return;
            };
            if form.problem().is_some() || form.busy.replace(true) {
                return;
            }
            add_label.set_text(&tr("Adding…"));
            form.update();
            let user_name = form.user_name.text();
            let real_name = form.real_name.text().trim().to_owned();
            let password = form.first.text();
            let administrator = form.administrator.get();
            let done = trf("%1 was added", &[&user_name]);
            let form = Rc::downgrade(&form);
            let label = add_label.clone();
            others.run(done, async move {
                let path = accounts::create(&user_name, &real_name, administrator).await;
                let result = match path {
                    Ok(path) if password.is_empty() => accounts::ask_password_at_login(&path).await,
                    Ok(path) => accounts::set_password(&path, &password).await,
                    Err(message) => Err(message),
                };
                if let Some(form) = form.upgrade() {
                    form.busy.set(false);
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
    page.keep(created);
    page.keep(selection);
    page.keep(form);
    page.keep(others);
}
