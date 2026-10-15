use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use super::fields::{note, show_note};
use super::{BOND_MODES, EAP_METHODS, Editor, MTU_MAX, choices};
use crate::core::i18n::tr;
use crate::panels::settings::content::Page;
use crate::platform::network::nmprofile::wireless::Security;
use crate::platform::network::nmprofile::{self, BRIDGE, Profile};
use crate::ui::widgets::input::controls::{ComboBox, ConfigSwitch};

const VLAN_ID_MAX: i64 = 4094;
const METERED: [(&str, i32); 3] = [("Automatic", 0), ("Yes", 1), ("No", 2)];
const SECURITY: [(&str, Security); 5] = [
    ("None", Security::Open),
    ("WPA & WPA2 Personal", Security::Personal),
    ("WPA3 Personal", Security::Wpa3),
    ("WPA & WPA2 Enterprise", Security::Enterprise),
    ("WEP", Security::Wep),
];

impl Editor {
    fn eap_link(self: &Rc<Self>, page: &Page, parent: &gtk4::Box) {
        let method = self.draft.borrow().eap().method;
        let method_name = EAP_METHODS
            .iter()
            .find(|(_, known)| *known == method)
            .map_or(method.clone(), |(name, _)| tr(name));
        let open = self.open_eap.clone();
        page.link_row(
            parent,
            "shield_lock",
            &tr("Authentication"),
            &method_name,
            move || open(),
        );
    }

    pub(super) fn general(self: &Rc<Self>, page: &Page) {
        let section = self.section("tune", &tr("General"));
        let profile = self.draft.borrow().clone();
        self.field(&section, &tr("Name"), &profile.id(), |text, profile| {
            profile.set_id(text.trim());
            Ok(())
        });
        self.switch(
            &section,
            "autorenew",
            &tr("Connect automatically"),
            Profile::is_autoconnect_enabled,
            Profile::set_autoconnect_enabled,
        );
        let all_users_switch = self.switch(
            &section,
            "group",
            &tr("Available to all users"),
            Profile::is_for_all_users,
            |profile, is_for_all| {
                profile.set_for_all_users(is_for_all, &glib::user_name().to_string_lossy())
            },
        );
        self.hold(page.unkept_tip(
            &all_users_switch.button,
            &tr("Off keeps the connection to your own account"),
        ));
        let metered_section = self.subsection(
            page,
            &section,
            &tr("Metered connection"),
            &tr("A connection paid for by the amount of data. Apps hold back on large downloads over it"),
        );
        let metered_choice = match profile.metered() {
            1 | 3 => 1,
            2 | 4 => 2,
            _ => 0,
        };
        self.choose(
            &metered_section,
            choices(&METERED),
            Value::from(metered_choice),
            |profile, value| profile.set_metered(value.as_i64().unwrap_or(0) as i32),
        );
    }

    fn mac_and_mtu(self: &Rc<Self>, parent: &gtk4::Box, profile: &Profile) {
        self.field(
            parent,
            &tr("Cloned MAC address"),
            &profile.cloned_mac(),
            |text, profile| {
                profile.set_cloned_mac(&nmprofile::parse_mac(text)?);
                Ok(())
            },
        );
        self.spin(
            parent,
            "straighten",
            &tr("MTU (0 picks it automatically)"),
            (0, MTU_MAX),
            profile.mtu() as i64,
            |profile, mtu| profile.set_mtu(mtu as u32),
        );
    }

    fn device_combo(
        self: &Rc<Self>,
        parent: &gtk4::Box,
        current: &str,
        any: Option<String>,
        write: impl Fn(&mut Profile, &str) + 'static,
    ) {
        let mut items: Vec<String> = any.iter().cloned().collect();
        items.extend(self.devices.borrow().iter().cloned());
        if !current.is_empty() && !items.iter().any(|item| item == current) {
            items.push(current.to_owned());
        }
        let index = items.iter().position(|item| item == current).unwrap_or(0);
        let combo = ComboBox::new(&self.theme);
        combo.set_icon("settings_ethernet");
        combo.button.set_hexpand(true);
        combo.set_items(&items, index as i32);
        let has_any_entry = any.is_some();
        combo.connect_activated({
            let editor = Rc::downgrade(self);
            let combo = Rc::downgrade(&combo);
            move |index| {
                let (Some(editor), Some(combo)) = (editor.upgrade(), combo.upgrade()) else {
                    return;
                };
                let name = if has_any_entry && index == 0 {
                    String::new()
                } else {
                    items.get(index).cloned().unwrap_or_default()
                };
                let _ = editor.edit(|profile| {
                    write(profile, &name);
                    Ok(())
                });
                combo.set_items(&items, index as i32);
                editor.refresh();
            }
        });
        parent.append(&combo.button);
        self.hold(combo);
    }

    pub(super) fn ports_differ(&self) -> bool {
        let mut saved_devices: Vec<String> = self
            .saved_ports
            .borrow()
            .iter()
            .map(|(_, device)| device.clone())
            .collect();
        let mut current_devices = self.port_devices.borrow().clone();
        saved_devices.sort();
        current_devices.sort();
        saved_devices != current_devices
    }

    pub(super) fn vlan(self: &Rc<Self>, page: &Page) {
        let section = self.section("lan", &tr("VLAN"));
        let profile = self.draft.borrow().clone();
        let parent = self.subsection(page, &section, &tr("Parent device"), "");
        self.device_combo(
            &parent,
            &profile.vlan_parent(),
            None,
            Profile::set_vlan_parent,
        );
        if profile.vlan_parent().is_empty()
            && let Some(first_device) = self.devices.borrow().first()
        {
            let first_device = first_device.clone();
            let _ = self.edit(|profile| {
                profile.set_vlan_parent(&first_device);
                Ok(())
            });
        }
        self.spin(
            &section,
            "tag",
            &tr("VLAN ID"),
            (1, VLAN_ID_MAX),
            profile.vlan_id().max(1) as i64,
            |profile, id| profile.set_vlan_id(id as u32),
        );
        if profile.vlan_id() == 0 {
            let _ = self.edit(|profile| {
                profile.set_vlan_id(1);
                Ok(())
            });
        }
        self.field(
            &section,
            &tr("Interface name (empty picks one)"),
            &profile.interface(),
            |text, profile| {
                profile.set_interface(text.trim());
                Ok(())
            },
        );
    }

    pub(super) fn controller(self: &Rc<Self>, page: &Page, kind: &str) {
        let is_bridge = kind == BRIDGE;
        let section = if is_bridge {
            self.section("device_hub", &tr("Bridge"))
        } else {
            self.section("join", &tr("Bond"))
        };
        let profile = self.draft.borrow().clone();
        self.field(
            &section,
            &tr("Interface name"),
            &profile.interface(),
            |text, profile| {
                profile.set_interface(text.trim());
                Ok(())
            },
        );
        if is_bridge {
            let stp_switch = self.switch(
                &section,
                "account_tree",
                &tr("Spanning tree (STP)"),
                Profile::is_stp_enabled,
                Profile::set_stp_enabled,
            );
            self.hold(page.unkept_tip(
                &stp_switch.button,
                &tr("Keeps loops out when the bridge connects to other switches"),
            ));
        } else {
            let mode_section = self.subsection(page, &section, &tr("Mode"), "");
            self.choose(
                &mode_section,
                choices(&BOND_MODES),
                Value::from(profile.bond_mode()),
                |profile, value| profile.set_bond_mode(value.as_str().unwrap_or("active-backup")),
            );
        }
        let ports_section = self.subsection(
            page,
            &section,
            &tr("Ports"),
            &tr("The devices that join it. A device that joins leaves its own connection"),
        );
        if self.devices.borrow().is_empty() {
            let empty_note = note();
            show_note(&empty_note, Some(&tr("No wired devices")), "colSubtext");
            ports_section.append(&empty_note);
        }
        let devices = self.devices.borrow().clone();
        for device in devices {
            let switch = ConfigSwitch::new(&self.theme, "settings_ethernet", &device, {
                let editor = Rc::downgrade(self);
                let device = device.clone();
                move |is_on| {
                    let Some(editor) = editor.upgrade() else {
                        return;
                    };
                    {
                        let mut port_devices = editor.port_devices.borrow_mut();
                        port_devices.retain(|known| *known != device);
                        if is_on {
                            port_devices.push(device.clone());
                        }
                    }
                    editor.outcome.replace(None);
                    editor.refresh();
                }
            });
            switch.button.set_hexpand(true);
            switch.bind({
                let editor = Rc::downgrade(self);
                move || {
                    editor
                        .upgrade()
                        .is_some_and(|editor| editor.port_devices.borrow().contains(&device))
                }
            });
            ports_section.append(&switch.button);
            self.hold(switch);
        }
    }

    pub(super) fn wired(self: &Rc<Self>, page: &Page) {
        let section = self.section("lan", &tr("Wired"));
        let profile = self.draft.borrow().clone();
        self.device_combo(
            &section,
            &profile.interface(),
            Some(tr("Any device")),
            Profile::set_interface,
        );
        self.mac_and_mtu(&section, &profile);
        let security_switch = self.switch(
            &section,
            "shield_lock",
            &tr("802.1X security"),
            Profile::has_eap,
            Profile::set_wired_eap,
        );
        self.hold(page.unkept_tip(
            &security_switch.button,
            &tr("Signs in to the network port, as office and campus networks ask"),
        ));
        if profile.has_eap() {
            self.eap_link(page, &section);
        }
    }

    pub(super) fn wireless(self: &Rc<Self>, page: &Page) {
        let section = self.section("wifi", &tr("Wi-Fi"));
        let profile = self.draft.borrow().clone();
        self.field(
            &section,
            &tr("Network name"),
            &profile.ssid(),
            |text, profile| {
                profile.set_ssid(text);
                Ok(())
            },
        );
        self.switch(
            &section,
            "visibility_off",
            &tr("Hidden network"),
            Profile::is_hidden,
            Profile::set_hidden,
        );
        let security_section = self.subsection(page, &section, &tr("Security"), "");
        match profile.security() {
            Security::Other => {
                page.notice(
                    &security_section,
                    "info",
                    &tr("This network uses a kind of security the editor does not offer, which stays as it is"),
                );
            }
            current => {
                let options: Vec<(String, Value)> = SECURITY
                    .iter()
                    .enumerate()
                    .map(|(index, (name, _))| (tr(name), Value::from(index)))
                    .collect();
                let index = SECURITY
                    .iter()
                    .position(|(_, security)| *security == current)
                    .unwrap_or(0);
                self.choose(
                    &security_section,
                    options,
                    Value::from(index),
                    |profile, value| {
                        let index = value.as_u64().unwrap_or(0) as usize;
                        profile.set_security(SECURITY[index].1);
                    },
                );
                match current {
                    Security::Personal | Security::Wpa3 => {
                        self.secret_field(
                            &security_section,
                            &tr("Password"),
                            &profile.psk(),
                            profile.is_agent_owned(nmprofile::SECURITY, "psk-flags"),
                            |text, profile| {
                                profile.set_psk(text);
                                Ok(())
                            },
                        );
                    }
                    Security::Wep => {
                        self.secret_field(
                            &security_section,
                            &tr("Key"),
                            &profile.wep_key(),
                            profile.is_agent_owned(nmprofile::SECURITY, "wep-key-flags"),
                            |text, profile| {
                                profile.set_wep_key(text);
                                Ok(())
                            },
                        );
                    }
                    Security::Enterprise => self.eap_link(page, &security_section),
                    Security::Open | Security::Other => {}
                }
            }
        }
        self.mac_and_mtu(&section, &profile);
    }
}
