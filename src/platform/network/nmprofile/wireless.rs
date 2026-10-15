use gtk4::glib::Variant;
use gtk4::prelude::*;

use super::enterprise::Eap;
use super::{EAP, Profile, SECURITY, WIRELESS};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Security {
    Open,
    Personal,
    Wpa3,
    Enterprise,
    Wep,
    Other,
}

impl Profile {
    pub fn ssid(&self) -> String {
        self.get(WIRELESS, "ssid")
            .and_then(Variant::get::<Vec<u8>>)
            .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            .unwrap_or_default()
    }

    pub fn set_ssid(&mut self, ssid: &str) {
        self.put(WIRELESS, "ssid", ssid.as_bytes().to_variant());
    }

    pub fn is_hidden(&self) -> bool {
        self.flag(WIRELESS, "hidden", false)
    }

    pub fn set_hidden(&mut self, is_hidden: bool) {
        self.put(WIRELESS, "hidden", is_hidden.to_variant());
    }

    pub fn security(&self) -> Security {
        if !self.settings.contains_key(SECURITY) {
            return Security::Open;
        }
        match self.string(SECURITY, "key-mgmt").as_str() {
            "wpa-psk" => Security::Personal,
            "sae" => Security::Wpa3,
            "wpa-eap" => Security::Enterprise,
            "none" => Security::Wep,
            _ => Security::Other,
        }
    }

    pub fn set_security(&mut self, security: Security) {
        let key_management = match security {
            Security::Open => {
                self.settings.remove(SECURITY);
                self.settings.remove(EAP);
                self.remove(WIRELESS, "security");
                return;
            }
            Security::Personal => "wpa-psk",
            Security::Wpa3 => "sae",
            Security::Enterprise => "wpa-eap",
            Security::Wep => "none",
            Security::Other => return,
        };
        self.put(SECURITY, "key-mgmt", key_management.to_variant());
        if security == Security::Wep {
            self.remove(SECURITY, "psk");
            self.put(SECURITY, "auth-alg", "open".to_variant());
            self.put(SECURITY, "wep-key-type", 1u32.to_variant());
            self.put(SECURITY, "wep-tx-keyidx", 0u32.to_variant());
        } else {
            for key in ["auth-alg", "wep-key-type", "wep-tx-keyidx", "wep-key0"] {
                self.remove(SECURITY, key);
            }
        }
        if security == Security::Enterprise {
            self.remove(SECURITY, "psk");
            if !self.settings.contains_key(EAP) {
                self.set_eap(&Eap {
                    method: "peap".to_owned(),
                    inner: "mschapv2".to_owned(),
                    ..Eap::default()
                });
            }
        } else {
            self.settings.remove(EAP);
        }
    }

    pub fn wep_key(&self) -> String {
        self.string(SECURITY, "wep-key0")
    }

    pub fn set_wep_key(&mut self, key: &str) {
        self.set_string(SECURITY, "wep-key0", key);
    }

    pub fn psk(&self) -> String {
        self.string(SECURITY, "psk")
    }

    pub fn set_psk(&mut self, psk: &str) {
        self.set_string(SECURITY, "psk", psk);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::i18n::tr;

    #[test]
    fn wireless_security_and_ssid_are_read_back() {
        let mut profile = Profile::new(WIRELESS, "Home");
        profile.set_ssid("Home");
        assert_eq!(profile.security(), Security::Open);
        assert_eq!(profile.problem(false), None);
        profile.set_security(Security::Personal);
        profile.set_psk("short");
        assert_eq!(profile.security(), Security::Personal);
        assert_eq!(
            profile.problem(false),
            Some(tr("A WPA password has 8 to 63 characters"))
        );
        profile.set_psk("long enough");
        assert_eq!(profile.problem(false), None);
        profile.set_security(Security::Open);
        assert_eq!(profile.security(), Security::Open);
        assert_eq!(profile.ssid(), "Home");
    }
}
