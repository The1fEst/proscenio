use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::panels::settings::pages::network::manager_running;
use crate::services::net::{Connections, nmcli};
use crate::ui::widgets::text;
use crate::ui::widgets::textfield::TextField;

const CONNECTION: &str = "Hotspot";
const LABEL_START: i32 = 2;
const STATUS_START: i32 = 8;
const PASSWORD_LENGTH: (usize, usize) = (8, 63);
const BANDS: [(&str, &str); 3] = [("Automatic", ""), ("2.4 GHz", "bg"), ("5 GHz", "a")];

struct Hotspot {
    connections: Rc<Connections>,
    active: Cell<bool>,
    busy: Cell<bool>,
    band: RefCell<String>,
    problem: RefCell<Option<String>>,
    changed: RefCell<Option<Rc<dyn Fn()>>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !manager_running(&page, &status) {
        return page;
    }
    if let Some(section) = status.parent() {
        section.set_visible(false);
    }
    let hotspot = Rc::new(Hotspot {
        connections: Connections::new(),
        active: Cell::new(false),
        busy: Cell::new(false),
        band: RefCell::new(String::new()),
        problem: RefCell::new(None),
        changed: RefCell::new(None),
    });

    let section = page.section("", "");
    let explanation = text::styled(&tr(
        "Other devices join this network to use this computer's internet connection. The Wi-Fi adapter cannot join another network meanwhile, so share a wired connection",
    ));
    text::set_color(&explanation, "colSubtext");
    explanation.set_xalign(0.0);
    explanation.set_wrap(true);
    explanation.set_margin_start(LABEL_START);
    section.append(&explanation);

    let name_group = page.subsection(&section, &tr("Network name"), "");
    let name = TextField::new(&page.theme, Style::Outlined, &tr("Name of the network"));
    name.root.set_hexpand(true);
    name.set_text(&glib::host_name());
    name_group.append(&name.root);
    page.keep(name.clone());

    let password_group = page.subsection(&section, &tr("Password"), &tr("8 to 63 characters"));
    let password = page.secret_field(&password_group, &tr("Password"));

    let band_group = page.subsection(&section, &tr("Band"), "");
    let choices = BANDS
        .iter()
        .map(|(label, value)| Choice {
            label: tr(label),
            icon: "",
            value: Value::from(*value),
        })
        .collect();
    let bands = page.selection_of(&band_group, choices, &[], || Value::from(""), {
        let hotspot = Rc::downgrade(&hotspot);
        move |value| {
            if let Some(hotspot) = hotspot.upgrade() {
                hotspot
                    .band
                    .replace(value.as_str().unwrap_or_default().to_owned());
                hotspot.announce();
            }
        }
    });

    let switch = page.switch(&section, "wifi_tethering", &tr("Share the connection"), {
        let hotspot = Rc::downgrade(&hotspot);
        let (name, password) = (Rc::downgrade(&name), Rc::downgrade(&password));
        move |wanted| {
            let (Some(hotspot), Some(name), Some(password)) =
                (hotspot.upgrade(), name.upgrade(), password.upgrade())
            else {
                return;
            };
            hotspot.toggle(wanted, name.text().trim(), &password.text());
        }
    });
    switch.bind({
        let hotspot = Rc::downgrade(&hotspot);
        move || {
            hotspot
                .upgrade()
                .is_some_and(|hotspot| hotspot.active.get())
        }
    });

    let problem = text::styled("");
    text::set_color(&problem, "colError");
    problem.set_xalign(0.0);
    problem.set_wrap(true);
    problem.set_margin_start(STATUS_START);
    problem.set_visible(false);
    section.append(&problem);

    let follow: Rc<dyn Fn()> = Rc::new({
        let hotspot = Rc::downgrade(&hotspot);
        let switch = Rc::downgrade(&switch);
        let bands = Rc::downgrade(&bands);
        move || {
            let (Some(hotspot), Some(switch), Some(bands)) =
                (hotspot.upgrade(), switch.upgrade(), bands.upgrade())
            else {
                return;
            };
            let active = hotspot
                .connections
                .list
                .borrow()
                .iter()
                .any(|connection| connection.name == CONNECTION && connection.active);
            hotspot.active.set(active);
            switch.refresh();
            switch.set_enabled(!hotspot.busy.get());
            bands.set_current(&Value::from(hotspot.band.borrow().clone()));
            let message = hotspot.problem.borrow().clone();
            problem.set_visible(message.is_some());
            problem.set_text(&message.unwrap_or_default());
        }
    });
    hotspot.changed.replace(Some(follow.clone()));
    hotspot.connections.connect_changed(move || follow());
    hotspot.read(Rc::downgrade(&name), Rc::downgrade(&password));
    page.keep(hotspot);
    page
}

impl Hotspot {
    fn announce(&self) {
        let changed = self.changed.borrow().clone();
        if let Some(changed) = changed {
            changed();
        }
    }

    fn read(self: &Rc<Self>, name: Weak<TextField>, password: Weak<TextField>) {
        let hotspot = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let found = nmcli(&[
                "-s",
                "-g",
                "802-11-wireless.ssid,802-11-wireless-security.psk,802-11-wireless.band",
                "connection",
                "show",
                "id",
                CONNECTION,
            ])
            .await;
            let Some(hotspot) = hotspot.upgrade() else {
                return;
            };
            if found.success {
                let lines: Vec<&str> = found.output.lines().collect();
                let field = |index: usize| lines.get(index).copied().unwrap_or_default();
                if let Some(name) = name.upgrade().filter(|_| !field(0).is_empty()) {
                    name.set_text(field(0));
                }
                if let Some(password) = password.upgrade() {
                    password.set_text(field(1));
                }
                hotspot.band.replace(field(2).to_owned());
            }
            hotspot.announce();
        });
    }

    fn toggle(self: &Rc<Self>, on: bool, ssid: &str, password: &str) {
        if self.busy.get() {
            return;
        }
        let fits = (PASSWORD_LENGTH.0..=PASSWORD_LENGTH.1).contains(&password.chars().count());
        if on && (ssid.is_empty() || !fits) {
            self.problem.replace(Some(if ssid.is_empty() {
                tr("The network needs a name")
            } else {
                tr("The password needs 8 to 63 characters")
            }));
            self.announce();
            return;
        }
        self.busy.set(true);
        self.problem.replace(None);
        self.announce();
        let hotspot = Rc::downgrade(self);
        let (ssid, password, band) = (
            ssid.to_owned(),
            password.to_owned(),
            self.band.borrow().clone(),
        );
        glib::spawn_future_local(async move {
            let finished = if on {
                nmcli(&["connection", "delete", "id", CONNECTION]).await;
                let mut arguments = vec![
                    "device", "wifi", "hotspot", "con-name", CONNECTION, "ssid", &ssid, "password",
                    &password,
                ];
                if !band.is_empty() {
                    arguments.extend(["band", band.as_str()]);
                }
                nmcli(&arguments).await
            } else {
                nmcli(&["connection", "down", "id", CONNECTION]).await
            };
            let Some(hotspot) = hotspot.upgrade() else {
                return;
            };
            hotspot.busy.set(false);
            if !finished.success {
                let message = finished
                    .errors
                    .lines()
                    .find(|line| !line.trim().is_empty())
                    .map(|line| line.trim_start_matches("Error: ").to_owned())
                    .unwrap_or_else(|| tr("NetworkManager could not start the hotspot"));
                hotspot.problem.replace(Some(message));
            }
            hotspot.announce();
        });
    }
}
