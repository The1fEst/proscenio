use gtk4::glib::{self, Variant};
use gtk4::prelude::*;
use std::collections::{BTreeMap, HashMap};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use crate::core::i18n::{tr, trf};

pub type Settings = HashMap<String, HashMap<String, Variant>>;
type Entry = BTreeMap<String, Variant>;

pub const CONNECTION: &str = "connection";
pub const WIRED: &str = "802-3-ethernet";
pub const WIRELESS: &str = "802-11-wireless";
pub const SECURITY: &str = "802-11-wireless-security";
pub const WIREGUARD: &str = "wireguard";
pub const VPN: &str = "vpn";
pub const SECRET_SETTINGS: [&str; 4] = [SECURITY, "802-1x", WIREGUARD, VPN];
const KEY_LENGTH: usize = 32;
const INTERFACE_LENGTH: usize = 15;
const SSID_LENGTH: usize = 32;
const PSK_LENGTH: (usize, usize) = (8, 63);
const PSK_HEX_LENGTH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Family {
    V4,
    V6,
}

impl Family {
    pub fn setting(self) -> &'static str {
        match self {
            Family::V4 => "ipv4",
            Family::V6 => "ipv6",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Family::V4 => "IPv4",
            Family::V6 => "IPv6",
        }
    }

    fn longest_prefix(self) -> u32 {
        match self {
            Family::V4 => 32,
            Family::V6 => 128,
        }
    }

    fn address(self, text: &str) -> Result<IpAddr, String> {
        text.parse::<IpAddr>()
            .ok()
            .filter(|address| address.is_ipv4() == (self == Family::V4))
            .ok_or_else(|| trf("%1 is not an %2 address", &[text, self.name()]))
    }

    fn network(self, text: &str) -> Result<(String, u32), String> {
        let (address, prefix) = match text.split_once('/') {
            Some((address, prefix)) => (
                address,
                prefix
                    .parse::<u32>()
                    .ok()
                    .filter(|prefix| *prefix <= self.longest_prefix())
                    .ok_or_else(|| trf("%1 has no valid prefix length", &[text]))?,
            ),
            None => (text, self.longest_prefix()),
        };
        Ok((self.address(address)?.to_string(), prefix))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Route {
    pub destination: String,
    pub prefix: u32,
    pub next_hop: String,
    pub metric: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ip {
    pub method: String,
    pub addresses: Vec<(String, u32)>,
    pub gateway: String,
    pub dns: Vec<String>,
    pub search: Vec<String>,
    pub automatic_dns: bool,
    pub automatic_routes: bool,
    pub never_default: bool,
    pub routes: Vec<Route>,
    pub privacy: i32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Peer {
    pub public_key: String,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub preshared_key: String,
    pub keepalive: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Security {
    Open,
    Personal,
    Wpa3,
    Other,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub settings: Settings,
}

pub fn items(text: &str) -> Vec<String> {
    text.split([',', ' ', '\n'])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_owned)
        .collect()
}

pub fn parse_addresses(text: &str, family: Family) -> Result<Vec<(String, u32)>, String> {
    items(text)
        .iter()
        .map(|item| family.network(item))
        .collect()
}

pub fn format_addresses(addresses: &[(String, u32)]) -> String {
    addresses
        .iter()
        .map(|(address, prefix)| format!("{address}/{prefix}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn parse_servers(text: &str, family: Family) -> Result<Vec<String>, String> {
    items(text)
        .iter()
        .map(|item| family.address(item).map(|address| address.to_string()))
        .collect()
}

pub fn parse_gateway(text: &str, family: Family) -> Result<String, String> {
    let text = text.trim();
    if text.is_empty() {
        return Ok(String::new());
    }
    family.address(text).map(|address| address.to_string())
}

pub fn parse_routes(text: &str, family: Family) -> Result<Vec<Route>, String> {
    text.split([',', '\n'])
        .map(str::trim)
        .filter(|route| !route.is_empty())
        .map(|route| {
            let mut words = route
                .split_whitespace()
                .filter(|word| !matches!(*word, "via" | "metric"));
            let (destination, prefix) = family.network(words.next().unwrap_or_default())?;
            let mut parsed = Route {
                destination,
                prefix,
                ..Route::default()
            };
            for word in words {
                if let Ok(metric) = word.parse::<u32>() {
                    parsed.metric = Some(metric);
                } else if parsed.next_hop.is_empty() {
                    parsed.next_hop = family.address(word)?.to_string();
                } else {
                    return Err(trf("%1 is not a route", &[route]));
                }
            }
            Ok(parsed)
        })
        .collect()
}

pub fn format_routes(routes: &[Route]) -> String {
    routes
        .iter()
        .map(|route| {
            let mut text = format!("{}/{}", route.destination, route.prefix);
            if !route.next_hop.is_empty() {
                text.push_str(&format!(" via {}", route.next_hop));
            }
            if let Some(metric) = route.metric {
                text.push_str(&format!(" metric {metric}"));
            }
            text
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn parse_networks(text: &str) -> Result<Vec<String>, String> {
    items(text)
        .iter()
        .map(|item| {
            let family = if item.contains(':') {
                Family::V6
            } else {
                Family::V4
            };
            family
                .network(item)
                .map(|(address, prefix)| format!("{address}/{prefix}"))
        })
        .collect()
}

pub fn parse_endpoint(text: &str) -> Result<String, String> {
    let text = text.trim();
    let fits = text.is_empty()
        || text
            .rsplit_once(':')
            .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok());
    if fits {
        Ok(text.to_owned())
    } else {
        Err(trf(
            "%1 is not a host and port, such as vpn.example.org:51820",
            &[text],
        ))
    }
}

pub fn parse_mac(text: &str) -> Result<String, String> {
    let text = text.trim();
    let named = matches!(text, "" | "preserve" | "permanent" | "random" | "stable");
    let parts: Vec<&str> = text.split(':').collect();
    let address = parts.len() == 6
        && parts.iter().all(|part| {
            part.len() == 2 && part.chars().all(|character| character.is_ascii_hexdigit())
        });
    if named || address {
        Ok(text.to_owned())
    } else {
        Err(trf(
            "%1 is neither a MAC address nor preserve, permanent, random or stable",
            &[text],
        ))
    }
}

pub fn free_name(base: &str, taken: &[String]) -> String {
    std::iter::once(base.to_owned())
        .chain((2..).map(|number| format!("{base} {number}")))
        .find(|name| !taken.contains(name))
        .unwrap_or_default()
}

pub fn free_interface(taken: &[String]) -> String {
    (0..)
        .map(|number| format!("wg{number}"))
        .find(|name| !taken.contains(name))
        .unwrap_or_default()
}

pub fn valid_key(key: &str) -> bool {
    let key = key.trim();
    !key.is_empty() && glib::base64_decode(key).len() == KEY_LENGTH
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
        variant
            .get::<Settings>()
            .map(|settings| Profile { settings })
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

    pub fn autoconnect(&self) -> bool {
        self.flag(CONNECTION, "autoconnect", true)
    }

    pub fn set_autoconnect(&mut self, on: bool) {
        self.put(CONNECTION, "autoconnect", on.to_variant());
    }

    pub fn all_users(&self) -> bool {
        self.strings(CONNECTION, "permissions").is_empty()
    }

    pub fn set_all_users(&mut self, all: bool, user: &str) {
        let permissions = if all {
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

    pub fn ssid(&self) -> String {
        self.get(WIRELESS, "ssid")
            .and_then(Variant::get::<Vec<u8>>)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    }

    pub fn set_ssid(&mut self, ssid: &str) {
        self.put(WIRELESS, "ssid", ssid.as_bytes().to_variant());
    }

    pub fn hidden(&self) -> bool {
        self.flag(WIRELESS, "hidden", false)
    }

    pub fn set_hidden(&mut self, hidden: bool) {
        self.put(WIRELESS, "hidden", hidden.to_variant());
    }

    pub fn security(&self) -> Security {
        if !self.settings.contains_key(SECURITY) {
            return Security::Open;
        }
        match self.string(SECURITY, "key-mgmt").as_str() {
            "wpa-psk" => Security::Personal,
            "sae" => Security::Wpa3,
            _ => Security::Other,
        }
    }

    pub fn set_security(&mut self, security: Security) {
        let key_management = match security {
            Security::Open => {
                self.settings.remove(SECURITY);
                self.remove(WIRELESS, "security");
                return;
            }
            Security::Personal => "wpa-psk",
            Security::Wpa3 => "sae",
            Security::Other => return,
        };
        self.put(SECURITY, "key-mgmt", key_management.to_variant());
    }

    pub fn psk(&self) -> String {
        self.string(SECURITY, "psk")
    }

    pub fn set_psk(&mut self, psk: &str) {
        self.set_string(SECURITY, "psk", psk);
    }

    pub fn private_key(&self) -> String {
        self.string(WIREGUARD, "private-key")
    }

    pub fn set_private_key(&mut self, key: &str) {
        self.set_string(WIREGUARD, "private-key", key.trim());
    }

    pub fn listen_port(&self) -> u32 {
        self.get(WIREGUARD, "listen-port")
            .and_then(Variant::get::<u32>)
            .unwrap_or(0)
    }

    pub fn set_listen_port(&mut self, port: u32) {
        self.put(WIREGUARD, "listen-port", port.to_variant());
    }

    pub fn peers(&self) -> Vec<Peer> {
        self.entries(WIREGUARD, "peers")
            .iter()
            .map(|peer| Peer {
                public_key: entry_string(peer, "public-key"),
                endpoint: entry_string(peer, "endpoint"),
                allowed_ips: peer
                    .get("allowed-ips")
                    .and_then(Variant::get::<Vec<String>>)
                    .unwrap_or_default(),
                preshared_key: entry_string(peer, "preshared-key"),
                keepalive: entry_u32(peer, "persistent-keepalive").unwrap_or(0),
            })
            .collect()
    }

    pub fn set_peers(&mut self, peers: &[Peer]) {
        let entries: Vec<Entry> = peers
            .iter()
            .map(|peer| {
                let mut entry = Entry::new();
                entry.insert("public-key".into(), peer.public_key.trim().to_variant());
                if !peer.endpoint.is_empty() {
                    entry.insert("endpoint".into(), peer.endpoint.to_variant());
                }
                entry.insert("allowed-ips".into(), peer.allowed_ips.to_variant());
                if !peer.preshared_key.is_empty() {
                    entry.insert("preshared-key".into(), peer.preshared_key.to_variant());
                    entry.insert("preshared-key-flags".into(), 0u32.to_variant());
                }
                if peer.keepalive > 0 {
                    entry.insert("persistent-keepalive".into(), peer.keepalive.to_variant());
                }
                entry
            })
            .collect();
        self.put(WIREGUARD, "peers", entries.to_variant());
    }

    pub fn vpn_service(&self) -> String {
        self.string(VPN, "service-type")
    }

    pub fn vpn_data(&self) -> Vec<(String, String)> {
        self.get(VPN, "data")
            .and_then(Variant::get::<BTreeMap<String, String>>)
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    pub fn set_vpn_data(&mut self, key: &str, value: &str) {
        let mut data: BTreeMap<String, String> = self.vpn_data().into_iter().collect();
        if value.is_empty() {
            data.remove(key);
        } else {
            data.insert(key.to_owned(), value.to_owned());
        }
        self.put(VPN, "data", data.to_variant());
    }

    pub fn ip(&self, family: Family) -> Ip {
        let setting = family.setting();
        let dns_data = self.strings(setting, "dns-data");
        let dns = if !dns_data.is_empty() {
            dns_data
        } else {
            match family {
                Family::V4 => self
                    .get(setting, "dns")
                    .and_then(Variant::get::<Vec<u32>>)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|raw| Ipv4Addr::from(raw.to_ne_bytes()).to_string())
                    .collect(),
                Family::V6 => self
                    .get(setting, "dns")
                    .and_then(Variant::get::<Vec<Vec<u8>>>)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|raw| <[u8; 16]>::try_from(raw).ok())
                    .map(|raw| Ipv6Addr::from(raw).to_string())
                    .collect(),
            }
        };
        Ip {
            method: match self.string(setting, "method") {
                method if method.is_empty() => "auto".to_owned(),
                method => method,
            },
            addresses: self
                .entries(setting, "address-data")
                .iter()
                .map(|entry| {
                    (
                        entry_string(entry, "address"),
                        entry_u32(entry, "prefix").unwrap_or(family.longest_prefix()),
                    )
                })
                .collect(),
            gateway: self.string(setting, "gateway"),
            dns,
            search: self.strings(setting, "dns-search"),
            automatic_dns: !self.flag(setting, "ignore-auto-dns", false),
            automatic_routes: !self.flag(setting, "ignore-auto-routes", false),
            never_default: self.flag(setting, "never-default", false),
            routes: self
                .entries(setting, "route-data")
                .iter()
                .map(|entry| Route {
                    destination: entry_string(entry, "dest"),
                    prefix: entry_u32(entry, "prefix").unwrap_or(family.longest_prefix()),
                    next_hop: entry_string(entry, "next-hop"),
                    metric: entry_u32(entry, "metric"),
                })
                .collect(),
            privacy: self
                .get(setting, "ip6-privacy")
                .and_then(Variant::get::<i32>)
                .unwrap_or(-1),
        }
    }

    pub fn set_ip(&mut self, family: Family, ip: &Ip) {
        let setting = family.setting();
        let configured = !matches!(ip.method.as_str(), "disabled" | "ignore" | "link-local");
        self.put(setting, "method", ip.method.to_variant());
        let addresses: Vec<Entry> = if configured {
            ip.addresses
                .iter()
                .map(|(address, prefix)| {
                    Entry::from([
                        ("address".to_owned(), address.to_variant()),
                        ("prefix".to_owned(), prefix.to_variant()),
                    ])
                })
                .collect()
        } else {
            Vec::new()
        };
        let gateway = if addresses.is_empty() {
            ""
        } else {
            ip.gateway.as_str()
        };
        self.put(setting, "address-data", addresses.to_variant());
        self.remove(setting, "addresses");
        self.set_string(setting, "gateway", gateway);
        let plain = ip.dns.iter().all(|server| family.address(server).is_ok());
        if plain {
            self.remove(setting, "dns-data");
            let servers = match family {
                Family::V4 => ip
                    .dns
                    .iter()
                    .filter_map(|server| server.parse::<Ipv4Addr>().ok())
                    .map(|server| u32::from_ne_bytes(server.octets()))
                    .collect::<Vec<u32>>()
                    .to_variant(),
                Family::V6 => ip
                    .dns
                    .iter()
                    .filter_map(|server| server.parse::<Ipv6Addr>().ok())
                    .map(|server| server.octets().to_vec())
                    .collect::<Vec<Vec<u8>>>()
                    .to_variant(),
            };
            self.put(setting, "dns", servers);
        } else {
            self.remove(setting, "dns");
            self.put(setting, "dns-data", ip.dns.to_variant());
        }
        self.put(setting, "dns-search", ip.search.to_variant());
        self.put(setting, "ignore-auto-dns", (!ip.automatic_dns).to_variant());
        self.put(
            setting,
            "ignore-auto-routes",
            (!ip.automatic_routes).to_variant(),
        );
        self.put(setting, "never-default", ip.never_default.to_variant());
        let routes: Vec<Entry> = if configured {
            ip.routes
                .iter()
                .map(|route| {
                    let mut entry = Entry::from([
                        ("dest".to_owned(), route.destination.to_variant()),
                        ("prefix".to_owned(), route.prefix.to_variant()),
                    ]);
                    if !route.next_hop.is_empty() {
                        entry.insert("next-hop".into(), route.next_hop.to_variant());
                    }
                    if let Some(metric) = route.metric {
                        entry.insert("metric".into(), metric.to_variant());
                    }
                    entry
                })
                .collect()
        } else {
            Vec::new()
        };
        self.put(setting, "route-data", routes.to_variant());
        self.remove(setting, "routes");
        if family == Family::V6 {
            self.put(setting, "ip6-privacy", ip.privacy.to_variant());
        }
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

    pub fn merge_secrets(&mut self, secrets: &Settings, overwrite: bool) {
        for (setting, values) in secrets {
            for (key, value) in values {
                if setting == WIREGUARD && key == "peers" {
                    self.merge_peer_secrets(value, overwrite);
                } else if overwrite || self.get(setting, key).is_none() {
                    self.put(setting, key, value.clone());
                }
            }
        }
    }

    fn merge_peer_secrets(&mut self, secret_peers: &Variant, overwrite: bool) {
        let Some(secret_peers) = secret_peers.get::<Vec<Entry>>() else {
            return;
        };
        let mut peers = self.entries(WIREGUARD, "peers");
        for secret in &secret_peers {
            let key = entry_string(secret, "public-key");
            if let Some(peer) = peers
                .iter_mut()
                .find(|peer| entry_string(peer, "public-key") == key)
            {
                for (name, value) in secret {
                    if overwrite || !peer.contains_key(name) {
                        peer.insert(name.clone(), value.clone());
                    }
                }
            }
        }
        self.put(WIREGUARD, "peers", peers.to_variant());
    }

    pub fn problem(&self) -> Option<String> {
        if self.id().trim().is_empty() {
            return Some(tr("The connection needs a name"));
        }
        let kind = self.kind();
        let interface = self.interface();
        if kind == WIREGUARD && interface.is_empty() {
            return Some(tr("A WireGuard connection needs an interface name"));
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
            let psk_fits = (PSK_LENGTH.0..=PSK_LENGTH.1).contains(&psk.len())
                || (psk.len() == PSK_HEX_LENGTH
                    && psk.chars().all(|character| character.is_ascii_hexdigit()));
            match self.security() {
                Security::Personal if !psk_fits => {
                    return Some(tr("A WPA password has 8 to 63 characters"));
                }
                Security::Wpa3 if psk.is_empty() => {
                    return Some(tr("A WPA3 network needs a password"));
                }
                _ => {}
            }
        }
        if kind == WIREGUARD {
            if !valid_key(&self.private_key()) {
                return Some(tr("The private key is not a WireGuard key"));
            }
            for peer in self.peers() {
                if !valid_key(&peer.public_key) {
                    return Some(trf("%1 is not a WireGuard public key", &[&peer.public_key]));
                }
                if !peer.preshared_key.is_empty() && !valid_key(&peer.preshared_key) {
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

    fn route(destination: &str, prefix: u32, next_hop: &str, metric: Option<u32>) -> Route {
        Route {
            destination: destination.to_owned(),
            prefix,
            next_hop: next_hop.to_owned(),
            metric,
        }
    }

    #[test]
    fn addresses_take_the_family_and_a_full_prefix_when_none_is_given() {
        assert_eq!(
            parse_addresses("192.168.1.10/24, 10.0.0.2", Family::V4),
            Ok(vec![
                ("192.168.1.10".to_owned(), 24),
                ("10.0.0.2".to_owned(), 32)
            ])
        );
        assert_eq!(
            parse_addresses("fd00::1/64", Family::V6),
            Ok(vec![("fd00::1".to_owned(), 64)])
        );
        assert!(parse_addresses("fd00::1/64", Family::V4).is_err());
        assert!(parse_addresses("10.0.0.1/33", Family::V4).is_err());
        assert_eq!(
            format_addresses(&[("10.0.0.2".to_owned(), 8), ("10.0.0.3".to_owned(), 32)]),
            "10.0.0.2/8, 10.0.0.3/32"
        );
    }

    #[test]
    fn peer_fields_take_either_family_a_port_and_a_mac_or_its_names() {
        assert_eq!(
            parse_networks("0.0.0.0/0, ::/0, 10.1.0.5"),
            Ok(vec![
                "0.0.0.0/0".to_owned(),
                "::/0".to_owned(),
                "10.1.0.5/32".to_owned()
            ])
        );
        assert!(parse_networks("10.0.0.0/40").is_err());
        assert!(parse_endpoint("vpn.example.org:51820").is_ok());
        assert!(parse_endpoint("[fd00::1]:51820").is_ok());
        assert!(parse_endpoint("").is_ok());
        assert!(parse_endpoint("vpn.example.org").is_err());
        assert!(parse_mac("random").is_ok());
        assert!(parse_mac("AA:bb:01:23:45:67").is_ok());
        assert!(parse_mac("AA:bb:01:23:45").is_err());
    }

    #[test]
    fn a_new_profile_takes_the_first_free_name_and_interface() {
        let taken = |names: &[&str]| {
            names
                .iter()
                .map(|name| (*name).to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(free_name("WireGuard", &taken(&["Office"])), "WireGuard");
        assert_eq!(
            free_name("WireGuard", &taken(&["WireGuard", "WireGuard 2"])),
            "WireGuard 3"
        );
        assert_eq!(free_interface(&taken(&["wg0", "eth0", "wg2"])), "wg1");
    }

    #[test]
    fn routes_read_like_ip_route_with_or_without_the_keywords() {
        assert_eq!(
            parse_routes(
                "10.0.0.0/8 via 192.168.1.1 metric 100, 172.16.0.0/12 192.168.1.2, 0.0.0.0/0",
                Family::V4
            ),
            Ok(vec![
                route("10.0.0.0", 8, "192.168.1.1", Some(100)),
                route("172.16.0.0", 12, "192.168.1.2", None),
                route("0.0.0.0", 0, "", None),
            ])
        );
        assert!(parse_routes("10.0.0.0/8 via nowhere", Family::V4).is_err());
        assert_eq!(
            format_routes(&[
                route("10.0.0.0", 8, "192.168.1.1", Some(100)),
                route("fd00::", 8, "", None)
            ]),
            "10.0.0.0/8 via 192.168.1.1 metric 100, fd00::/8"
        );
    }

    #[test]
    fn ip_settings_survive_a_write_and_drop_the_legacy_keys() {
        let mut profile = Profile::new(WIRED, "Office");
        profile.put("ipv4", "addresses", Vec::<Vec<u32>>::new().to_variant());
        profile.put("ipv4", "dns-data", vec!["1.1.1.1".to_owned()].to_variant());
        let written = Ip {
            method: "manual".to_owned(),
            addresses: vec![("192.168.1.10".to_owned(), 24)],
            gateway: "192.168.1.1".to_owned(),
            dns: vec!["9.9.9.9".to_owned(), "1.0.0.1".to_owned()],
            search: vec!["lan".to_owned()],
            automatic_dns: false,
            automatic_routes: true,
            never_default: false,
            routes: vec![route("10.0.0.0", 8, "192.168.1.2", Some(5))],
            privacy: -1,
        };
        profile.set_ip(Family::V4, &written);
        assert_eq!(profile.ip(Family::V4), written);
        assert!(profile.get("ipv4", "addresses").is_none());
        assert!(profile.get("ipv4", "dns-data").is_none());
        assert_eq!(
            profile
                .get("ipv4", "dns")
                .and_then(Variant::get::<Vec<u32>>),
            Some(vec![
                u32::from_ne_bytes([9, 9, 9, 9]),
                u32::from_ne_bytes([1, 0, 0, 1])
            ])
        );

        let v6 = Ip {
            method: "auto".to_owned(),
            dns: vec!["2606:4700:4700::1111".to_owned()],
            automatic_dns: true,
            automatic_routes: true,
            privacy: 2,
            ..Ip::default()
        };
        profile.set_ip(Family::V6, &v6);
        assert_eq!(profile.ip(Family::V6), v6);
        assert_eq!(profile.normalized(), profile);

        let encrypted = Ip {
            dns: vec!["1.1.1.1#cloudflare-dns.com".to_owned()],
            ..written
        };
        profile.set_ip(Family::V4, &encrypted);
        assert_eq!(profile.ip(Family::V4).dns, encrypted.dns);
        assert!(profile.get("ipv4", "dns").is_none());
    }

    #[test]
    fn a_method_without_addresses_writes_none_and_no_gateway() {
        let mut profile = Profile::new(WIRED, "Office");
        profile.set_ip(
            Family::V4,
            &Ip {
                method: "disabled".to_owned(),
                addresses: vec![("10.0.0.2".to_owned(), 8)],
                gateway: "10.0.0.1".to_owned(),
                ..Ip::default()
            },
        );
        let read = profile.ip(Family::V4);
        assert!(read.addresses.is_empty());
        assert!(read.gateway.is_empty());
    }

    #[test]
    fn peer_secrets_join_the_peer_with_the_same_public_key() {
        let key = |byte: u8| glib::base64_encode(&[byte; 32]).to_string();
        let mut profile = Profile::new(WIREGUARD, "Tunnel");
        profile.set_peers(&[
            Peer {
                public_key: key(1),
                allowed_ips: vec!["0.0.0.0/0".to_owned()],
                ..Peer::default()
            },
            Peer {
                public_key: key(2),
                endpoint: "vpn.example.org:51820".to_owned(),
                ..Peer::default()
            },
        ]);
        let secret_peer = Entry::from([
            ("public-key".to_owned(), key(2).to_variant()),
            ("preshared-key".to_owned(), key(3).to_variant()),
        ]);
        let secrets = Settings::from([(
            WIREGUARD.to_owned(),
            HashMap::from([
                ("private-key".to_owned(), key(4).to_variant()),
                ("peers".to_owned(), vec![secret_peer].to_variant()),
            ]),
        )]);
        let mut typed = profile.clone();
        typed.set_private_key(&key(5));
        profile.merge_secrets(&secrets, true);
        assert_eq!(profile.private_key(), key(4));
        let peers = profile.peers();
        assert_eq!(peers[0].preshared_key, "");
        assert_eq!(peers[1].preshared_key, key(3));
        assert_eq!(peers[1].endpoint, "vpn.example.org:51820");
        typed.merge_secrets(&secrets, false);
        assert_eq!(typed.private_key(), key(5));
        assert_eq!(typed.peers()[1].preshared_key, key(3));
        profile.set_interface("wg0");
        assert_eq!(
            profile.problem(),
            Some(trf("Manual %1 needs at least one address", &["IPv4"]))
        );
    }

    #[test]
    fn wireless_security_and_ssid_are_read_back() {
        let mut profile = Profile::new(WIRELESS, "Home");
        profile.set_ssid("Home");
        assert_eq!(profile.security(), Security::Open);
        assert_eq!(profile.problem(), None);
        profile.set_security(Security::Personal);
        profile.set_psk("short");
        assert_eq!(profile.security(), Security::Personal);
        assert_eq!(
            profile.problem(),
            Some(tr("A WPA password has 8 to 63 characters"))
        );
        profile.set_psk("long enough");
        assert_eq!(profile.problem(), None);
        profile.set_security(Security::Open);
        assert_eq!(profile.security(), Security::Open);
        assert_eq!(profile.ssid(), "Home");
    }
}
