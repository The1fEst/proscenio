use gtk4::glib::{self, Variant};
use gtk4::prelude::*;

use super::ipconfig::Family;
use super::{Entry, Profile, SECRET_FLAG_AGENT_OWNED, WIREGUARD, entry_string, entry_u32, items};
use crate::core::i18n::trf;

const KEY_LENGTH: usize = 32;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Peer {
    pub public_key: String,
    pub endpoint: String,
    pub allowed_ips: Vec<String>,
    pub preshared_key: String,
    pub preshared_key_flags: Option<u32>,
    pub keepalive: u32,
    pub stored_public_key: Option<String>,
}

impl Peer {
    pub fn set_preshared_key(&mut self, key: String, are_secrets_read: bool) {
        if key.is_empty() && are_secrets_read && !self.is_preshared_key_agent_owned() {
            self.preshared_key_flags = None;
        }
        self.preshared_key = key;
    }

    pub fn is_preshared_key_agent_owned(&self) -> bool {
        self.preshared_key_flags
            .is_some_and(|flags| flags & SECRET_FLAG_AGENT_OWNED != 0)
    }
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
    let is_valid = text.is_empty()
        || text
            .rsplit_once(':')
            .is_some_and(|(host, port)| !host.is_empty() && port.parse::<u16>().is_ok());
    if is_valid {
        Ok(text.to_owned())
    } else {
        Err(trf(
            "%1 is not a host and port, such as vpn.example.org:51820",
            &[text],
        ))
    }
}

pub fn is_valid_key(key: &str) -> bool {
    let key = key.trim();
    !key.is_empty() && glib::base64_decode(key).len() == KEY_LENGTH
}

impl Profile {
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
            .enumerate()
            .map(|(index, peer)| Peer {
                stored_public_key: self
                    .stored_peer_keys
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| Some(entry_string(peer, "public-key"))),
                public_key: entry_string(peer, "public-key"),
                endpoint: entry_string(peer, "endpoint"),
                allowed_ips: peer
                    .get("allowed-ips")
                    .and_then(Variant::get::<Vec<String>>)
                    .unwrap_or_default(),
                preshared_key: entry_string(peer, "preshared-key"),
                preshared_key_flags: entry_u32(peer, "preshared-key-flags"),
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
                    let flags = peer.preshared_key_flags.unwrap_or(0);
                    entry.insert("preshared-key-flags".into(), flags.to_variant());
                } else if let Some(flags) = peer.preshared_key_flags {
                    entry.insert("preshared-key-flags".into(), flags.to_variant());
                }
                if peer.keepalive > 0 {
                    entry.insert("persistent-keepalive".into(), peer.keepalive.to_variant());
                }
                entry
            })
            .collect();
        self.put(WIREGUARD, "peers", entries.to_variant());
        self.stored_peer_keys = peers
            .iter()
            .map(|peer| peer.stored_public_key.clone())
            .collect();
    }

    pub fn has_changed_peer_with_stored_key(&self) -> bool {
        self.peers().iter().any(|peer| {
            let is_public_key_changed = peer
                .stored_public_key
                .as_ref()
                .is_some_and(|key| *key != peer.public_key);
            let is_preshared_key_stored = peer.preshared_key_flags == Some(0);
            is_public_key_changed && peer.preshared_key.is_empty() && is_preshared_key_stored
        })
    }

    pub fn reset_stored_peer_keys(&mut self) {
        self.stored_peer_keys.clear();
    }

    pub fn rebase_stored_peer_keys(&mut self, sent_profile: &Profile) {
        let sent_peers = sent_profile.peers();
        self.stored_peer_keys = self
            .peers()
            .into_iter()
            .map(|peer| {
                sent_peers
                    .iter()
                    .find(|sent_peer| match &peer.stored_public_key {
                        Some(key) => sent_peer.stored_public_key.as_ref() == Some(key),
                        None => {
                            sent_peer.stored_public_key.is_none()
                                && sent_peer.public_key == peer.public_key
                        }
                    })
                    .map(|sent_peer| sent_peer.public_key.clone())
            })
            .collect();
    }

    pub(super) fn merge_peer_secrets(&mut self, secret_peers: &Variant, should_overwrite: bool) {
        let Some(secret_peers) = secret_peers.get::<Vec<Entry>>() else {
            return;
        };
        let mut peers = self.entries(WIREGUARD, "peers");
        let stored_keys: Vec<Option<String>> = self
            .peers()
            .into_iter()
            .map(|peer| peer.stored_public_key)
            .collect();
        for secret in &secret_peers {
            let key = entry_string(secret, "public-key");
            let Some(index) = stored_keys
                .iter()
                .position(|stored_key| stored_key.as_ref() == Some(&key))
            else {
                continue;
            };
            let peer = &mut peers[index];
            for (name, value) in secret {
                if name != "public-key" && (should_overwrite || !peer.contains_key(name)) {
                    peer.insert(name.clone(), value.clone());
                }
            }
        }
        self.put(WIREGUARD, "peers", peers.to_variant());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::network::nmprofile::{Settings, parse_mac};
    use std::collections::HashMap;

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
        profile.reset_stored_peer_keys();
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
        let mut typed_profile = profile.clone();
        typed_profile.set_private_key(&key(5));
        profile.merge_secrets(&secrets, true);
        assert_eq!(profile.private_key(), key(4));
        let peers = profile.peers();
        assert_eq!(peers[0].preshared_key, "");
        assert_eq!(peers[1].preshared_key, key(3));
        assert_eq!(peers[1].endpoint, "vpn.example.org:51820");
        typed_profile.merge_secrets(&secrets, false);
        assert_eq!(typed_profile.private_key(), key(5));
        assert_eq!(typed_profile.peers()[1].preshared_key, key(3));
        profile.set_interface("wg0");
        assert_eq!(
            profile.problem(false),
            Some(trf("Manual %1 needs at least one address", &["IPv4"]))
        );
    }

    #[test]
    fn peer_edits_keep_where_each_preshared_key_is_stored() {
        let key = |byte: u8| glib::base64_encode(&[byte; 32]).to_string();
        let mut tunnel = Profile::new(WIREGUARD, "Tunnel");
        let peer_entry = |public_key: String, preshared_key: Option<String>, flags: u32| {
            let mut entry = Entry::from([
                ("public-key".to_owned(), public_key.to_variant()),
                ("preshared-key-flags".to_owned(), flags.to_variant()),
            ]);
            if let Some(preshared_key) = preshared_key {
                entry.insert("preshared-key".to_owned(), preshared_key.to_variant());
            }
            entry
        };
        let entries = vec![
            peer_entry(key(1), None, 1),
            peer_entry(key(2), Some(key(3)), 1),
            peer_entry(key(4), None, 4),
            peer_entry(key(8), Some(key(9)), 0),
        ];
        tunnel.put(WIREGUARD, "peers", entries.to_variant());
        let mut peers = tunnel.peers();
        peers[0].set_preshared_key(String::new(), true);
        peers[1].set_preshared_key(key(5), true);
        peers[3].set_preshared_key(String::new(), true);
        peers.push(Peer {
            public_key: key(6),
            preshared_key: key(7),
            ..Peer::default()
        });
        tunnel.set_peers(&peers);
        let flags: Vec<Option<u32>> = tunnel
            .entries(WIREGUARD, "peers")
            .iter()
            .map(|entry| entry_u32(entry, "preshared-key-flags"))
            .collect();
        assert_eq!(flags, [Some(1), Some(1), Some(4), None, Some(0)]);
        assert_eq!(tunnel.peers()[1].preshared_key, key(5));
    }

    #[test]
    fn a_peer_with_a_changed_public_key_keeps_its_stored_preshared_key() {
        let key = |byte: u8| glib::base64_encode(&[byte; 32]).to_string();
        let mut tunnel = Profile::new(WIREGUARD, "Tunnel");
        let entries = vec![Entry::from([
            ("public-key".to_owned(), key(1).to_variant()),
            ("preshared-key-flags".to_owned(), 0u32.to_variant()),
        ])];
        tunnel.put(WIREGUARD, "peers", entries.to_variant());
        assert!(!tunnel.has_changed_peer_with_stored_key());
        let mut peers = tunnel.peers();
        peers[0].public_key = key(2);
        peers.push(Peer {
            public_key: key(1),
            ..Peer::default()
        });
        tunnel.set_peers(&peers);
        assert!(tunnel.has_changed_peer_with_stored_key());
        for flags in [Some(1), Some(2), Some(4), None] {
            let mut unstored_tunnel = tunnel.clone();
            let mut unstored_peers = unstored_tunnel.peers();
            unstored_peers[0].preshared_key_flags = flags;
            unstored_tunnel.set_peers(&unstored_peers);
            assert!(!unstored_tunnel.has_changed_peer_with_stored_key());
        }
        let stored_peers = vec![Entry::from([
            ("public-key".to_owned(), key(1).to_variant()),
            ("preshared-key".to_owned(), key(3).to_variant()),
        ])];
        let secrets = Settings::from([(
            WIREGUARD.to_owned(),
            HashMap::from([("peers".to_owned(), stored_peers.to_variant())]),
        )]);
        tunnel.merge_secrets(&secrets, false);
        let to_key_pair = |peer: &Peer| (peer.public_key.clone(), peer.preshared_key.clone());
        let pairs: Vec<(String, String)> = tunnel.peers().iter().map(to_key_pair).collect();
        assert_eq!(pairs, [(key(2), key(3)), (key(1), String::new())]);
        assert!(!tunnel.has_changed_peer_with_stored_key());
        let sent_tunnel = tunnel.clone();
        let mut peers = tunnel.peers();
        peers[0].public_key = key(4);
        tunnel.set_peers(&peers);
        tunnel.rebase_stored_peer_keys(&sent_tunnel);
        let stored_keys: Vec<Option<String>> = tunnel
            .peers()
            .into_iter()
            .map(|peer| peer.stored_public_key)
            .collect();
        assert_eq!(stored_keys, [Some(key(2)), Some(key(1))]);
    }

    #[test]
    fn an_erased_preshared_key_keeps_its_flags_while_the_stored_one_is_unread() {
        let key = |byte: u8| glib::base64_encode(&[byte; 32]).to_string();
        let stored_peers = vec![Entry::from([
            ("public-key".to_owned(), key(1).to_variant()),
            ("preshared-key".to_owned(), key(2).to_variant()),
        ])];
        let secrets = Settings::from([(
            WIREGUARD.to_owned(),
            HashMap::from([("peers".to_owned(), stored_peers.to_variant())]),
        )]);
        for flags in [0u32, 1, 2, 4] {
            for are_secrets_read in [false, true] {
                let mut tunnel = Profile::new(WIREGUARD, "Tunnel");
                tunnel.set_peers(&[Peer {
                    public_key: key(1),
                    preshared_key_flags: Some(flags),
                    ..Peer::default()
                }]);
                tunnel.reset_stored_peer_keys();
                let mut peers = tunnel.peers();
                peers[0].set_preshared_key(key(3), are_secrets_read);
                peers[0].set_preshared_key(String::new(), are_secrets_read);
                tunnel.set_peers(&peers);
                let is_kept = !are_secrets_read || flags & SECRET_FLAG_AGENT_OWNED != 0;
                assert_eq!(
                    tunnel.peers()[0].preshared_key_flags,
                    is_kept.then_some(flags)
                );
                if !are_secrets_read {
                    tunnel.merge_secrets(&secrets, false);
                    let peer = &tunnel.peers()[0];
                    assert_eq!(
                        (peer.preshared_key.as_str(), peer.preshared_key_flags),
                        (key(2).as_str(), Some(flags))
                    );
                }
            }
        }
    }
}
