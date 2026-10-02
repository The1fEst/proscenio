use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, Parent, Style};
use crate::platform::nmprofile;
use crate::platform::proxy::{self, Endpoint, Mode, Proxy};
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::spinbox::SpinBox;
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;

const LABEL_START: i32 = 2;
const PORT_MAX: i64 = 65535;
const SETTLE: Duration = Duration::from_millis(800);
const MODES: [(&str, Mode); 3] = [
    ("Off", Mode::Off),
    ("Automatic", Mode::Automatic),
    ("Manual", Mode::Manual),
];

pub fn mode_name(mode: Mode) -> String {
    MODES
        .iter()
        .find(|(_, known)| *known == mode)
        .map(|(name, _)| tr(name))
        .unwrap_or_default()
}

struct Writer {
    proxy: RefCell<Proxy>,
    written: RefCell<Proxy>,
    queued: Cell<Option<glib::SourceId>>,
}

impl Writer {
    fn change(self: &Rc<Self>, edit: impl FnOnce(&mut Proxy)) {
        edit(&mut self.proxy.borrow_mut());
        if let Some(queued) = self.queued.take() {
            queued.remove();
        }
        let writer = Rc::downgrade(self);
        self.queued
            .set(Some(glib::timeout_add_local_once(SETTLE, move || {
                if let Some(writer) = writer.upgrade() {
                    writer.queued.set(None);
                    writer.flush();
                }
            })));
    }

    fn flush(&self) {
        let proxy = self.proxy.borrow().clone();
        proxy::write(&proxy, &self.written.borrow());
        self.written.replace(proxy);
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if let Some(queued) = self.queued.take() {
            queued.remove();
            self.flush();
        }
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let section = page.section("", "");
    if !proxy::available() {
        page.notice(
            &section,
            "info",
            &tr("The system proxy settings schema is missing. Install gsettings-desktop-schemas"),
        );
        return page;
    }
    let explanation = text::styled(&tr(
        "GTK and KDE apps follow it, and so do programs started after the change that read the usual proxy variables",
    ));
    text::set_color(&explanation, "colSubtext");
    explanation.set_xalign(0.0);
    explanation.set_wrap(true);
    explanation.set_margin_start(LABEL_START);
    section.append(&explanation);
    glib::spawn_future_local({
        let page = Rc::downgrade(&page);
        async move {
            let current = proxy::with_credentials(proxy::read()).await;
            if let Some(page) = page.upgrade() {
                fill(&page, &section, current);
            }
        }
    });
    page
}

fn fill(page: &Rc<Page>, section: &gtk4::Box, current: Proxy) {
    let writer = Rc::new(Writer {
        proxy: RefCell::new(current.clone()),
        written: RefCell::new(current.clone()),
        queued: Cell::new(None),
    });

    let automatic = page.section("", "");
    let script = page.subsection(
        &automatic,
        &tr("Configuration script"),
        &tr("Leave empty to find the proxy on the network (WPAD)"),
    );
    text_field(
        page,
        &writer,
        &script,
        &tr("Script address"),
        current.script.clone(),
        false,
        |proxy, text| {
            proxy.script = text.trim().to_owned();
        },
    );

    let manual = page.section("", "");
    endpoint_row(
        page,
        &writer,
        &manual,
        &tr("HTTP proxy"),
        &current.http,
        |proxy| &mut proxy.http,
    );
    let https = Rc::new(RefCell::new(None::<gtk4::Widget>));
    let same = page.switch(&manual, "lock", &tr("Use the HTTP proxy for HTTPS too"), {
        let writer = Rc::downgrade(&writer);
        let https = https.clone();
        move |on| {
            if let Some(writer) = writer.upgrade() {
                writer.change(|proxy| proxy.same = on);
            }
            if let Some(https) = https.borrow().as_ref() {
                https.set_visible(!on);
            }
        }
    });
    same.bind({
        let writer = Rc::downgrade(&writer);
        move || {
            writer
                .upgrade()
                .is_some_and(|writer| writer.proxy.borrow().same)
        }
    });
    let https_row = endpoint_row(
        page,
        &writer,
        &manual,
        &tr("HTTPS proxy"),
        &current.https,
        |proxy| &mut proxy.https,
    );
    https_row.set_visible(!current.same);
    https.replace(Some(https_row));
    endpoint_row(
        page,
        &writer,
        &manual,
        &tr("SOCKS proxy"),
        &current.socks,
        |proxy| &mut proxy.socks,
    );
    let ignore = page.subsection(
        &manual,
        &tr("Not for these hosts"),
        &tr("Separated by commas: host names, domains such as .example.org, and networks such as 10.0.0.0/8"),
    );
    text_field(
        page,
        &writer,
        &ignore,
        &tr("Hosts"),
        current.ignore.join(", "),
        false,
        |proxy, text| {
            proxy.ignore = nmprofile::items(text);
        },
    );

    let show = {
        let automatic = automatic.parent();
        let manual = manual.parent();
        move |mode: Mode| {
            if let Some(automatic) = &automatic {
                automatic.set_visible(mode == Mode::Automatic);
            }
            if let Some(manual) = &manual {
                manual.set_visible(mode == Mode::Manual);
            }
        }
    };
    show(current.mode);
    let mode_group = page.subsection(section, &tr("Proxy"), "");
    let choices = MODES
        .iter()
        .enumerate()
        .map(|(index, (name, _))| Choice {
            label: tr(name),
            icon: "",
            value: Value::from(index),
        })
        .collect();
    let selection: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
    let created = Selection::new(&page.theme, choices, {
        let writer = Rc::downgrade(&writer);
        let selection = selection.clone();
        move |value| {
            let index = value.as_u64().unwrap_or(0) as usize;
            let mode = MODES[index].1;
            if let Some(writer) = writer.upgrade() {
                writer.change(|proxy| proxy.mode = mode);
            }
            if let Some(selection) = selection.borrow().as_ref().and_then(Weak::upgrade) {
                selection.set_current(&value);
            }
            show(mode);
        }
    });
    let index = MODES
        .iter()
        .position(|(_, mode)| *mode == current.mode)
        .unwrap_or(0);
    created.set_current(&Value::from(index));
    mode_group.append(&created.root);
    selection.replace(Some(Rc::downgrade(&created)));
    page.keep(created);
    page.keep(writer);
}

#[allow(clippy::too_many_arguments)]
fn text_field(
    page: &Page,
    writer: &Rc<Writer>,
    parent: &impl Parent,
    label: &str,
    value: String,
    secret: bool,
    apply: impl Fn(&mut Proxy, &str) + 'static,
) {
    let field = if secret {
        TextField::secret(&page.theme, Style::Outlined, label)
    } else {
        TextField::new(&page.theme, Style::Outlined, label)
    };
    field.root.set_hexpand(true);
    field.set_text(&value);
    field.connect_finished({
        let writer = Rc::downgrade(writer);
        move |text| {
            if let Some(writer) = writer.upgrade() {
                writer.change(|proxy| apply(proxy, &text));
            }
        }
    });
    parent.add(&field.root);
    page.keep(field);
}

fn endpoint_row(
    page: &Page,
    writer: &Rc<Writer>,
    parent: &gtk4::Box,
    title: &str,
    current: &Endpoint,
    pick: fn(&mut Proxy) -> &mut Endpoint,
) -> gtk4::Widget {
    let group = page.subsection(parent, title, "");
    let row = page.row(&group);
    text_field(
        page,
        writer,
        &row,
        &tr("Host"),
        current.host.clone(),
        false,
        move |proxy, text| {
            pick(proxy).host = text.trim().to_owned();
        },
    );
    let port = SpinBox::new(&page.theme, 0, PORT_MAX, 1, 0);
    port.set_value(i64::from(current.port));
    port.connect_changed({
        let writer = Rc::downgrade(writer);
        move |value| {
            if let Some(writer) = writer.upgrade() {
                writer.change(|proxy| pick(proxy).port = u16::try_from(value).unwrap_or(0));
            }
        }
    });
    row.append(&port.root);
    page.keep(port);
    let login = page.uniform_row(&group);
    text_field(
        page,
        writer,
        &login,
        &tr("Username"),
        current.user.clone(),
        false,
        move |proxy, text| {
            pick(proxy).user = text.trim().to_owned();
        },
    );
    text_field(
        page,
        writer,
        &login,
        &tr("Password"),
        current.password.clone(),
        true,
        move |proxy, text| {
            pick(proxy).password = text.to_owned();
        },
    );
    Page::subsection_root(&group)
}
