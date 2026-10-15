use gtk4::glib::Variant;
use gtk4::prelude::*;

use super::{EAP, Profile};

const FILE_SCHEME: &str = "file://";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Eap {
    pub method: String,
    pub identity: String,
    pub anonymous_identity: String,
    pub inner: String,
    pub password: String,
    pub ca_cert: String,
    pub domain: String,
    pub client_cert: String,
    pub private_key: String,
    pub key_password: String,
}

impl Eap {
    pub fn is_tunneled(&self) -> bool {
        matches!(self.method.as_str(), "peap" | "ttls")
    }
}

fn certificate_path(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let path = text.strip_prefix(FILE_SCHEME)?.trim_end_matches('\0');
    Some(path.to_owned())
}

fn certificate_bytes(path: &str) -> Vec<u8> {
    let mut bytes = format!("{FILE_SCHEME}{path}").into_bytes();
    bytes.push(0);
    bytes
}

impl Profile {
    pub fn has_eap(&self) -> bool {
        self.settings.contains_key(EAP)
    }

    pub fn set_wired_eap(&mut self, is_enabled: bool) {
        if !is_enabled {
            self.settings.remove(EAP);
            return;
        }
        if !self.has_eap() {
            self.set_eap(&Eap {
                method: "peap".to_owned(),
                inner: "mschapv2".to_owned(),
                ..Eap::default()
            });
        }
    }

    fn certificate(&self, key: &str) -> String {
        self.get(EAP, key)
            .and_then(Variant::get::<Vec<u8>>)
            .and_then(|bytes| certificate_path(&bytes))
            .unwrap_or_default()
    }

    fn set_certificate(&mut self, key: &str, path: &str) {
        if self.certificate(key) == path {
            return;
        }
        if path.is_empty() {
            self.remove(EAP, key);
        } else {
            self.put(EAP, key, certificate_bytes(path).to_variant());
        }
    }

    pub fn eap(&self) -> Eap {
        Eap {
            method: self
                .strings(EAP, "eap")
                .into_iter()
                .next()
                .unwrap_or_else(|| "peap".to_owned()),
            identity: self.string(EAP, "identity"),
            anonymous_identity: self.string(EAP, "anonymous-identity"),
            inner: self.string(EAP, "phase2-auth"),
            password: self.string(EAP, "password"),
            ca_cert: self.certificate("ca-cert"),
            domain: self.string(EAP, "domain-suffix-match"),
            client_cert: self.certificate("client-cert"),
            private_key: self.certificate("private-key"),
            key_password: self.string(EAP, "private-key-password"),
        }
    }

    pub fn set_eap(&mut self, eap: &Eap) {
        self.put(EAP, "eap", vec![eap.method.clone()].to_variant());
        self.set_string(EAP, "identity", eap.identity.trim());
        self.set_certificate("ca-cert", eap.ca_cert.trim());
        self.set_string(EAP, "domain-suffix-match", eap.domain.trim());
        let is_tunneled = eap.is_tunneled();
        let (anonymous_identity, inner, password) = if is_tunneled {
            (
                eap.anonymous_identity.trim(),
                eap.inner.as_str(),
                eap.password.as_str(),
            )
        } else {
            ("", "", "")
        };
        self.set_string(EAP, "anonymous-identity", anonymous_identity);
        self.set_string(EAP, "phase2-auth", inner);
        self.set_string(EAP, "password", password);
        let (client_cert, key, key_password) = if is_tunneled {
            ("", "", "")
        } else {
            (
                eap.client_cert.trim(),
                eap.private_key.trim(),
                eap.key_password.as_str(),
            )
        };
        self.set_certificate("client-cert", client_cert);
        self.set_certificate("private-key", key);
        self.set_string(EAP, "private-key-password", key_password);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::i18n::tr;
    use crate::platform::network::nmprofile::wireless::Security;
    use crate::platform::network::nmprofile::{WIRED, WIRELESS};

    #[test]
    fn enterprise_and_wep_security_keep_only_their_own_keys() {
        let mut profile = Profile::new(WIRELESS, "Campus");
        profile.set_ssid("eduroam");
        profile.set_security(Security::Enterprise);
        assert_eq!(profile.security(), Security::Enterprise);
        assert_eq!(profile.eap().method, "peap");
        assert_eq!(profile.problem(false), Some(tr("802.1X needs a user name")));
        let peap = Eap {
            method: "peap".to_owned(),
            identity: "student@uni.example".to_owned(),
            anonymous_identity: "anonymous@uni.example".to_owned(),
            inner: "mschapv2".to_owned(),
            password: "secret".to_owned(),
            ca_cert: "/etc/ssl/certs/uni.pem".to_owned(),
            domain: "radius.uni.example".to_owned(),
            ..Eap::default()
        };
        profile.set_eap(&peap);
        assert_eq!(profile.eap(), peap);
        assert_eq!(profile.problem(false), None);
        assert_eq!(
            profile
                .get(EAP, "ca-cert")
                .and_then(Variant::get::<Vec<u8>>),
            Some(b"file:///etc/ssl/certs/uni.pem\0".to_vec())
        );

        let tls = Eap {
            method: "tls".to_owned(),
            identity: "laptop".to_owned(),
            ..peap.clone()
        };
        profile.set_eap(&tls);
        let eap = profile.eap();
        assert!(eap.password.is_empty() && eap.inner.is_empty());
        assert_eq!(
            profile.problem(false),
            Some(tr("TLS needs a user certificate and a private key"))
        );
        profile.set_eap(&Eap {
            client_cert: "/home/me/laptop.pem".to_owned(),
            private_key: "/home/me/laptop.key".to_owned(),
            key_password: "unlock".to_owned(),
            ..tls
        });
        assert_eq!(profile.problem(false), None);

        profile.set_security(Security::Wep);
        assert_eq!(profile.security(), Security::Wep);
        assert!(!profile.has_eap());
        profile.set_wep_key("abc");
        assert!(profile.problem(false).is_some());
        profile.set_wep_key("0123456789");
        assert_eq!(profile.problem(false), None);
        profile.set_security(Security::Personal);
        assert!(profile.wep_key().is_empty());

        let mut wired = Profile::new(WIRED, "Office");
        wired.set_wired_eap(true);
        assert!(wired.has_eap());
        wired.set_wired_eap(false);
        assert!(!wired.has_eap());
    }
}
