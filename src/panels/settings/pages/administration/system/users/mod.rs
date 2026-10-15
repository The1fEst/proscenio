use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::{tr, trf};
use crate::core::tools;
use crate::panels::settings::content::{Context, Page, Style};
use crate::platform::security::pam;
use crate::services::session::accounts::{self, User};
use crate::ui::image;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::layout::centered::Centered;

const ACCOUNTS: &str = "org.freedesktop.Accounts";
const NO_ACCOUNTS: &str = "AccountsService is not on the system bus, so the name, email and picture can be neither read nor changed. It comes with the accountsservice package.";
const HEADER_SPACING: i32 = 20;
const HEADER_MARGIN: i32 = 10;
const AVATAR: i32 = 80;
const AVATAR_SYMBOL: f64 = 40.0;
const AVATAR_SOURCE: i32 = 160;
const NAME_SPACING: i32 = 5;
const FORM_SPACING: i32 = 8;
const AUTHENTICATION_FAILURE: &str = "Authentication failure";

mod others;

struct Form {
    root: gtk4::Box,
    opener: gtk4::Widget,
    current_field: Rc<TextField>,
    new_field: Rc<TextField>,
    repeat_field: Rc<TextField>,
    error_label: gtk4::Label,
    cancel_button: gtk4::Widget,
    confirm_button: gtk4::Widget,
    confirm_label: gtk4::Label,
    is_changing: Cell<bool>,
    complaint: RefCell<String>,
}

type Show = Rc<RefCell<Option<Rc<dyn Fn(User)>>>>;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let user = Rc::new(RefCell::new(User::default()));
    let show_slot: Show = Rc::new(RefCell::new(None));

    let account_section = page.section("person", &tr("Account"));
    if !tools::has_system_service(ACCOUNTS) {
        page.notice(&account_section, "info", &tr(NO_ACCOUNTS));
    }
    let header = gtk4::Box::new(gtk4::Orientation::Horizontal, HEADER_SPACING);
    header.set_halign(gtk4::Align::Center);
    header.set_margin_top(HEADER_MARGIN);
    header.set_margin_bottom(HEADER_MARGIN);
    let avatar = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    avatar.add_css_class("settings-avatar");
    avatar.set_overflow(gtk4::Overflow::Hidden);
    avatar.set_size_request(AVATAR, AVATAR);
    header.append(&avatar);
    let names = gtk4::Box::new(gtk4::Orientation::Vertical, NAME_SPACING);
    names.set_valign(gtk4::Align::Center);
    let name_label = text::styled_sized("", pixel_size::TITLE);
    name_label.set_xalign(0.0);
    names.append(&Centered::filling_width(&name_label));
    let role_label = text::styled("");
    text::set_color(&role_label, "colSubtext");
    role_label.set_xalign(0.0);
    names.append(&Centered::filling_width(&role_label));
    header.append(&names);
    account_section.append(&header);
    let pictures = gtk4::Box::new(gtk4::Orientation::Horizontal, FORM_SPACING);
    pictures.set_halign(gtk4::Align::Center);
    let picture_problem_label = text::styled("");
    text::set_color(&picture_problem_label, "m3error");
    picture_problem_label.set_wrap(true);
    picture_problem_label.set_visible(false);
    let (choose_button, _) =
        page.icon_button("add_photo_alternate", true, &tr("Choose picture…"), {
            let (user, show_slot) = (user.clone(), show_slot.clone());
            let problem_label = picture_problem_label.downgrade();
            move || choose_picture(&user, &show_slot, problem_label.clone())
        });
    pictures.append(&choose_button);
    let missing_tools = tools::find_missing(&[&tools::KDIALOG]);
    if !missing_tools.is_empty() {
        choose_button.set_sensitive(false);
        page.tip(
            &choose_button,
            &tools::missing_message(&missing_tools, &tr("there is no file picker")),
        );
    }
    let (remove_picture_button, _) =
        page.icon_button("hide_image", false, &tr("Remove picture"), {
            let (user, show_slot) = (user.clone(), show_slot.clone());
            let problem_label = picture_problem_label.downgrade();
            move || change_picture(&user, &show_slot, Ok(String::new()), problem_label.clone())
        });
    pictures.append(&remove_picture_button);
    account_section.append(&pictures);
    account_section.append(&picture_problem_label);

    let real_name_section = page.subsection(&account_section, &tr("Name"), "");
    let real_name_field = page.text_field(
        &real_name_section,
        Style::Outlined,
        "",
        {
            let user = user.clone();
            move || user.borrow().real_name.clone()
        },
        {
            let (user, show_slot) = (user.clone(), show_slot.clone());
            move |value| write(&user, &show_slot, "RealName", &value)
        },
    );
    let email_section = page.subsection(&account_section, &tr("Email address"), "");
    let email_field = page.text_field(
        &email_section,
        Style::Outlined,
        &tr("name@example.com"),
        {
            let user = user.clone();
            move || user.borrow().email.clone()
        },
        {
            let (user, show_slot) = (user.clone(), show_slot.clone());
            move |value| write(&user, &show_slot, "Email", &value)
        },
    );

    let form_slot: Rc<RefCell<Weak<Form>>> = Rc::new(RefCell::new(Weak::new()));
    let (opener, _) = page.icon_button("password", true, &tr("Change password…"), {
        let form_slot = form_slot.clone();
        move || {
            if let Some(form) = form_slot.borrow().upgrade() {
                form.show(true);
            }
        }
    });
    account_section.append(&opener);
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, FORM_SPACING);
    let current_field = page.secret_field(&root, &tr("Current password"));
    let new_field = page.secret_field(&root, &tr("New password"));
    let repeat_field = page.secret_field(&root, &tr("Repeat new password"));
    let error_label = text::styled("");
    text::set_color(&error_label, "m3error");
    error_label.set_wrap(true);
    error_label.set_xalign(0.0);
    error_label.set_hexpand(true);
    root.append(&error_label);
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, FORM_SPACING);
    buttons.set_halign(gtk4::Align::End);
    let (cancel_button, _) = page.icon_button("close", true, &tr("Cancel"), {
        let form_slot = form_slot.clone();
        move || {
            if let Some(form) = form_slot.borrow().upgrade() {
                form.show(false);
            }
        }
    });
    buttons.append(&cancel_button);
    let (confirm_button, confirm_label) =
        page.icon_button("check", true, &tr("Change password"), {
            let form_slot = form_slot.clone();
            move || {
                let form = form_slot.borrow().upgrade();
                if let Some(form) = form {
                    form.submit();
                }
            }
        });
    buttons.append(&confirm_button);
    root.append(&buttons);
    account_section.append(&root);

    let password_form = Rc::new(Form {
        root,
        opener: opener.upcast(),
        current_field,
        new_field,
        repeat_field,
        error_label,
        cancel_button: cancel_button.upcast(),
        confirm_button: confirm_button.upcast(),
        confirm_label,
        is_changing: Cell::new(false),
        complaint: RefCell::new(String::new()),
    });
    for field in [
        &password_form.current_field,
        &password_form.new_field,
        &password_form.repeat_field,
    ] {
        let form = Rc::downgrade(&password_form);
        field.connect_changed(move || {
            if let Some(form) = form.upgrade() {
                form.update();
            }
        });
    }
    password_form.show(false);
    form_slot.replace(Rc::downgrade(&password_form));
    page.keep(password_form);

    let show: Rc<dyn Fn(User)> = {
        let (real_name_field, email_field) =
            (Rc::downgrade(&real_name_field), Rc::downgrade(&email_field));
        let avatar = avatar.clone();
        Rc::new(move |account: User| {
            name_label.set_text(account.display_name());
            role_label.set_text(&if account.is_administrator {
                trf("%1 · Administrator", &[&account.user_name])
            } else {
                trf("%1 · Standard", &[&account.user_name])
            });
            show_avatar(&avatar, &account.icon_file);
            let placeholder = account.user_name.clone();
            user.replace(account);
            if let Some(real_name_field) = real_name_field.upgrade() {
                real_name_field.set_placeholder(&placeholder);
                real_name_field.refresh();
            }
            if let Some(email_field) = email_field.upgrade() {
                email_field.refresh();
            }
        })
    };
    show_slot.replace(Some(show.clone()));
    show_avatar(&avatar, "");
    accounts::read(move |account| show(account));
    if tools::has_system_service(ACCOUNTS) {
        others::build(&page, context);
    }
    page
}

fn change_picture(
    user: &Rc<RefCell<User>>,
    show_slot: &Show,
    file: Result<String, String>,
    problem_label: glib::WeakRef<gtk4::Label>,
) {
    let path = user.borrow().path.clone();
    let show_slot = show_slot.clone();
    glib::spawn_future_local(async move {
        let result = match file {
            Ok(file) => accounts::set_icon(&path, &file).await,
            Err(message) => Err(message),
        };
        if let Some(problem_label) = problem_label.upgrade() {
            let message = result.err().unwrap_or_default();
            problem_label.set_visible(!message.is_empty());
            problem_label.set_text(&message);
        }
        accounts::read(move |account| {
            let show = show_slot.borrow().clone();
            if let Some(show) = show {
                show(account);
            }
        });
    });
}

fn choose_picture(
    user: &Rc<RefCell<User>>,
    show_slot: &Show,
    problem_label: glib::WeakRef<gtk4::Label>,
) {
    let (user, show_slot) = (user.clone(), show_slot.clone());
    glib::spawn_future_local(async move {
        let pictures =
            glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(glib::home_dir);
        let picker = crate::core::process::command(&[
            "kdialog",
            "--getopenfilename",
            &pictures.to_string_lossy(),
            &format!("*.png *.jpg *.jpeg *.webp|{}", tr("Pictures")),
            "--title",
            &tr("Choose picture"),
        ]);
        let file = crate::core::process::capture_text(picker)
            .await
            .map(|file| file.trim().to_owned())
            .filter(|file| !file.is_empty());
        if let Some(file) = file {
            let face = accounts::face_from(&file).await;
            change_picture(&user, &show_slot, face, problem_label);
        }
    });
}

fn write(user: &Rc<RefCell<User>>, show_slot: &Show, property: &str, value: &str) {
    let show_slot = show_slot.clone();
    let account = user.borrow().clone();
    accounts::set(&account, property, value, move |account| {
        let show = show_slot.borrow().clone();
        if let Some(show) = show {
            show(account);
        }
    });
}

fn show_avatar(avatar: &gtk4::Box, icon_file: &str) {
    while let Some(child) = avatar.first_child() {
        avatar.remove(&child);
    }
    if icon_file.is_empty() {
        let symbol = text::symbol("person", AVATAR_SYMBOL);
        text::set_color(&symbol, "colSubtext");
        let symbol_box = Centered::integral(&symbol);
        symbol_box.set_hexpand(true);
        avatar.append(&symbol_box);
        return;
    }
    let portrait = gtk4::Image::new();
    portrait.set_pixel_size(AVATAR);
    avatar.append(&portrait);
    let (portrait, path) = (portrait.downgrade(), icon_file.into());
    glib::spawn_future_local(async move {
        let texture = image::cover_texture(path, (AVATAR_SOURCE, AVATAR_SOURCE)).await;
        if let (Some(portrait), Some(texture)) = (portrait.upgrade(), texture) {
            portrait.set_paintable(Some(&texture));
        }
    });
}

impl Form {
    fn show(&self, is_open: bool) {
        self.root.set_visible(is_open);
        self.opener.set_visible(!is_open);
        if !is_open {
            for field in [&self.current_field, &self.new_field, &self.repeat_field] {
                field.set_text("");
            }
            self.complaint.replace(String::new());
        }
        self.update();
    }

    fn are_fields_filled(&self) -> bool {
        [&self.current_field, &self.new_field, &self.repeat_field]
            .iter()
            .all(|field| !field.is_empty())
    }

    fn passwords_differ(&self) -> bool {
        if self.repeat_field.is_empty() {
            return false;
        }
        let (new_password, repeated_password) = (self.new_field.text(), self.repeat_field.text());
        let is_mismatch = new_password != repeated_password;
        pam::wipe(&mut new_password.into_bytes());
        pam::wipe(&mut repeated_password.into_bytes());
        is_mismatch
    }

    fn update(&self) {
        let is_changing = self.is_changing.get();
        let is_mismatch = self.passwords_differ();
        let message = if is_mismatch {
            tr("The new passwords do not match")
        } else if *self.complaint.borrow() == AUTHENTICATION_FAILURE {
            tr("The current password is not correct")
        } else {
            self.complaint.borrow().clone()
        };
        self.error_label.set_visible(!message.is_empty());
        self.error_label.set_text(&message);
        self.cancel_button.set_sensitive(!is_changing);
        self.confirm_button
            .set_sensitive(self.are_fields_filled() && !is_mismatch && !is_changing);
        self.confirm_label.set_text(&if is_changing {
            tr("Changing…")
        } else {
            tr("Change password")
        });
    }

    fn submit(self: &Rc<Self>) {
        if self.is_changing.replace(true) {
            return;
        }
        self.complaint.replace(String::new());
        self.update();
        let form = Rc::downgrade(self);
        let (current_password, new_password) = (self.current_field.text(), self.new_field.text());
        accounts::change_password(&current_password, &new_password, move |outcome| {
            let Some(form) = form.upgrade() else {
                return;
            };
            form.is_changing.set(false);
            match outcome {
                Ok(()) => form.show(false),
                Err(complaint) => {
                    form.complaint.replace(complaint);
                    form.update();
                }
            }
        });
        pam::wipe(&mut current_password.into_bytes());
        pam::wipe(&mut new_password.into_bytes());
    }
}
