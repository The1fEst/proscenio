use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::tools;
use crate::panels::settings::content::{Context, Page, Style};
use crate::services::accounts::{self, User};
use crate::ui::image;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;

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

struct Form {
    root: gtk4::Box,
    opener: gtk4::Widget,
    current: Rc<TextField>,
    next: Rc<TextField>,
    repeat: Rc<TextField>,
    error: gtk4::Label,
    cancel: gtk4::Widget,
    confirm: gtk4::Widget,
    confirm_label: gtk4::Label,
    changing: Cell<bool>,
    complaint: RefCell<String>,
}

type Show = Rc<RefCell<Option<Rc<dyn Fn(User)>>>>;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let user = Rc::new(RefCell::new(User::default()));
    let shown: Show = Rc::new(RefCell::new(None));

    let account = page.section("person", "Account");
    if !tools::system_service(ACCOUNTS) {
        page.notice(&account, "info", NO_ACCOUNTS);
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
    let name = text::styled_sized("", pixel_size::TITLE);
    name.set_xalign(0.0);
    names.append(&Centred::filling_width(&name));
    let role = text::styled("");
    text::set_color(&role, "colSubtext");
    role.set_xalign(0.0);
    names.append(&Centred::filling_width(&role));
    header.append(&names);
    account.append(&header);

    let real_name = page.subsection(&account, "Name", "");
    let real_name = page.text_field(
        &real_name,
        Style::Outlined,
        "",
        {
            let user = user.clone();
            move || user.borrow().real_name.clone()
        },
        {
            let (user, shown) = (user.clone(), shown.clone());
            move |value| write(&user, &shown, "RealName", &value)
        },
    );
    let email = page.subsection(&account, "Email address", "");
    let email = page.text_field(
        &email,
        Style::Outlined,
        "name@example.com",
        {
            let user = user.clone();
            move || user.borrow().email.clone()
        },
        {
            let (user, shown) = (user.clone(), shown.clone());
            move |value| write(&user, &shown, "Email", &value)
        },
    );

    let form: Rc<RefCell<Weak<Form>>> = Rc::new(RefCell::new(Weak::new()));
    let (opener, _) = page.icon_button("password", true, "Change password…", {
        let form = form.clone();
        move || {
            if let Some(form) = form.borrow().upgrade() {
                form.show(true);
            }
        }
    });
    account.append(&opener);
    let root = gtk4::Box::new(gtk4::Orientation::Vertical, FORM_SPACING);
    let current = page.secret_field(&root, "Current password");
    let next = page.secret_field(&root, "New password");
    let repeat = page.secret_field(&root, "Repeat new password");
    let error = text::styled("");
    text::set_color(&error, "m3error");
    error.set_wrap(true);
    error.set_xalign(0.0);
    error.set_hexpand(true);
    root.append(&error);
    let buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, FORM_SPACING);
    buttons.set_halign(gtk4::Align::End);
    let (cancel, _) = page.icon_button("close", true, "Cancel", {
        let form = form.clone();
        move || {
            if let Some(form) = form.borrow().upgrade() {
                form.show(false);
            }
        }
    });
    buttons.append(&cancel);
    let (confirm, confirm_label) = page.icon_button("check", true, "Change password", {
        let form = form.clone();
        move || {
            let form = form.borrow().upgrade();
            if let Some(form) = form {
                form.submit();
            }
        }
    });
    buttons.append(&confirm);
    root.append(&buttons);
    account.append(&root);

    let built = Rc::new(Form {
        root,
        opener: opener.upcast(),
        current,
        next,
        repeat,
        error,
        cancel: cancel.upcast(),
        confirm: confirm.upcast(),
        confirm_label,
        changing: Cell::new(false),
        complaint: RefCell::new(String::new()),
    });
    for field in [&built.current, &built.next, &built.repeat] {
        let form = Rc::downgrade(&built);
        field.connect_changed(move || {
            if let Some(form) = form.upgrade() {
                form.update();
            }
        });
    }
    built.show(false);
    form.replace(Rc::downgrade(&built));
    page.keep(built);

    let show: Rc<dyn Fn(User)> = {
        let (real_name, email) = (Rc::downgrade(&real_name), Rc::downgrade(&email));
        let avatar = avatar.clone();
        Rc::new(move |account: User| {
            name.set_text(account.display_name());
            let kind = if account.administrator {
                "Administrator"
            } else {
                "Standard"
            };
            role.set_text(&format!("{} · {kind}", account.user_name));
            show_avatar(&avatar, &account.icon_file);
            let placeholder = account.user_name.clone();
            user.replace(account);
            if let Some(real_name) = real_name.upgrade() {
                real_name.set_placeholder(&placeholder);
                real_name.refresh();
            }
            if let Some(email) = email.upgrade() {
                email.refresh();
            }
        })
    };
    shown.replace(Some(show.clone()));
    show_avatar(&avatar, "");
    accounts::read(move |account| show(account));
    page
}

fn write(user: &Rc<RefCell<User>>, shown: &Show, property: &str, value: &str) {
    let shown = shown.clone();
    let account = user.borrow().clone();
    accounts::set(&account, property, value, move |account| {
        let show = shown.borrow().clone();
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
        let centred = Centred::integral(&symbol);
        centred.set_hexpand(true);
        avatar.append(&centred);
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
    fn show(&self, open: bool) {
        self.root.set_visible(open);
        self.opener.set_visible(!open);
        if !open {
            for field in [&self.current, &self.next, &self.repeat] {
                field.set_text("");
            }
            self.complaint.replace(String::new());
        }
        self.update();
    }

    fn filled(&self) -> bool {
        [&self.current, &self.next, &self.repeat]
            .iter()
            .all(|field| !field.text().is_empty())
    }

    fn mismatched(&self) -> bool {
        !self.repeat.text().is_empty() && self.next.text() != self.repeat.text()
    }

    fn update(&self) {
        let changing = self.changing.get();
        let message = if self.mismatched() {
            "The new passwords do not match".to_owned()
        } else if *self.complaint.borrow() == AUTHENTICATION_FAILURE {
            "The current password is not correct".to_owned()
        } else {
            self.complaint.borrow().clone()
        };
        self.error.set_visible(!message.is_empty());
        self.error.set_text(&message);
        self.cancel.set_sensitive(!changing);
        self.confirm
            .set_sensitive(self.filled() && !self.mismatched() && !changing);
        self.confirm_label.set_text(if changing {
            "Changing…"
        } else {
            "Change password"
        });
    }

    fn submit(self: &Rc<Self>) {
        if self.changing.replace(true) {
            return;
        }
        self.complaint.replace(String::new());
        self.update();
        let form = Rc::downgrade(self);
        accounts::change_password(&self.current.text(), &self.next.text(), move |outcome| {
            let Some(form) = form.upgrade() else {
                return;
            };
            form.changing.set(false);
            match outcome {
                Ok(()) => form.show(false),
                Err(complaint) => {
                    form.complaint.replace(complaint);
                    form.update();
                }
            }
        });
    }
}
