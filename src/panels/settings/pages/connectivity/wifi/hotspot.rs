use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::panels::settings::pages::connectivity::network::check_manager_running;
use crate::platform::security::pam;
use crate::services::network::net::{Connections, nmcli};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::textfield::TextField;

const CONNECTION: &str = "Hotspot";
const SSID: &str = "802-11-wireless.ssid";
const PSK: &str = "802-11-wireless-security.psk";
const BAND: &str = "802-11-wireless.band";
const LABEL_START: i32 = 2;
const STATUS_START: i32 = 8;
const PASSWORD_LENGTH: (usize, usize) = (8, 63);
const BANDS: [(&str, &str); 3] = [("Automatic", ""), ("2.4 GHz", "bg"), ("5 GHz", "a")];

struct Hotspot {
    connections: Rc<Connections>,
    is_active: Cell<bool>,
    is_busy: Cell<bool>,
    band: RefCell<String>,
    problem: RefCell<Option<String>>,
    on_changed: RefCell<Option<Rc<dyn Fn()>>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !check_manager_running(&page, &status) {
        return page;
    }
    if let Some(section) = status.parent() {
        section.set_visible(false);
    }
    let hotspot = Rc::new(Hotspot {
        connections: Connections::new(),
        is_active: Cell::new(false),
        is_busy: Cell::new(false),
        band: RefCell::new(String::new()),
        problem: RefCell::new(None),
        on_changed: RefCell::new(None),
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
    let name_field = TextField::new(&page.theme, Style::Outlined, &tr("Name of the network"));
    name_field.root.set_hexpand(true);
    name_field.set_text(&glib::host_name());
    name_group.append(&name_field.root);
    page.keep(name_field.clone());

    let password_group = page.subsection(&section, &tr("Password"), &tr("8 to 63 characters"));
    let password_field = page.secret_field(&password_group, &tr("Password"));

    let band_group = page.subsection(&section, &tr("Band"), "");
    let choices = BANDS
        .iter()
        .map(|(label, value)| Choice {
            label: tr(label),
            icon: "",
            value: Value::from(*value),
        })
        .collect();
    let band_selection = page.selection_of(&band_group, choices, &[], || Value::from(""), {
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

    let share_switch = page.switch(&section, "wifi_tethering", &tr("Share the connection"), {
        let hotspot = Rc::downgrade(&hotspot);
        let (name_field, password_field) =
            (Rc::downgrade(&name_field), Rc::downgrade(&password_field));
        move |is_on| {
            let (Some(hotspot), Some(name_field), Some(password_field)) = (
                hotspot.upgrade(),
                name_field.upgrade(),
                password_field.upgrade(),
            ) else {
                return;
            };
            let secret = password_field.text();
            hotspot.toggle(is_on, name_field.text().trim(), &secret);
            pam::wipe(&mut secret.into_bytes());
        }
    });
    share_switch.bind({
        let hotspot = Rc::downgrade(&hotspot);
        move || {
            hotspot
                .upgrade()
                .is_some_and(|hotspot| hotspot.is_active.get())
        }
    });

    let problem_label = text::styled("");
    text::set_color(&problem_label, "colError");
    problem_label.set_xalign(0.0);
    problem_label.set_wrap(true);
    problem_label.set_margin_start(STATUS_START);
    problem_label.set_visible(false);
    section.append(&problem_label);

    let show_state: Rc<dyn Fn()> = Rc::new({
        let hotspot = Rc::downgrade(&hotspot);
        let share_switch = Rc::downgrade(&share_switch);
        let band_selection = Rc::downgrade(&band_selection);
        move || {
            let (Some(hotspot), Some(share_switch), Some(band_selection)) = (
                hotspot.upgrade(),
                share_switch.upgrade(),
                band_selection.upgrade(),
            ) else {
                return;
            };
            let is_active = hotspot
                .connections
                .list
                .borrow()
                .iter()
                .any(|connection| connection.name == CONNECTION && connection.is_active);
            hotspot.is_active.set(is_active);
            share_switch.refresh();
            share_switch.set_enabled(!hotspot.is_busy.get());
            band_selection.set_current(&Value::from(hotspot.band.borrow().clone()));
            let message = hotspot.problem.borrow().clone();
            problem_label.set_visible(message.is_some());
            problem_label.set_text(&message.unwrap_or_default());
        }
    });
    hotspot.on_changed.replace(Some(show_state.clone()));
    hotspot.connections.connect_changed(move || show_state());
    hotspot.read(Rc::downgrade(&name_field), Rc::downgrade(&password_field));
    page.keep(hotspot);
    page
}

impl Hotspot {
    fn announce(&self) {
        let on_changed = self.on_changed.borrow().clone();
        if let Some(on_changed) = on_changed {
            on_changed();
        }
    }

    fn read(self: &Rc<Self>, name_field: Weak<TextField>, password_field: Weak<TextField>) {
        let hotspot = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let properties = [SSID, PSK, BAND].join(",");
            let reply = nmcli(&[
                "-s",
                "-t",
                "-m",
                "multiline",
                "-f",
                &properties,
                "connection",
                "show",
                "id",
                CONNECTION,
            ])
            .await;
            if let Some(hotspot) = hotspot.upgrade() {
                if reply.is_success {
                    let fields = parse_fields(&reply.stdout);
                    let read_field = |key: &str| fields.get(key).copied().unwrap_or_default();
                    if let Some(name_field) = name_field
                        .upgrade()
                        .filter(|_| !read_field(SSID).is_empty())
                    {
                        name_field.set_text(read_field(SSID));
                    }
                    if let Some(password_field) = password_field.upgrade() {
                        password_field.set_text(read_field(PSK));
                    }
                    hotspot.band.replace(read_field(BAND).to_owned());
                }
                hotspot.announce();
            }
            pam::wipe(&mut reply.stdout.into_bytes());
        });
    }

    fn toggle(self: &Rc<Self>, is_on: bool, ssid: &str, password: &str) {
        if self.is_busy.get() {
            return;
        }
        let is_length_valid =
            (PASSWORD_LENGTH.0..=PASSWORD_LENGTH.1).contains(&password.chars().count());
        if is_on && (ssid.is_empty() || !is_length_valid) {
            self.problem.replace(Some(if ssid.is_empty() {
                tr("The network needs a name")
            } else {
                tr("The password needs 8 to 63 characters")
            }));
            self.announce();
            return;
        }
        self.is_busy.set(true);
        self.problem.replace(None);
        self.announce();
        let hotspot = Rc::downgrade(self);
        let (ssid, password, band) = (
            ssid.to_owned(),
            password.to_owned(),
            self.band.borrow().clone(),
        );
        glib::spawn_future_local(async move {
            let reply = if is_on {
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
            hotspot.is_busy.set(false);
            if !reply.is_success {
                let message = reply
                    .stderr
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

fn parse_fields(output: &str) -> HashMap<&str, &str> {
    output
        .lines()
        .filter_map(|line| line.split_once(':'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fields_are_read_by_name_with_their_values_whole() {
        assert_eq!(
            parse_fields(
                "802-11-wireless.ssid:Cafe:Guest\n802-11-wireless-security.psk:pa:ss\\word1\n802-11-wireless.band:a\n"
            ),
            HashMap::from([(SSID, "Cafe:Guest"), (PSK, "pa:ss\\word1"), (BAND, "a")])
        );
        assert_eq!(
            parse_fields("802-11-wireless.ssid:Cafe\n802-11-wireless.band:a\n"),
            HashMap::from([(SSID, "Cafe"), (BAND, "a")])
        );
    }
}
