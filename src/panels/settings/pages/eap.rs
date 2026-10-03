use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::connection::{
    self, EAP_METHODS, Editor, Part, choices, edit_eap, inner_methods,
};

pub fn build(context: &Context) -> Rc<Page> {
    connection::show(context, Part::Sub(fill))
}

fn fill(editor: &Rc<Editor>, page: &Page) {
    if !editor.draft.borrow().has_eap() {
        editor.leave();
        return;
    }
    let eap = editor.draft.borrow().eap();
    let section = editor.section("shield_lock", &tr("802.1X security"));
    let method = editor.subsection(page, &section, &tr("Authentication"), "");
    editor.choose(
        &method,
        choices(&EAP_METHODS),
        Value::from(eap.method.as_str()),
        |profile, value| {
            let method = value.as_str().unwrap_or("peap").to_owned();
            edit_eap(profile, |eap| {
                let inner = inner_methods(&method);
                if !inner.iter().any(|(_, known)| *known == eap.inner) {
                    eap.inner = inner
                        .first()
                        .map(|(_, first)| *first)
                        .unwrap_or_default()
                        .to_owned();
                }
                eap.method = method;
            });
        },
    );
    editor.field(
        &section,
        &tr("User name"),
        &eap.identity,
        |text, profile| {
            edit_eap(profile, |eap| eap.identity = text.to_owned());
            Ok(())
        },
    );
    if eap.tunneled() {
        editor.field(
            &section,
            &tr("Anonymous identity"),
            &eap.anonymous_identity,
            |text, profile| {
                edit_eap(profile, |eap| eap.anonymous_identity = text.to_owned());
                Ok(())
            },
        );
        let inner = editor.subsection(page, &section, &tr("Inner authentication"), "");
        editor.choose(
            &inner,
            choices(inner_methods(&eap.method)),
            Value::from(eap.inner.as_str()),
            |profile, value| {
                let chosen = value.as_str().unwrap_or_default().to_owned();
                edit_eap(profile, |eap| eap.inner = chosen);
            },
        );
        editor.secret_field(&section, &tr("Password"), &eap.password, |text, profile| {
            edit_eap(profile, |eap| eap.password = text.to_owned());
            Ok(())
        });
    } else {
        editor.certificate_field(
            &section,
            &tr("User certificate"),
            &eap.client_cert,
            |profile, path| edit_eap(profile, |eap| eap.client_cert = path),
        );
        editor.certificate_field(
            &section,
            &tr("Private key"),
            &eap.private_key,
            |profile, path| edit_eap(profile, |eap| eap.private_key = path),
        );
        editor.secret_field(
            &section,
            &tr("Private key password"),
            &eap.key_password,
            |text, profile| {
                edit_eap(profile, |eap| eap.key_password = text.to_owned());
                Ok(())
            },
        );
    }
    editor.certificate_field(
        &section,
        &tr("CA certificate (empty trusts any server)"),
        &eap.ca_cert,
        |profile, path| edit_eap(profile, |eap| eap.ca_cert = path),
    );
    editor.field(
        &section,
        &tr("Server domain"),
        &eap.domain,
        |text, profile| {
            edit_eap(profile, |eap| eap.domain = text.to_owned());
            Ok(())
        },
    );
}
