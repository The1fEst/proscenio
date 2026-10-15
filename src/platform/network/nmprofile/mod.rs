pub mod enterprise;
pub mod ipconfig;
mod secrets;
pub mod vpn;
pub mod wireguard;
pub mod wireless;

use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::collections::{BTreeMap, HashMap};

use self::ipconfig::Family;
use self::wireguard::is_valid_key;
use self::wireless::Security;
use crate::core::i18n::{tr, trf};

pub type Settings = HashMap<String, HashMap<String, Variant>>;
type Entry = BTreeMap<String, Variant>;

pub const CONNECTION: &str = "connection";
pub const WIRED: &str = "802-3-ethernet";
pub const WIRELESS: &str = "802-11-wireless";
pub const SECURITY: &str = "802-11-wireless-security";
pub const WIREGUARD: &str = "wireguard";
pub const VPN: &str = "vpn";
pub const EAP: &str = "802-1x";
pub const VLAN: &str = "vlan";
pub const BRIDGE: &str = "bridge";
pub const BOND: &str = "bond";
pub const OPENVPN: &str = "org.freedesktop.NetworkManager.openvpn";
const VLAN_ID_MAX: u32 = 4094;
const BOND_MODE: &str = "active-backup";
const BOND_MONITOR: (&str, &str) = ("miimon", "100");
const SECRET_FLAG_NONE: &str = "0";
const SECRET_FLAG_AGENT_OWNED: u32 = 0x1;
pub const SECRET_SETTINGS: [&str; 4] = [SECURITY, EAP, WIREGUARD, VPN];
const WEP_ASCII_LENGTHS: [usize; 2] = [5, 13];
const WEP_HEX_LENGTHS: [usize; 2] = [10, 26];
const INTERFACE_LENGTH: usize = 15;
const SSID_LENGTH: usize = 32;
const PSK_LENGTH: (usize, usize) = (8, 63);
const PSK_HEX_LENGTH: usize = 64;

fn wep_key_fits(key: &str) -> bool {
    WEP_ASCII_LENGTHS.contains(&key.len())
        || (WEP_HEX_LENGTHS.contains(&key.len())
            && key.chars().all(|character| character.is_ascii_hexdigit()))
}

#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub settings: Settings,
    stored_peer_keys: Vec<Option<String>>,
}

impl PartialEq for Profile {
    fn eq(&self, other: &Self) -> bool {
        self.settings == other.settings
    }
}

pub fn items(text: &str) -> Vec<String> {
    text.split([',', ' ', '\n'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn parse_mac(text: &str) -> Result<String, String> {
    let text = text.trim();
    let is_named = matches!(text, "" | "preserve" | "permanent" | "random" | "stable");
    let parts: Vec<&str> = text.split(':').collect();
    let is_address = parts.len() == 6
        && parts.iter().all(|part| {
            part.len() == 2 && part.chars().all(|character| character.is_ascii_hexdigit())
        });
    if is_named || is_address {
        Ok(text.to_owned())
    } else {
        Err(trf(
            "%1 is neither a MAC address nor preserve, permanent, random or stable",
            &[text],
        ))
    }
}

pub fn free_name(base: &str, taken_names: &[String]) -> String {
    std::iter::once(base.to_owned())
        .chain((2..).map(|number| format!("{base} {number}")))
        .find(|name| !taken_names.contains(name))
        .unwrap_or_default()
}

pub fn free_interface(prefix: &str, taken_names: &[String]) -> String {
    (0..)
        .map(|number| format!("{prefix}{number}"))
        .find(|name| !taken_names.contains(name))
        .unwrap_or_default()
}

fn entry_string(entry: &Entry, key: &str) -> String {
    entry
        .get(key)
        .and_then(|value| value.str().map(str::to_owned))
        .unwrap_or_default()
}

fn entry_u32(entry: &Entry, key: &str) -> Option<u32> {
    entry.get(key).and_then(Variant::get::<u32>)
}

impl Profile {
    pub fn from_variant(variant: &Variant) -> Option<Self> {
        variant.get::<Settings>().map(|settings| Profile {
            settings,
            ..Profile::default()
        })
    }

    pub fn to_variant(&self) -> Variant {
        self.settings.to_variant()
    }

    pub fn new(kind: &str, id: &str) -> Self {
        let mut profile = Profile::default();
        profile.put(CONNECTION, "id", id.to_variant());
        profile.put(CONNECTION, "uuid", glib::uuid_string_random().to_variant());
        profile.put(CONNECTION, "type", kind.to_variant());
        profile.settings.entry(kind.to_owned()).or_default();
        let (ipv4, ipv6) = if kind == WIREGUARD {
            ("manual", "disabled")
        } else {
            ("auto", "auto")
        };
        profile.put("ipv4", "method", ipv4.to_variant());
        profile.put("ipv6", "method", ipv6.to_variant());
        profile
    }

    fn get(&self, setting: &str, key: &str) -> Option<&Variant> {
        self.settings.get(setting)?.get(key)
    }

    fn put(&mut self, setting: &str, key: &str, value: Variant) {
        self.settings
            .entry(setting.to_owned())
            .or_default()
            .insert(key.to_owned(), value);
    }

    fn remove(&mut self, setting: &str, key: &str) {
        if let Some(values) = self.settings.get_mut(setting) {
            values.remove(key);
        }
    }

    fn string(&self, setting: &str, key: &str) -> String {
        self.get(setting, key)
            .and_then(|value| value.str().map(str::to_owned))
            .unwrap_or_default()
    }

    fn set_string(&mut self, setting: &str, key: &str, value: &str) {
        if value.is_empty() {
            self.remove(setting, key);
        } else {
            self.put(setting, key, value.to_variant());
        }
    }

    fn flag(&self, setting: &str, key: &str, default: bool) -> bool {
        self.get(setting, key)
            .and_then(Variant::get::<bool>)
            .unwrap_or(default)
    }

    fn strings(&self, setting: &str, key: &str) -> Vec<String> {
        self.get(setting, key)
            .and_then(Variant::get::<Vec<String>>)
            .unwrap_or_default()
    }

    fn entries(&self, setting: &str, key: &str) -> Vec<Entry> {
        self.get(setting, key)
            .and_then(Variant::get::<Vec<Entry>>)
            .unwrap_or_default()
    }

    pub fn id(&self) -> String {
        self.string(CONNECTION, "id")
    }

    pub fn set_id(&mut self, id: &str) {
        self.set_string(CONNECTION, "id", id);
    }

    pub fn uuid(&self) -> String {
        self.string(CONNECTION, "uuid")
    }

    pub fn kind(&self) -> String {
        self.string(CONNECTION, "type")
    }

    pub fn is_autoconnect_enabled(&self) -> bool {
        self.flag(CONNECTION, "autoconnect", true)
    }

    pub fn set_autoconnect_enabled(&mut self, is_enabled: bool) {
        self.put(CONNECTION, "autoconnect", is_enabled.to_variant());
    }

    pub fn is_for_all_users(&self) -> bool {
        self.strings(CONNECTION, "permissions").is_empty()
    }

    pub fn set_for_all_users(&mut self, is_for_all_users: bool, user: &str) {
        let permissions = if is_for_all_users {
            Vec::new()
        } else {
            vec![format!("user:{user}:")]
        };
        self.put(CONNECTION, "permissions", permissions.to_variant());
    }

    pub fn metered(&self) -> i32 {
        self.get(CONNECTION, "metered")
            .and_then(Variant::get::<i32>)
            .unwrap_or(0)
    }

    pub fn set_metered(&mut self, metered: i32) {
        self.put(CONNECTION, "metered", metered.to_variant());
    }

    pub fn interface(&self) -> String {
        self.string(CONNECTION, "interface-name")
    }

    pub fn set_interface(&mut self, name: &str) {
        self.set_string(CONNECTION, "interface-name", name);
    }

    pub fn mtu(&self) -> u32 {
        self.get(&self.kind(), "mtu")
            .and_then(Variant::get::<u32>)
            .unwrap_or(0)
    }

    pub fn set_mtu(&mut self, mtu: u32) {
        let kind = self.kind();
        self.put(&kind, "mtu", mtu.to_variant());
    }

    pub fn cloned_mac(&self) -> String {
        self.string(&self.kind(), "assigned-mac-address")
    }

    pub fn set_cloned_mac(&mut self, mac: &str) {
        let kind = self.kind();
        self.set_string(&kind, "assigned-mac-address", mac);
    }

    pub fn vlan_parent(&self) -> String {
        self.string(VLAN, "parent")
    }

    pub fn set_vlan_parent(&mut self, parent: &str) {
        self.set_string(VLAN, "parent", parent);
    }

    pub fn vlan_id(&self) -> u32 {
        self.get(VLAN, "id")
            .and_then(Variant::get::<u32>)
            .unwrap_or(0)
    }

    pub fn set_vlan_id(&mut self, id: u32) {
        self.put(VLAN, "id", id.to_variant());
    }

    pub fn is_stp_enabled(&self) -> bool {
        self.flag(BRIDGE, "stp", true)
    }

    pub fn set_stp_enabled(&mut self, is_enabled: bool) {
        self.put(BRIDGE, "stp", is_enabled.to_variant());
    }

    fn bond_options(&self) -> BTreeMap<String, String> {
        self.get(BOND, "options")
            .and_then(Variant::get::<BTreeMap<String, String>>)
            .unwrap_or_default()
    }

    pub fn bond_mode(&self) -> String {
        self.bond_options()
            .remove("mode")
            .unwrap_or_else(|| BOND_MODE.to_owned())
    }

    pub fn set_bond_mode(&mut self, mode: &str) {
        let mut options = self.bond_options();
        options.insert("mode".to_owned(), mode.to_owned());
        options
            .entry(BOND_MONITOR.0.to_owned())
            .or_insert_with(|| BOND_MONITOR.1.to_owned());
        self.put(BOND, "options", options.to_variant());
    }

    pub fn new_port(controller: &str, kind: &str, device: &str) -> Profile {
        let mut profile = Profile::new(WIRED, &format!("{controller} port {device}"));
        profile.settings.remove("ipv4");
        profile.settings.remove("ipv6");
        profile.set_interface(device);
        profile.put(CONNECTION, "master", controller.to_variant());
        profile.put(CONNECTION, "slave-type", kind.to_variant());
        profile
    }

    pub fn normalized(&self) -> Profile {
        let mut profile = self.clone();
        for family in [Family::V4, Family::V6] {
            if profile.settings.contains_key(family.setting()) {
                let ip = profile.ip(family);
                profile.set_ip(family, &ip);
            }
        }
        profile
    }

    pub fn problem(&self, is_secret_kept: bool) -> Option<String> {
        if self.id().trim().is_empty() {
            return Some(tr("The connection needs a name"));
        }
        let kind = self.kind();
        let interface = self.interface();
        if kind == WIREGUARD && interface.is_empty() {
            return Some(tr("A WireGuard connection needs an interface name"));
        }
        if matches!(kind.as_str(), BRIDGE | BOND) && interface.is_empty() {
            return Some(tr("A bridge or bond needs an interface name"));
        }
        if kind == VLAN {
            if self.vlan_parent().is_empty() {
                return Some(tr("A VLAN needs a parent device"));
            }
            if !(1..=VLAN_ID_MAX).contains(&self.vlan_id()) {
                return Some(tr("A VLAN ID is a number from 1 to 4094"));
            }
        }
        if interface.len() > INTERFACE_LENGTH
            || interface.contains(|character: char| character.is_whitespace() || character == '/')
        {
            return Some(trf("%1 is not a valid interface name", &[&interface]));
        }
        if kind == WIRELESS {
            let ssid = self.ssid();
            if ssid.is_empty() || ssid.len() > SSID_LENGTH {
                return Some(tr("The network name has to be 1 to 32 bytes long"));
            }
            let psk = self.psk();
            let is_kept = is_secret_kept && psk.is_empty();
            let psk_fits = is_kept
                || (PSK_LENGTH.0..=PSK_LENGTH.1).contains(&psk.len())
                || (psk.len() == PSK_HEX_LENGTH
                    && psk.chars().all(|character| character.is_ascii_hexdigit()));
            match self.security() {
                Security::Personal if !psk_fits => {
                    return Some(tr("A WPA password has 8 to 63 characters"));
                }
                Security::Wpa3 if psk.is_empty() && !is_kept => {
                    return Some(tr("A WPA3 network needs a password"));
                }
                Security::Wep if !self.wep_key().is_empty() && !wep_key_fits(&self.wep_key()) => {
                    return Some(tr(
                        "A WEP key has 5 or 13 characters, or 10 or 26 hexadecimal digits",
                    ));
                }
                _ => {}
            }
        }
        if kind == VPN && self.is_openvpn() {
            let vpn = self.openvpn();
            let is_blank = |value: &str| value.trim().is_empty();
            if is_blank(&vpn.gateway) {
                return Some(tr("An OpenVPN connection needs a gateway"));
            }
            if !vpn.uses_static_key() && is_blank(&vpn.ca) {
                return Some(tr("OpenVPN needs the CA certificate of its server"));
            }
            if vpn.uses_certificates() && (is_blank(&vpn.cert) || is_blank(&vpn.key)) {
                return Some(tr(
                    "OpenVPN with certificates needs a user certificate and a private key",
                ));
            }
            if vpn.uses_password() && is_blank(&vpn.username) {
                return Some(tr("OpenVPN with a password needs a user name"));
            }
            if vpn.uses_static_key()
                && (is_blank(&vpn.static_key)
                    || is_blank(&vpn.remote_ip)
                    || is_blank(&vpn.local_ip))
            {
                return Some(tr(
                    "A static key OpenVPN needs the key file and both tunnel addresses",
                ));
            }
        }
        if self.has_eap() {
            let eap = self.eap();
            if eap.identity.trim().is_empty() {
                return Some(tr("802.1X needs a user name"));
            }
            let has_key = |key| self.get(EAP, key).is_some();
            if !eap.is_tunneled() && !(has_key("client-cert") && has_key("private-key")) {
                return Some(tr("TLS needs a user certificate and a private key"));
            }
        }
        if kind == WIREGUARD {
            let private_key = self.private_key();
            let is_kept = is_secret_kept && private_key.is_empty();
            if !is_kept && !is_valid_key(&private_key) {
                return Some(tr("The private key is not a WireGuard key"));
            }
            for peer in self.peers() {
                if !is_valid_key(&peer.public_key) {
                    return Some(trf("%1 is not a WireGuard public key", &[&peer.public_key]));
                }
                if !peer.preshared_key.is_empty() && !is_valid_key(&peer.preshared_key) {
                    return Some(tr("A preshared key is not a WireGuard key"));
                }
            }
        }
        for family in [Family::V4, Family::V6] {
            let ip = self.ip(family);
            if ip.method == "manual" && ip.addresses.is_empty() {
                return Some(trf(
                    "Manual %1 needs at least one address",
                    &[family.name()],
                ));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_profile_takes_the_first_free_name_and_interface() {
        let to_strings = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            free_name("WireGuard", &to_strings(&["Office"])),
            "WireGuard"
        );
        assert_eq!(
            free_name("WireGuard", &to_strings(&["WireGuard", "WireGuard 2"])),
            "WireGuard 3"
        );
        assert_eq!(
            free_interface("wg", &to_strings(&["wg0", "eth0", "wg2"])),
            "wg1"
        );
        assert_eq!(free_interface("br", &to_strings(&["wg0"])), "br0");
    }

    #[test]
    fn vlans_bonds_and_ports_carry_what_networkmanager_needs() {
        let mut vlan = Profile::new(VLAN, "Office VLAN");
        assert_eq!(
            vlan.problem(false),
            Some(tr("A VLAN needs a parent device"))
        );
        vlan.set_vlan_parent("eth0");
        assert_eq!(
            vlan.problem(false),
            Some(tr("A VLAN ID is a number from 1 to 4094"))
        );
        vlan.set_vlan_id(10);
        assert_eq!(vlan.problem(false), None);
        assert_eq!(
            (vlan.vlan_parent(), vlan.vlan_id()),
            ("eth0".to_owned(), 10)
        );

        let mut bond = Profile::new(BOND, "Bond");
        assert_eq!(
            bond.problem(false),
            Some(tr("A bridge or bond needs an interface name"))
        );
        bond.set_interface("bond0");
        bond.set_bond_mode("802.3ad");
        assert_eq!(bond.bond_mode(), "802.3ad");
        assert_eq!(
            bond.bond_options().get("miimon").map(String::as_str),
            Some("100")
        );
        assert_eq!(bond.problem(false), None);

        let port = Profile::new_port("bond0", BOND, "eth1");
        assert_eq!(port.kind(), WIRED);
        assert_eq!(port.interface(), "eth1");
        assert_eq!(port.string(CONNECTION, "master"), "bond0");
        assert_eq!(port.string(CONNECTION, "slave-type"), BOND);
        assert!(!port.settings.contains_key("ipv4"));
        assert_eq!(port.problem(false), None);
    }
}
