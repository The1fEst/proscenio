use gtk4::glib::Variant;
use gtk4::prelude::*;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use super::{Entry, Profile, entry_string, entry_u32, items};
use crate::core::i18n::trf;

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

    pub fn network(self, text: &str) -> Result<(String, u32), String> {
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
    pub uses_automatic_dns: bool,
    pub uses_automatic_routes: bool,
    pub is_never_default: bool,
    pub routes: Vec<Route>,
    pub privacy: i32,
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
        .filter(|route_text| !route_text.is_empty())
        .map(|route_text| {
            let mut words = route_text
                .split_whitespace()
                .filter(|word| !matches!(*word, "via" | "metric"));
            let (destination, prefix) = family.network(words.next().unwrap_or_default())?;
            let mut route = Route {
                destination,
                prefix,
                ..Route::default()
            };
            for word in words {
                if let Ok(metric) = word.parse::<u32>() {
                    route.metric = Some(metric);
                } else if route.next_hop.is_empty() {
                    route.next_hop = family.address(word)?.to_string();
                } else {
                    return Err(trf("%1 is not a route", &[route_text]));
                }
            }
            Ok(route)
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

impl Profile {
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
                    .map(|address| Ipv4Addr::from(address.to_ne_bytes()).to_string())
                    .collect(),
                Family::V6 => self
                    .get(setting, "dns")
                    .and_then(Variant::get::<Vec<Vec<u8>>>)
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|bytes| <[u8; 16]>::try_from(bytes).ok())
                    .map(|octets| Ipv6Addr::from(octets).to_string())
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
            uses_automatic_dns: !self.flag(setting, "ignore-auto-dns", false),
            uses_automatic_routes: !self.flag(setting, "ignore-auto-routes", false),
            is_never_default: self.flag(setting, "never-default", false),
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
        let is_configured = !matches!(ip.method.as_str(), "disabled" | "ignore" | "link-local");
        self.put(setting, "method", ip.method.to_variant());
        let addresses: Vec<Entry> = if is_configured {
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
        let are_plain_addresses = ip.dns.iter().all(|server| family.address(server).is_ok());
        if are_plain_addresses {
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
        self.put(
            setting,
            "ignore-auto-dns",
            (!ip.uses_automatic_dns).to_variant(),
        );
        self.put(
            setting,
            "ignore-auto-routes",
            (!ip.uses_automatic_routes).to_variant(),
        );
        self.put(setting, "never-default", ip.is_never_default.to_variant());
        let routes: Vec<Entry> = if is_configured {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::network::nmprofile::WIRED;

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
        let manual_ip = Ip {
            method: "manual".to_owned(),
            addresses: vec![("192.168.1.10".to_owned(), 24)],
            gateway: "192.168.1.1".to_owned(),
            dns: vec!["9.9.9.9".to_owned(), "1.0.0.1".to_owned()],
            search: vec!["lan".to_owned()],
            uses_automatic_dns: false,
            uses_automatic_routes: true,
            is_never_default: false,
            routes: vec![route("10.0.0.0", 8, "192.168.1.2", Some(5))],
            privacy: -1,
        };
        profile.set_ip(Family::V4, &manual_ip);
        assert_eq!(profile.ip(Family::V4), manual_ip);
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
            uses_automatic_dns: true,
            uses_automatic_routes: true,
            privacy: 2,
            ..Ip::default()
        };
        profile.set_ip(Family::V6, &v6);
        assert_eq!(profile.ip(Family::V6), v6);
        assert_eq!(profile.normalized(), profile);

        let encrypted_dns_ip = Ip {
            dns: vec!["1.1.1.1#cloudflare-dns.com".to_owned()],
            ..manual_ip
        };
        profile.set_ip(Family::V4, &encrypted_dns_ip);
        assert_eq!(profile.ip(Family::V4).dns, encrypted_dns_ip.dns);
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
        let ip = profile.ip(Family::V4);
        assert!(ip.addresses.is_empty());
        assert!(ip.gateway.is_empty());
    }
}
