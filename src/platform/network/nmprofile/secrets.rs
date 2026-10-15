use gtk4::glib::Variant;

use super::wireless::Security;
use super::{Profile, SECRET_FLAG_AGENT_OWNED, SECURITY, Settings, VPN, WIREGUARD};

impl Profile {
    pub fn merge_secrets(&mut self, secrets: &Settings, should_overwrite: bool) {
        for (setting, values) in secrets {
            for (key, value) in values {
                if setting == WIREGUARD && key == "peers" {
                    self.merge_peer_secrets(value, should_overwrite);
                } else if setting == VPN && key == "secrets" {
                    self.merge_vpn_secrets(value, should_overwrite);
                } else if should_overwrite || self.get(setting, key).is_none() {
                    self.put(setting, key, value.clone());
                }
            }
        }
    }

    fn keeps_secret(&self, are_secrets_read: bool) -> bool {
        let Some(flags) = self.secret_flags() else {
            return false;
        };
        !are_secrets_read || flags != 0
    }

    pub fn keeps_secret_of(&self, saved_profile: &Profile, are_secrets_read: bool) -> bool {
        saved_profile.keeps_secret(are_secrets_read)
            && self.secret_flags() == saved_profile.secret_flags()
    }

    fn secret_flags(&self) -> Option<u32> {
        let (setting, flags) = if self.kind() == WIREGUARD {
            (WIREGUARD, "private-key-flags")
        } else if matches!(self.security(), Security::Personal | Security::Wpa3) {
            (SECURITY, "psk-flags")
        } else {
            return None;
        };
        Some(
            self.get(setting, flags)
                .and_then(Variant::get::<u32>)
                .unwrap_or(0),
        )
    }

    pub fn is_agent_owned(&self, setting: &str, flags_key: &str) -> bool {
        self.get(setting, flags_key)
            .and_then(Variant::get::<u32>)
            .is_some_and(|flags| flags & SECRET_FLAG_AGENT_OWNED != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::i18n::tr;
    use crate::platform::network::nmprofile::WIRELESS;
    use crate::platform::network::nmprofile::wireguard::Peer;
    use gtk4::prelude::*;

    #[test]
    fn an_unread_stored_password_counts_as_kept_until_one_is_typed() {
        let mut profile = Profile::new(WIRELESS, "Home");
        profile.set_ssid("Home");
        profile.set_security(Security::Wep);
        assert!(!profile.keeps_secret(false));
        for security in [Security::Personal, Security::Wpa3] {
            profile.set_security(security);
            assert!(profile.keeps_secret(false));
            assert!(!profile.keeps_secret(true));
            assert_eq!(profile.problem(true), None);
            assert!(profile.problem(false).is_some());
        }
        profile.put(SECURITY, "psk-flags", 1u32.to_variant());
        assert!(profile.keeps_secret(true));
        profile.set_security(Security::Personal);
        profile.set_psk("short");
        assert_eq!(
            profile.problem(true),
            Some(tr("A WPA password has 8 to 63 characters"))
        );
    }

    #[test]
    fn a_password_held_by_an_agent_is_kept_only_while_the_draft_still_hands_it_over() {
        let mut saved_profile = Profile::new(WIRELESS, "Home");
        saved_profile.set_ssid("Home");
        saved_profile.set_security(Security::Personal);
        saved_profile.put(SECURITY, "psk-flags", 1u32.to_variant());
        let mut draft = saved_profile.clone();
        draft.set_ssid("Home 2");
        for are_secrets_read in [false, true] {
            assert!(draft.keeps_secret_of(&saved_profile, are_secrets_read));
        }
        draft.set_security(Security::Open);
        draft.set_security(Security::Personal);
        for are_secrets_read in [false, true] {
            assert!(!draft.keeps_secret_of(&saved_profile, are_secrets_read));
        }
        saved_profile.remove(SECURITY, "psk-flags");
        assert!(draft.keeps_secret_of(&saved_profile, false));
    }

    #[test]
    fn an_unread_stored_private_key_counts_as_kept_until_one_is_typed() {
        let invalid_key_error = Some(tr("The private key is not a WireGuard key"));
        let mut tunnel = Profile::new(WIREGUARD, "Tunnel");
        tunnel.set_interface("wg0");
        tunnel.set_private_key("");
        assert!(tunnel.keeps_secret(false));
        assert!(!tunnel.keeps_secret(true));
        assert_ne!(tunnel.problem(true), invalid_key_error);
        assert_eq!(tunnel.problem(false), invalid_key_error);
        tunnel.put(WIREGUARD, "private-key-flags", 1u32.to_variant());
        assert!(tunnel.keeps_secret(true));
        tunnel.set_private_key("not a key");
        assert_eq!(tunnel.problem(true), invalid_key_error);
    }

    #[test]
    fn a_secret_is_agent_owned_only_when_its_flags_say_so() {
        let mut profile = Profile::new(WIRELESS, "Home");
        let mut vpn_profile = Profile::new_openvpn("Work");
        assert!(!profile.is_agent_owned(SECURITY, "psk-flags"));
        assert!(!vpn_profile.is_vpn_secret_agent_owned("password-flags"));
        assert!(!Peer::default().is_preshared_key_agent_owned());
        for (flags, is_agent_owned) in [(0u32, false), (1, true), (2, false), (3, true), (4, false)]
        {
            profile.put(SECURITY, "psk-flags", flags.to_variant());
            assert_eq!(
                profile.is_agent_owned(SECURITY, "psk-flags"),
                is_agent_owned
            );
            vpn_profile.set_vpn_data("password-flags", &flags.to_string());
            assert_eq!(
                vpn_profile.is_vpn_secret_agent_owned("password-flags"),
                is_agent_owned
            );
            let peer = Peer {
                preshared_key_flags: Some(flags),
                ..Peer::default()
            };
            assert_eq!(peer.is_preshared_key_agent_owned(), is_agent_owned);
        }
    }
}
