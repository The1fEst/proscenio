use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::pin::Pin;
use std::rc::{Rc, Weak};

use crate::core::i18n::tr;
use crate::panels::settings::content::{Choice, Context, Page, Style};
use crate::panels::settings::pages::connectivity::network::check_manager_running;
use crate::platform::network::nmprofile::{SECURITY, WIRELESS};
use crate::platform::security::pam;
use crate::services::network::net::Connections;
use crate::services::network::networkmanager::NetworkManager;
use crate::services::network::networkmanager::calls::{self, NO_OBJECT};
use crate::services::network::nmsettings;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::icon_button;
use crate::ui::widgets::input::textfield::TextField;
use crate::ui::widgets::overlay::windowdialog::WindowDialog;

use super::qrdialog;

const CONNECTION: &str = "Hotspot";
const WPA_CAPABILITY: u32 = 0x10;
const RSN_CAPABILITY: u32 = 0x20;
const DISABLED_PMF: i32 = 1;
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

    let (code_button, _) = icon_button(&page.theme, "qr_code", false, &tr("Share"));
    code_button.set_halign(gtk4::Align::Start);
    code_button.set_visible(false);
    code_button.connect_clicked({
        let theme = page.theme.clone();
        let present: Rc<dyn Fn(Rc<WindowDialog>)> = Rc::new(context.dialog_presenter());
        move |button| {
            qrdialog::open(
                &theme,
                present.clone(),
                qrdialog::Source::Id(CONNECTION.to_owned()),
                button,
            )
        }
    });
    section.append(&code_button);

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
            code_button.set_visible(is_active);
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
            let saved_connection = match nmsettings::find_uuids(CONNECTION).await.first() {
                Some(uuid) => nmsettings::load(uuid).await.ok(),
                None => None,
            };
            let Some(hotspot) = hotspot.upgrade() else {
                return;
            };
            if let Some(saved_connection) = saved_connection {
                let profile = saved_connection.profile;
                let ssid = profile.ssid();
                if let Some(name_field) = name_field.upgrade().filter(|_| !ssid.is_empty()) {
                    name_field.set_text(&ssid);
                }
                if let Some(password_field) = password_field.upgrade() {
                    let psk = profile.psk();
                    password_field.set_text(&psk);
                    pam::wipe(&mut psk.into_bytes());
                }
                let band = profile
                    .settings
                    .get(WIRELESS)
                    .and_then(|wireless| wireless.get("band"))
                    .and_then(|band| band.str().map(str::to_owned))
                    .unwrap_or_default();
                hotspot.band.replace(band);
            }
            hotspot.announce();
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
        let snapshot = NetworkManager::instance().snapshot();
        let call: Pin<Box<dyn Future<Output = Result<(), String>>>> = if is_on {
            let device = snapshot
                .wifi_devices()
                .find(|device| device.can_host_access_point())
                .map(|device| (device.path.clone(), device.wireless_capabilities));
            let settings = device.as_ref().map(|(_, capabilities)| {
                build_hotspot_settings(ssid, password, &self.band.borrow(), *capabilities)
            });
            Box::pin(start_hotspot(device.map(|(path, _)| path), settings))
        } else {
            let active_connection = snapshot
                .find_active_by_id(CONNECTION)
                .map(|active_connection| active_connection.path.clone());
            Box::pin(async move {
                match active_connection {
                    Some(active_connection) => {
                        calls::deactivate_connection(&active_connection).await
                    }
                    None => Ok(()),
                }
            })
        };
        glib::spawn_future_local(async move {
            let call_result = call.await;
            let Some(hotspot) = hotspot.upgrade() else {
                return;
            };
            hotspot.is_busy.set(false);
            if let Err(message) = call_result {
                hotspot.problem.replace(Some(message));
            }
            hotspot.announce();
        });
    }
}

async fn start_hotspot(device: Option<String>, settings: Option<Variant>) -> Result<(), String> {
    for uuid in nmsettings::find_uuids(CONNECTION).await {
        let _ = nmsettings::delete(&uuid).await;
    }
    let (Some(device), Some(settings)) = (device, settings) else {
        return Err(tr("NetworkManager could not start the hotspot"));
    };
    let (_, active_connection) =
        calls::add_and_activate_connection(settings, &device, NO_OBJECT).await?;
    if calls::wait_for_activation(&active_connection).await {
        return Ok(());
    }
    Err(tr("NetworkManager could not start the hotspot"))
}

fn build_hotspot_settings(ssid: &str, password: &str, band: &str, capabilities: u32) -> Variant {
    let new_entry = |key: &str, value: Variant| (key.to_owned(), value);
    let mut wireless = HashMap::from([
        new_entry("ssid", ssid.as_bytes().to_variant()),
        new_entry("mode", "ap".to_variant()),
    ]);
    if !band.is_empty() {
        wireless.insert("band".to_owned(), band.to_variant());
    }
    let mut security = HashMap::from([
        new_entry("key-mgmt", "wpa-psk".to_variant()),
        new_entry("psk", password.to_variant()),
        new_entry("pmf", DISABLED_PMF.to_variant()),
    ]);
    let ciphers = if capabilities & RSN_CAPABILITY != 0 {
        Some(("rsn", "ccmp"))
    } else if capabilities & WPA_CAPABILITY != 0 {
        Some(("wpa", "tkip"))
    } else {
        None
    };
    if let Some((protocol, cipher)) = ciphers {
        security.insert("proto".to_owned(), vec![protocol].to_variant());
        security.insert("pairwise".to_owned(), vec![cipher].to_variant());
        security.insert("group".to_owned(), vec![cipher].to_variant());
    }
    let settings: HashMap<String, HashMap<String, Variant>> = HashMap::from([
        (
            "connection".to_owned(),
            HashMap::from([
                new_entry("id", CONNECTION.to_variant()),
                new_entry("type", WIRELESS.to_variant()),
                new_entry("autoconnect", false.to_variant()),
            ]),
        ),
        (WIRELESS.to_owned(), wireless),
        (SECURITY.to_owned(), security),
        (
            "ipv4".to_owned(),
            HashMap::from([new_entry("method", "shared".to_variant())]),
        ),
        (
            "ipv6".to_owned(),
            HashMap::from([new_entry("method", "ignore".to_variant())]),
        ),
    ]);
    settings.to_variant()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::network::nmprofile::Profile;

    #[test]
    fn a_hotspot_shares_the_connection_with_the_strongest_cipher_the_adapter_offers() {
        let read_settings = |settings: Variant| {
            settings
                .get::<HashMap<String, HashMap<String, Variant>>>()
                .unwrap()
        };
        let settings = read_settings(build_hotspot_settings("Attic", "secret12", "a", 0x70));
        assert_eq!(
            settings["connection"],
            HashMap::from([
                ("id".to_owned(), "Hotspot".to_variant()),
                ("type".to_owned(), WIRELESS.to_variant()),
                ("autoconnect".to_owned(), false.to_variant()),
            ])
        );
        assert_eq!(
            settings[WIRELESS],
            HashMap::from([
                ("ssid".to_owned(), b"Attic".to_variant()),
                ("mode".to_owned(), "ap".to_variant()),
                ("band".to_owned(), "a".to_variant()),
            ])
        );
        assert_eq!(
            settings[SECURITY],
            HashMap::from([
                ("key-mgmt".to_owned(), "wpa-psk".to_variant()),
                ("psk".to_owned(), "secret12".to_variant()),
                ("pmf".to_owned(), 1i32.to_variant()),
                ("proto".to_owned(), vec!["rsn"].to_variant()),
                ("pairwise".to_owned(), vec!["ccmp"].to_variant()),
                ("group".to_owned(), vec!["ccmp"].to_variant()),
            ])
        );
        assert_eq!(settings["ipv4"]["method"], "shared".to_variant());
        assert_eq!(settings["ipv6"]["method"], "ignore".to_variant());

        let wpa_settings = read_settings(build_hotspot_settings("Attic", "secret12", "", 0x50));
        assert!(!wpa_settings[WIRELESS].contains_key("band"));
        assert_eq!(wpa_settings[SECURITY]["proto"], vec!["wpa"].to_variant());
        assert_eq!(
            wpa_settings[SECURITY]["pairwise"],
            vec!["tkip"].to_variant()
        );

        let default_cipher_settings =
            read_settings(build_hotspot_settings("Attic", "secret12", "", 0x40));
        assert!(!default_cipher_settings[SECURITY].contains_key("proto"));
    }

    #[test]
    fn a_hotspot_profile_is_shared_as_a_wpa_network_that_is_not_hidden() {
        let profile =
            Profile::from_variant(&build_hotspot_settings("Attic", "secret12", "a", 0x70)).unwrap();
        let share = profile.to_network_share().unwrap();
        assert_eq!(share.ssid, "Attic");
        assert_eq!(share.password, "secret12");
        assert_eq!(share.payload, "WIFI:T:WPA;S:Attic;P:secret12;;");
    }
}
