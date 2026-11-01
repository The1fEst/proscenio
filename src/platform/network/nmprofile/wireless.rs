use gtk4::glib::Variant;
use gtk4::prelude::*;

use super::enterprise::Eap;
use super::{EAP, PSK_HEX_LENGTH, Profile, SECURITY, WEP_HEX_LENGTHS, WIRELESS, wep_key_fits};

const WEP_PASSPHRASE_TYPE: u32 = 2;
const PAYLOAD_SPECIAL_CHARACTERS: [char; 5] = ['\\', ';', ',', ':', '"'];

pub struct NetworkShare {
    pub ssid: String,
    pub password: String,
    pub payload: String,
}

fn format_payload_field(text: &str, is_raw_key: bool) -> String {
    let mut field = String::with_capacity(text.len() + 2);
    let is_hex = text.chars().all(|character| character.is_ascii_hexdigit());
    if is_hex && !is_raw_key {
        field.push('"');
    }
    for character in text.chars() {
        if PAYLOAD_SPECIAL_CHARACTERS.contains(&character) {
            field.push('\\');
        }
        field.push(character);
    }
    if is_hex && !is_raw_key {
        field.push('"');
    }
    field
}

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

    pub fn to_network_share(&self) -> Option<NetworkShare> {
        let security = self.security();
        let (kind, password, is_raw_key) = match security {
            Security::Open => ("nopass", String::new(), false),
            Security::Personal | Security::Wpa3 => {
                let psk = self.psk();
                let is_raw_key = psk.len() == PSK_HEX_LENGTH;
                ("WPA", psk, is_raw_key)
            }
            Security::Wep => {
                let read_number = |key| {
                    self.get(SECURITY, key)
                        .and_then(Variant::get::<u32>)
                        .unwrap_or(0)
                };
                let key = self.string(
                    SECURITY,
                    &format!("wep-key{}", read_number("wep-tx-keyidx")),
                );
                if read_number("wep-key-type") == WEP_PASSPHRASE_TYPE || !wep_key_fits(&key) {
                    return None;
                }
                let is_raw_key = WEP_HEX_LENGTHS.contains(&key.len());
                ("WEP", key, is_raw_key)
            }
            Security::Enterprise | Security::Other => return None,
        };
        let ssid = String::from_utf8(self.get(WIRELESS, "ssid")?.get::<Vec<u8>>()?).ok()?;
        if ssid.is_empty() || (security != Security::Open && password.is_empty()) {
            return None;
        }
        let mut payload = format!("WIFI:T:{kind};S:{};", format_payload_field(&ssid, false));
        if !password.is_empty() {
            payload.push_str(&format!(
                "P:{};",
                format_payload_field(&password, is_raw_key)
            ));
        }
        if self.is_hidden() {
            payload.push_str("H:true;");
        }
        payload.push(';');
        Some(NetworkShare {
            ssid,
            password,
            payload,
        })
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

    fn new_profile(ssid: &str, security: Security, secret: &str) -> Profile {
        let mut profile = Profile::new(WIRELESS, ssid);
        profile.set_ssid(ssid);
        profile.set_security(security);
        match security {
            Security::Wep => profile.set_wep_key(secret),
            _ => profile.set_psk(secret),
        }
        profile
    }

    #[test]
    fn a_network_share_names_the_security_and_escapes_special_characters() {
        let mut hidden_profile = new_profile("Cafe;\"Guest\"", Security::Personal, "pa:ss,w\\rd");
        hidden_profile.set_hidden(true);
        for (profile, payload) in [
            (
                new_profile("Home", Security::Personal, "secret12"),
                "WIFI:T:WPA;S:Home;P:secret12;;",
            ),
            (
                new_profile("Home", Security::Wpa3, "secret12"),
                "WIFI:T:WPA;S:Home;P:secret12;;",
            ),
            (
                new_profile("Home", Security::Wep, "wxyz1"),
                "WIFI:T:WEP;S:Home;P:wxyz1;;",
            ),
            (
                new_profile("Open: Cafe", Security::Open, ""),
                "WIFI:T:nopass;S:Open\\: Cafe;;",
            ),
            (
                hidden_profile,
                "WIFI:T:WPA;S:Cafe\\;\\\"Guest\\\";P:pa\\:ss\\,w\\\\rd;H:true;;",
            ),
        ] {
            let share = profile.to_network_share().unwrap();
            assert_eq!(share.payload, payload);
        }
    }

    #[test]
    fn hexadecimal_looking_values_are_quoted_unless_they_are_raw_keys() {
        let raw_psk = "0123456789abcdef".repeat(4);
        for (profile, payload) in [
            (
                new_profile("1234", Security::Personal, "12345678"),
                "WIFI:T:WPA;S:\"1234\";P:\"12345678\";;".to_owned(),
            ),
            (
                new_profile("Home", Security::Personal, "abcdefgh"),
                "WIFI:T:WPA;S:Home;P:abcdefgh;;".to_owned(),
            ),
            (
                new_profile("Home", Security::Wpa3, &raw_psk),
                format!("WIFI:T:WPA;S:Home;P:{raw_psk};;"),
            ),
            (
                new_profile("Home", Security::Wep, "0123456789"),
                "WIFI:T:WEP;S:Home;P:0123456789;;".to_owned(),
            ),
            (
                new_profile("Home", Security::Wep, "12345"),
                "WIFI:T:WEP;S:Home;P:\"12345\";;".to_owned(),
            ),
        ] {
            assert_eq!(profile.to_network_share().unwrap().payload, payload);
        }
    }

    #[test]
    fn a_wep_passphrase_or_a_key_of_the_wrong_length_has_no_share() {
        let mut passphrase = new_profile("Home", Security::Wep, "12345");
        passphrase.put(SECURITY, "wep-key-type", 2u32.to_variant());
        assert!(passphrase.to_network_share().is_none());
        assert!(
            new_profile("Home", Security::Wep, "wxyz123")
                .to_network_share()
                .is_none()
        );
    }

    #[test]
    fn a_wep_share_carries_the_key_the_profile_transmits_with() {
        let mut profile = new_profile("Home", Security::Wep, "abcde");
        profile.put(SECURITY, "wep-key1", "fghij".to_variant());
        profile.put(SECURITY, "wep-tx-keyidx", 1u32.to_variant());
        assert_eq!(
            profile.to_network_share().unwrap().payload,
            "WIFI:T:WEP;S:Home;P:fghij;;"
        );
    }

    #[test]
    fn a_network_name_that_is_not_text_has_no_share() {
        let mut profile = new_profile("Home", Security::Personal, "secret12");
        profile.put(WIRELESS, "ssid", vec![0xFFu8, 0xFE].to_variant());
        assert!(profile.to_network_share().is_none());
    }

    #[test]
    fn a_network_share_keeps_the_name_and_the_password_unescaped() {
        let share = new_profile("Cafe;1", Security::Personal, "pa;ss")
            .to_network_share()
            .unwrap();
        assert_eq!(share.ssid, "Cafe;1");
        assert_eq!(share.password, "pa;ss");
    }

    #[test]
    fn a_network_without_a_shareable_sign_in_has_no_share() {
        assert!(
            new_profile("Home", Security::Personal, "")
                .to_network_share()
                .is_none()
        );
        assert!(
            new_profile("Home", Security::Wep, "")
                .to_network_share()
                .is_none()
        );
        assert!(
            new_profile("Office", Security::Enterprise, "")
                .to_network_share()
                .is_none()
        );
        assert!(
            new_profile("", Security::Personal, "secret12")
                .to_network_share()
                .is_none()
        );
    }

    #[test]
    fn the_longest_network_share_fits_a_version_11_code() {
        let mut profile = new_profile(&";".repeat(32), Security::Personal, &",".repeat(63));
        profile.set_hidden(true);
        let share = profile.to_network_share().unwrap();
        assert_eq!(share.payload.len(), 215);
        let code = crate::core::qr::encode(share.payload.as_bytes()).unwrap();
        assert_eq!(code.size(), 61);
    }
}
