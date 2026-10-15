use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use super::fields::{note, show_note};
use super::{Editor, PORT_MAX, choices};
use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::Page;
use crate::platform::network::nmprofile::{self, Profile};

const OPENVPN_KINDS: [(&str, &str); 4] = [
    ("Certificates", "tls"),
    ("Password", "password"),
    ("Password and certificates", "password-tls"),
    ("Static key", "static-key"),
];
const KEY_DIRECTIONS: [(&str, &str); 3] = [("No direction", ""), ("0", "0"), ("1", "1")];

fn edit_openvpn(profile: &mut Profile, change: impl FnOnce(&mut nmprofile::vpn::OpenVpn)) {
    let mut vpn = profile.openvpn();
    change(&mut vpn);
    profile.set_openvpn(&vpn);
}

impl Editor {
    fn openvpn(self: &Rc<Self>, page: &Page) {
        let section = self.section("vpn_key", &tr("OpenVPN"));
        let vpn = self.draft.borrow().openvpn();
        self.field(&section, &tr("Gateway"), &vpn.gateway, |text, profile| {
            edit_openvpn(profile, |vpn| vpn.gateway = text.to_owned());
            Ok(())
        });
        self.spin(
            &section,
            "numbers",
            &tr("Port (0 is the default, 1194)"),
            (0, PORT_MAX),
            vpn.port as i64,
            |profile, port| edit_openvpn(profile, |vpn| vpn.port = port as u32),
        );
        self.switch(
            &section,
            "swap_horiz",
            &tr("Use TCP"),
            |profile| profile.openvpn().uses_tcp,
            |profile, is_on| edit_openvpn(profile, |vpn| vpn.uses_tcp = is_on),
        );
        let kind_section = self.subsection(page, &section, &tr("Authentication"), "");
        self.choose(
            &kind_section,
            choices(&OPENVPN_KINDS),
            Value::from(vpn.kind.as_str()),
            |profile, value| {
                let kind = value.as_str().unwrap_or("tls").to_owned();
                edit_openvpn(profile, |vpn| vpn.kind = kind);
            },
        );
        if !vpn.uses_static_key() {
            self.certificate_field(&section, &tr("CA certificate"), &vpn.ca, |profile, path| {
                edit_openvpn(profile, |vpn| vpn.ca = path)
            });
        }
        if vpn.uses_certificates() {
            self.certificate_field(
                &section,
                &tr("User certificate"),
                &vpn.cert,
                |profile, path| edit_openvpn(profile, |vpn| vpn.cert = path),
            );
            self.certificate_field(&section, &tr("Private key"), &vpn.key, |profile, path| {
                edit_openvpn(profile, |vpn| vpn.key = path)
            });
            let is_agent_owned = self
                .draft
                .borrow()
                .is_vpn_secret_agent_owned("cert-pass-flags");
            self.secret_field(
                &section,
                &tr("Private key password"),
                &vpn.key_password,
                is_agent_owned,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.key_password = text.to_owned());
                    Ok(())
                },
            );
        }
        if vpn.uses_password() {
            self.field(
                &section,
                &tr("User name"),
                &vpn.username,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.username = text.to_owned());
                    Ok(())
                },
            );
            let is_agent_owned = self
                .draft
                .borrow()
                .is_vpn_secret_agent_owned("password-flags");
            self.secret_field(
                &section,
                &tr("Password"),
                &vpn.password,
                is_agent_owned,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.password = text.to_owned());
                    Ok(())
                },
            );
        }
        if vpn.uses_static_key() {
            self.certificate_field(
                &section,
                &tr("Static key"),
                &vpn.static_key,
                |profile, path| edit_openvpn(profile, |vpn| vpn.static_key = path),
            );
            let direction_section = self.subsection(page, &section, &tr("Key direction"), "");
            self.choose(
                &direction_section,
                choices(&KEY_DIRECTIONS),
                Value::from(vpn.static_key_direction.as_str()),
                |profile, value| {
                    let direction = value.as_str().unwrap_or_default().to_owned();
                    edit_openvpn(profile, |vpn| vpn.static_key_direction = direction);
                },
            );
            self.field(
                &section,
                &tr("Remote tunnel address"),
                &vpn.remote_ip,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.remote_ip = text.to_owned());
                    Ok(())
                },
            );
            self.field(
                &section,
                &tr("Local tunnel address"),
                &vpn.local_ip,
                |text, profile| {
                    edit_openvpn(profile, |vpn| vpn.local_ip = text.to_owned());
                    Ok(())
                },
            );
            return;
        }
        let tls_auth_section = self.subsection(
            page,
            &section,
            &tr("TLS authentication"),
            &tr("A key shared by the server and every client that signs the TLS handshake, the tls-auth line of an OpenVPN config"),
        );
        self.certificate_field(
            &tls_auth_section,
            &tr("TLS authentication key (optional)"),
            &vpn.tls_auth,
            |profile, path| edit_openvpn(profile, |vpn| vpn.tls_auth = path),
        );
        if !vpn.tls_auth.is_empty() {
            self.choose(
                &tls_auth_section,
                choices(&KEY_DIRECTIONS),
                Value::from(vpn.tls_auth_direction.as_str()),
                |profile, value| {
                    let direction = value.as_str().unwrap_or_default().to_owned();
                    edit_openvpn(profile, |vpn| vpn.tls_auth_direction = direction);
                },
            );
        }
    }

    pub(super) fn vpn(self: &Rc<Self>, page: &Page) {
        if self.draft.borrow().is_openvpn() {
            self.openvpn(page);
            return;
        }
        let section = self.section("vpn_key", &tr("VPN"));
        let profile = self.draft.borrow().clone();
        let plugin_note = note();
        let plugin = profile
            .vpn_service()
            .rsplit('.')
            .next()
            .unwrap_or_default()
            .to_owned();
        show_note(
            &plugin_note,
            Some(&trf("Plugin: %1", &[&plugin])),
            "colSubtext",
        );
        section.append(&plugin_note);
        for (key, value) in profile.vpn_data() {
            self.field(&section, &key, &value, {
                let key = key.clone();
                move |text, profile| {
                    profile.set_vpn_data(&key, text.trim());
                    Ok(())
                }
            });
        }
    }
}
