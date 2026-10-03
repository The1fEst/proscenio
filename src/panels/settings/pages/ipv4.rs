use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::connection::{
    self, Editor, IPV4_METHODS, IPV6_METHODS, Part, choices,
};
use crate::platform::nmprofile::{self, Family, Profile};

pub fn build(context: &Context) -> Rc<Page> {
    connection::show(
        context,
        Part::Sub(|editor, page| {
            ip(editor, page, Family::V4);
        }),
    )
}

pub(super) fn ip(editor: &Rc<Editor>, page: &Page, family: Family) -> gtk4::Box {
    let section = editor.section("", "");
    let ip = editor.draft.borrow().ip(family);
    let methods: &[(&str, &str)] = match family {
        Family::V4 => &IPV4_METHODS,
        Family::V6 => &IPV6_METHODS,
    };
    let mut options = choices(methods);
    if !methods.iter().any(|(_, method)| *method == ip.method) {
        options.push((ip.method.clone(), Value::from(ip.method.clone())));
    }
    let method = editor.subsection(page, &section, &tr("Method"), "");
    editor.choose(
        &method,
        options,
        Value::from(ip.method.clone()),
        move |profile, value| {
            let mut ip = profile.ip(family);
            ip.method = value.as_str().unwrap_or("auto").to_owned();
            profile.set_ip(family, &ip);
        },
    );
    let automatic = matches!(ip.method.as_str(), "auto" | "dhcp");
    let configured = automatic || matches!(ip.method.as_str(), "manual" | "shared");
    if !configured {
        return section;
    }
    if ip.method == "manual" {
        let addresses = editor.subsection(
            page,
            &section,
            &tr("Addresses"),
            &tr("Separated by commas, each with its prefix length, such as 192.168.1.10/24"),
        );
        editor.field(
            &addresses,
            &tr("Addresses"),
            &nmprofile::format_addresses(&ip.addresses),
            move |text, profile| {
                let parsed = nmprofile::parse_addresses(text, family)?;
                edit_ip(profile, family, &|ip| ip.addresses = parsed.clone());
                Ok(())
            },
        );
        editor.field(
            &addresses,
            &tr("Gateway"),
            &ip.gateway,
            move |text, profile| {
                let parsed = nmprofile::parse_gateway(text, family)?;
                edit_ip(profile, family, &|ip| ip.gateway = parsed.clone());
                Ok(())
            },
        );
    }
    let dns = editor.subsection(page, &section, &tr("DNS"), "");
    if automatic {
        editor.switch(
            &dns,
            "dns",
            &tr("Automatic DNS"),
            move |profile| profile.ip(family).automatic_dns,
            move |profile, on| edit_ip(profile, family, &|ip| ip.automatic_dns = on),
        );
    }
    editor.field(
        &dns,
        &tr("DNS servers"),
        &ip.dns.join(", "),
        move |text, profile| {
            let parsed = nmprofile::parse_servers(text, family)?;
            edit_ip(profile, family, &|ip| ip.dns = parsed.clone());
            Ok(())
        },
    );
    editor.field(
        &dns,
        &tr("Search domains"),
        &ip.search.join(", "),
        move |text, profile| {
            let parsed = nmprofile::items(text);
            edit_ip(profile, family, &|ip| ip.search = parsed.clone());
            Ok(())
        },
    );
    let routing = editor.subsection(page, &section, &tr("Routing"), "");
    if automatic {
        editor.switch(
            &routing,
            "route",
            &tr("Automatic routes"),
            move |profile| profile.ip(family).automatic_routes,
            move |profile, on| edit_ip(profile, family, &|ip| ip.automatic_routes = on),
        );
    }
    let own = editor.switch(
        &routing,
        "alt_route",
        &tr("Only for its own network"),
        move |profile| profile.ip(family).never_default,
        move |profile, on| edit_ip(profile, family, &|ip| ip.never_default = on),
    );
    editor.hold(page.unkept_tip(
        &own.button,
        &tr("Never the default route: only traffic for this network's addresses and routes goes through it"),
    ));
    let routes = editor.subsection(
        page,
        &section,
        &tr("Routes"),
        &tr("Separated by commas, written like ip route: 10.0.0.0/8 via 192.168.1.1 metric 100"),
    );
    editor.field(
        &routes,
        &tr("Routes"),
        &nmprofile::format_routes(&ip.routes),
        move |text, profile| {
            let parsed = nmprofile::parse_routes(text, family)?;
            edit_ip(profile, family, &|ip| ip.routes = parsed.clone());
            Ok(())
        },
    );
    section
}

pub(super) fn edit_ip(profile: &mut Profile, family: Family, change: &dyn Fn(&mut nmprofile::Ip)) {
    let mut ip = profile.ip(family);
    change(&mut ip);
    profile.set_ip(family, &ip);
}
