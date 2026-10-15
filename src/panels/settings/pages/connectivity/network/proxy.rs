use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, Parent, Style};
use crate::platform::network::nmprofile;
use crate::platform::network::proxy::{self, Endpoint, Mode, Proxy};
use crate::platform::security::pam;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::spinbox::SpinBox;
use crate::ui::widgets::input::textfield::TextField;

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
    saved_proxy: RefCell<Proxy>,
    write_timer: Cell<Option<glib::SourceId>>,
}

impl Writer {
    fn change(self: &Rc<Self>, edit: impl FnOnce(&mut Proxy)) {
        edit(&mut self.proxy.borrow_mut());
        if let Some(timer) = self.write_timer.take() {
            timer.remove();
        }
        let writer = Rc::downgrade(self);
        self.write_timer
            .set(Some(glib::timeout_add_local_once(SETTLE, move || {
                if let Some(writer) = writer.upgrade() {
                    writer.write_timer.set(None);
                    writer.flush();
                }
            })));
    }

    fn flush(&self) {
        let proxy = self.proxy.borrow().clone();
        proxy::write(&proxy, &self.saved_proxy.borrow());
        self.saved_proxy.replace(proxy);
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        if let Some(timer) = self.write_timer.take() {
            timer.remove();
            self.flush();
        }
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let section = page.section("", "");
    if !proxy::is_available() {
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
            let initial_proxy = proxy::with_credentials(proxy::read()).await;
            if let Some(page) = page.upgrade() {
                fill(&page, &section, initial_proxy);
            }
        }
    });
    page
}

fn fill(page: &Rc<Page>, section: &gtk4::Box, initial_proxy: Proxy) {
    let writer = Rc::new(Writer {
        proxy: RefCell::new(initial_proxy.clone()),
        saved_proxy: RefCell::new(initial_proxy.clone()),
        write_timer: Cell::new(None),
    });

    let automatic_section = page.section("", "");
    let script_section = page.subsection(
        &automatic_section,
        &tr("Configuration script"),
        &tr("Leave empty to find the proxy on the network (WPAD)"),
    );
    text_field(
        page,
        &writer,
        &script_section,
        &tr("Script address"),
        initial_proxy.script.clone(),
        false,
        |proxy, text| {
            proxy.script = text.trim().to_owned();
        },
    );

    let manual_section = page.section("", "");
    endpoint_row(
        page,
        &writer,
        &manual_section,
        &tr("HTTP proxy"),
        &initial_proxy.http,
        |proxy| &mut proxy.http,
    );
    let https_row_slot = Rc::new(RefCell::new(None::<gtk4::Widget>));
    let same_proxy_switch = page.switch(
        &manual_section,
        "lock",
        &tr("Use the HTTP proxy for HTTPS too"),
        {
            let writer = Rc::downgrade(&writer);
            let https_row_slot = https_row_slot.clone();
            move |is_on| {
                if let Some(writer) = writer.upgrade() {
                    writer.change(|proxy| proxy.uses_same_proxy = is_on);
                }
                if let Some(https_row) = https_row_slot.borrow().as_ref() {
                    https_row.set_visible(!is_on);
                }
            }
        },
    );
    same_proxy_switch.bind({
        let writer = Rc::downgrade(&writer);
        move || {
            writer
                .upgrade()
                .is_some_and(|writer| writer.proxy.borrow().uses_same_proxy)
        }
    });
    let https_row = endpoint_row(
        page,
        &writer,
        &manual_section,
        &tr("HTTPS proxy"),
        &initial_proxy.https,
        |proxy| &mut proxy.https,
    );
    https_row.set_visible(!initial_proxy.uses_same_proxy);
    https_row_slot.replace(Some(https_row));
    endpoint_row(
        page,
        &writer,
        &manual_section,
        &tr("SOCKS proxy"),
        &initial_proxy.socks,
        |proxy| &mut proxy.socks,
    );
    let ignore_section = page.subsection(
        &manual_section,
        &tr("Not for these hosts"),
        &tr("Separated by commas: host names, domains such as .example.org, and networks such as 10.0.0.0/8"),
    );
    text_field(
        page,
        &writer,
        &ignore_section,
        &tr("Hosts"),
        initial_proxy.ignore_hosts.join(", "),
        false,
        |proxy, text| {
            proxy.ignore_hosts = nmprofile::items(text);
        },
    );

    let show = {
        let automatic_card = automatic_section.parent();
        let manual_card = manual_section.parent();
        move |mode: Mode| {
            if let Some(automatic_card) = &automatic_card {
                automatic_card.set_visible(mode == Mode::Automatic);
            }
            if let Some(manual_card) = &manual_card {
                manual_card.set_visible(mode == Mode::Manual);
            }
        }
    };
    show(initial_proxy.mode);
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
    let selection_slot: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
    let mode_selection = Selection::new(&page.theme, choices, {
        let writer = Rc::downgrade(&writer);
        let selection_slot = selection_slot.clone();
        move |value| {
            let index = value.as_u64().unwrap_or(0) as usize;
            let mode = MODES[index].1;
            if let Some(writer) = writer.upgrade() {
                writer.change(|proxy| proxy.mode = mode);
            }
            if let Some(selection) = selection_slot.borrow().as_ref().and_then(Weak::upgrade) {
                selection.set_current(&value);
            }
            show(mode);
        }
    });
    let index = MODES
        .iter()
        .position(|(_, mode)| *mode == initial_proxy.mode)
        .unwrap_or(0);
    mode_selection.set_current(&Value::from(index));
    mode_group.append(&mode_selection.root);
    selection_slot.replace(Some(Rc::downgrade(&mode_selection)));
    page.keep(mode_selection);
    page.keep(writer);
}

#[allow(clippy::too_many_arguments)]
fn text_field(
    page: &Page,
    writer: &Rc<Writer>,
    parent: &impl Parent,
    label: &str,
    value: String,
    is_secret: bool,
    apply: impl Fn(&mut Proxy, &str) + 'static,
) {
    let field = if is_secret {
        TextField::secret(&page.theme, Style::Outlined, label)
    } else {
        TextField::new(&page.theme, Style::Outlined, label)
    };
    field.root.set_hexpand(true);
    field.set_text(&value);
    if is_secret {
        pam::wipe(&mut value.into_bytes());
    }
    field.connect_finished({
        let writer = Rc::downgrade(writer);
        move |text| {
            if let Some(writer) = writer.upgrade() {
                writer.change(|proxy| apply(proxy, &text));
            }
            if is_secret {
                pam::wipe(&mut text.into_bytes());
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
    endpoint: &Endpoint,
    pick: fn(&mut Proxy) -> &mut Endpoint,
) -> gtk4::Widget {
    let group = page.subsection(parent, title, "");
    let row = page.row(&group);
    text_field(
        page,
        writer,
        &row,
        &tr("Host"),
        endpoint.host.clone(),
        false,
        move |proxy, text| {
            pick(proxy).host = text.trim().to_owned();
        },
    );
    let port_spin = SpinBox::new(&page.theme, 0, PORT_MAX, 1, 0);
    port_spin.set_value(i64::from(endpoint.port));
    port_spin.connect_changed({
        let writer = Rc::downgrade(writer);
        move |value| {
            if let Some(writer) = writer.upgrade() {
                writer.change(|proxy| pick(proxy).port = u16::try_from(value).unwrap_or(0));
            }
        }
    });
    row.append(&port_spin.root);
    page.keep(port_spin);
    let login_row = page.uniform_row(&group);
    text_field(
        page,
        writer,
        &login_row,
        &tr("Username"),
        endpoint.user.clone(),
        false,
        move |proxy, text| {
            pick(proxy).user = text.trim().to_owned();
        },
    );
    text_field(
        page,
        writer,
        &login_row,
        &tr("Password"),
        endpoint.password.clone(),
        true,
        move |proxy, text| {
            pick(proxy).password = text.to_owned();
        },
    );
    Page::subsection_root(&group)
}
