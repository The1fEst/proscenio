use gtk4::glib::Variant;
use gtk4::prelude::*;
use std::collections::BTreeMap;

use super::{OPENVPN, Profile, SECRET_FLAG_AGENT_OWNED, SECRET_FLAG_NONE, VPN};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct OpenVpn {
    pub gateway: String,
    pub port: u32,
    pub uses_tcp: bool,
    pub kind: String,
    pub ca: String,
    pub cert: String,
    pub key: String,
    pub key_password: String,
    pub username: String,
    pub password: String,
    pub static_key: String,
    pub static_key_direction: String,
    pub remote_ip: String,
    pub local_ip: String,
    pub tls_auth: String,
    pub tls_auth_direction: String,
}

impl OpenVpn {
    pub fn uses_certificates(&self) -> bool {
        matches!(self.kind.as_str(), "tls" | "password-tls")
    }

    pub fn uses_password(&self) -> bool {
        matches!(self.kind.as_str(), "password" | "password-tls")
    }

    pub fn uses_static_key(&self) -> bool {
        self.kind == "static-key"
    }
}

impl Profile {
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

    fn vpn_secrets(&self) -> BTreeMap<String, String> {
        self.get(VPN, "secrets")
            .and_then(Variant::get::<BTreeMap<String, String>>)
            .unwrap_or_default()
    }

    pub fn new_openvpn(id: &str) -> Profile {
        let mut profile = Profile::new(VPN, id);
        profile.put(VPN, "service-type", OPENVPN.to_variant());
        profile.set_openvpn(&OpenVpn {
            kind: "tls".to_owned(),
            ..OpenVpn::default()
        });
        profile
    }

    pub fn is_openvpn(&self) -> bool {
        self.vpn_service() == OPENVPN
    }

    pub fn openvpn(&self) -> OpenVpn {
        let data: BTreeMap<String, String> = self.vpn_data().into_iter().collect();
        let secrets = self.vpn_secrets();
        let value = |key: &str| data.get(key).cloned().unwrap_or_default();
        OpenVpn {
            gateway: value("remote"),
            port: value("port").parse().unwrap_or(0),
            uses_tcp: value("proto-tcp") == "yes",
            kind: match value("connection-type") {
                kind if kind.is_empty() => "tls".to_owned(),
                kind => kind,
            },
            ca: value("ca"),
            cert: value("cert"),
            key: value("key"),
            key_password: secrets.get("cert-pass").cloned().unwrap_or_default(),
            username: value("username"),
            password: secrets.get("password").cloned().unwrap_or_default(),
            static_key: value("static-key"),
            static_key_direction: value("static-key-direction"),
            remote_ip: value("remote-ip"),
            local_ip: value("local-ip"),
            tls_auth: value("ta"),
            tls_auth_direction: value("ta-dir"),
        }
    }

    pub fn set_openvpn(&mut self, vpn: &OpenVpn) {
        let mut data: BTreeMap<String, String> = self.vpn_data().into_iter().collect();
        let mut secrets = self.vpn_secrets();
        let mut set = |key: &str, value: &str, is_used: bool| {
            let value = value.trim();
            if is_used && !value.is_empty() {
                data.insert(key.to_owned(), value.to_owned());
            } else {
                data.remove(key);
            }
        };
        let port = if vpn.port > 0 {
            vpn.port.to_string()
        } else {
            String::new()
        };
        set("remote", &vpn.gateway, true);
        set("port", &port, true);
        set("proto-tcp", "yes", vpn.uses_tcp);
        set("connection-type", &vpn.kind, true);
        let uses_certificates = vpn.uses_certificates();
        let uses_password = vpn.uses_password();
        let uses_static_key = vpn.uses_static_key();
        set("ca", &vpn.ca, !uses_static_key);
        set("cert", &vpn.cert, uses_certificates);
        set("key", &vpn.key, uses_certificates);
        set("username", &vpn.username, uses_password);
        set("static-key", &vpn.static_key, uses_static_key);
        set(
            "static-key-direction",
            &vpn.static_key_direction,
            uses_static_key,
        );
        set("remote-ip", &vpn.remote_ip, uses_static_key);
        set("local-ip", &vpn.local_ip, uses_static_key);
        set("ta", &vpn.tls_auth, !uses_static_key);
        set(
            "ta-dir",
            &vpn.tls_auth_direction,
            !uses_static_key && !vpn.tls_auth.trim().is_empty(),
        );
        let has_key_password = uses_certificates && !vpn.key_password.is_empty();
        let has_password = uses_password && !vpn.password.is_empty();
        for (key, has_secret) in [
            ("cert-pass-flags", has_key_password),
            ("password-flags", has_password),
        ] {
            if has_secret {
                data.entry(key.to_owned())
                    .or_insert_with(|| SECRET_FLAG_NONE.to_owned());
            }
        }
        let mut set_secret = |key: &str, value: &str, is_used: bool| {
            if is_used && !value.is_empty() {
                secrets.insert(key.to_owned(), value.to_owned());
            } else {
                secrets.remove(key);
            }
        };
        set_secret("cert-pass", &vpn.key_password, has_key_password);
        set_secret("password", &vpn.password, uses_password);
        self.put(VPN, "data", data.to_variant());
        if secrets.is_empty() {
            self.remove(VPN, "secrets");
        } else {
            self.put(VPN, "secrets", secrets.to_variant());
        }
    }

    pub(super) fn merge_vpn_secrets(&mut self, stored_secrets: &Variant, should_overwrite: bool) {
        let Some(stored_secrets) = stored_secrets.get::<BTreeMap<String, String>>() else {
            return;
        };
        let mut secrets = self.vpn_secrets();
        for (key, value) in stored_secrets {
            if should_overwrite || !secrets.contains_key(&key) {
                secrets.insert(key, value);
            }
        }
        self.put(VPN, "secrets", secrets.to_variant());
    }

    pub fn is_vpn_secret_agent_owned(&self, flags_key: &str) -> bool {
        self.get(VPN, "data")
            .and_then(Variant::get::<BTreeMap<String, String>>)
            .and_then(|data| data.get(flags_key)?.parse::<u32>().ok())
            .is_some_and(|flags| flags & SECRET_FLAG_AGENT_OWNED != 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::i18n::tr;
    use crate::platform::network::nmprofile::Settings;
    use std::collections::HashMap;

    #[test]
    fn openvpn_fields_land_in_the_plugin_data_and_secrets() {
        let mut profile = Profile::new_openvpn("Work");
        assert!(profile.is_openvpn());
        assert_eq!(profile.openvpn().kind, "tls");
        assert_eq!(
            profile.problem(false),
            Some(tr("An OpenVPN connection needs a gateway"))
        );
        profile.set_vpn_data("cipher", "AES-256-GCM");
        let vpn = OpenVpn {
            gateway: "vpn.example.org".to_owned(),
            port: 1194,
            uses_tcp: true,
            kind: "password-tls".to_owned(),
            ca: "/home/me/ca.crt".to_owned(),
            cert: "/home/me/me.crt".to_owned(),
            key: "/home/me/me.key".to_owned(),
            key_password: "unlock".to_owned(),
            username: "me".to_owned(),
            password: "secret".to_owned(),
            tls_auth: "/home/me/ta.key".to_owned(),
            tls_auth_direction: "1".to_owned(),
            ..OpenVpn::default()
        };
        profile.set_openvpn(&vpn);
        assert_eq!(profile.openvpn(), vpn);
        assert_eq!(profile.problem(false), None);
        let data: BTreeMap<String, String> = profile.vpn_data().into_iter().collect();
        assert_eq!(data.get("cipher").map(String::as_str), Some("AES-256-GCM"));
        assert_eq!(data.get("password-flags").map(String::as_str), Some("0"));
        assert!(!data.contains_key("password"));
        assert_eq!(
            profile.vpn_secrets().get("password").map(String::as_str),
            Some("secret")
        );

        profile.set_openvpn(&OpenVpn {
            kind: "static-key".to_owned(),
            static_key: "/home/me/static.key".to_owned(),
            ..vpn
        });
        let static_key_vpn = profile.openvpn();
        assert!(
            static_key_vpn.cert.is_empty()
                && static_key_vpn.username.is_empty()
                && static_key_vpn.ca.is_empty()
        );
        assert!(profile.vpn_secrets().is_empty());
        assert_eq!(
            profile.problem(false),
            Some(tr(
                "A static key OpenVPN needs the key file and both tunnel addresses"
            ))
        );
    }

    #[test]
    fn openvpn_edits_keep_where_each_password_is_stored() {
        let mut profile = Profile::new_openvpn("Work");
        profile.set_vpn_data("password-flags", "1");
        profile.set_vpn_data("cert-pass-flags", "1");
        let vpn = OpenVpn {
            gateway: "vpn.example.org".to_owned(),
            kind: "password-tls".to_owned(),
            username: "me".to_owned(),
            ..profile.openvpn()
        };
        for (kind, secret) in [
            ("password-tls", ""),
            ("static-key", ""),
            ("password-tls", "secret"),
        ] {
            profile.set_openvpn(&OpenVpn {
                kind: kind.to_owned(),
                key_password: secret.to_owned(),
                password: secret.to_owned(),
                ..vpn.clone()
            });
            let data: BTreeMap<String, String> = profile.vpn_data().into_iter().collect();
            assert_eq!(data.get("password-flags").map(String::as_str), Some("1"));
            assert_eq!(data.get("cert-pass-flags").map(String::as_str), Some("1"));
        }
        let OpenVpn {
            key_password,
            password,
            ..
        } = profile.openvpn();
        assert_eq!(
            (key_password.as_str(), password.as_str()),
            ("secret", "secret")
        );
    }

    #[test]
    fn stored_vpn_secrets_fill_each_one_the_draft_lacks() {
        let mut draft = Profile::new_openvpn("Work");
        draft.set_openvpn(&OpenVpn {
            kind: "password-tls".to_owned(),
            password: "typed".to_owned(),
            ..draft.openvpn()
        });
        let stored_secrets = BTreeMap::from([
            ("password".to_owned(), "stored".to_owned()),
            ("cert-pass".to_owned(), "unlock".to_owned()),
        ]);
        let secrets = Settings::from([(
            VPN.to_owned(),
            HashMap::from([("secrets".to_owned(), stored_secrets.to_variant())]),
        )]);
        let mut profile = draft.clone();
        draft.merge_secrets(&secrets, false);
        profile.merge_secrets(&secrets, true);
        let into_secret_pair = |vpn: OpenVpn| (vpn.password, vpn.key_password);
        assert_eq!(
            into_secret_pair(draft.openvpn()),
            ("typed".to_owned(), "unlock".to_owned())
        );
        assert_eq!(
            into_secret_pair(profile.openvpn()),
            ("stored".to_owned(), "unlock".to_owned())
        );
    }
}
