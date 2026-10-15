use serde_json::Value;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::connectivity::network::connection::{
    self, Editor, IPV4_METHODS, IPV6_METHODS, Part, choices,
};
use crate::platform::network::nmprofile::ipconfig::{self, Family};
use crate::platform::network::nmprofile::{self, Profile};

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
    let method_section = editor.subsection(page, &section, &tr("Method"), "");
    editor.choose(
        &method_section,
        options,
        Value::from(ip.method.clone()),
        move |profile, value| {
            let mut ip = profile.ip(family);
            ip.method = value.as_str().unwrap_or("auto").to_owned();
            profile.set_ip(family, &ip);
        },
    );
    let is_automatic = matches!(ip.method.as_str(), "auto" | "dhcp");
    let is_configured = is_automatic || matches!(ip.method.as_str(), "manual" | "shared");
    if !is_configured {
        return section;
    }
    if ip.method == "manual" {
        let addresses_section = editor.subsection(
            page,
            &section,
            &tr("Addresses"),
            &tr("Separated by commas, each with its prefix length, such as 192.168.1.10/24"),
        );
        editor.field(
            &addresses_section,
            &tr("Addresses"),
            &ipconfig::format_addresses(&ip.addresses),
            move |text, profile| {
                let addresses = ipconfig::parse_addresses(text, family)?;
                edit_ip(profile, family, &|ip| ip.addresses = addresses.clone());
                Ok(())
            },
        );
        editor.field(
            &addresses_section,
            &tr("Gateway"),
            &ip.gateway,
            move |text, profile| {
                let gateway = ipconfig::parse_gateway(text, family)?;
                edit_ip(profile, family, &|ip| ip.gateway = gateway.clone());
                Ok(())
            },
        );
    }
    let dns_section = editor.subsection(page, &section, &tr("DNS"), "");
    if is_automatic {
        editor.switch(
            &dns_section,
            "dns",
            &tr("Automatic DNS"),
            move |profile| profile.ip(family).uses_automatic_dns,
            move |profile, is_on| edit_ip(profile, family, &|ip| ip.uses_automatic_dns = is_on),
        );
    }
    editor.field(
        &dns_section,
        &tr("DNS servers"),
        &ip.dns.join(", "),
        move |text, profile| {
            let servers = ipconfig::parse_servers(text, family)?;
            edit_ip(profile, family, &|ip| ip.dns = servers.clone());
            Ok(())
        },
    );
    editor.field(
        &dns_section,
        &tr("Search domains"),
        &ip.search.join(", "),
        move |text, profile| {
            let domains = nmprofile::items(text);
            edit_ip(profile, family, &|ip| ip.search = domains.clone());
            Ok(())
        },
    );
    let routing_section = editor.subsection(page, &section, &tr("Routing"), "");
    if is_automatic {
        editor.switch(
            &routing_section,
            "route",
            &tr("Automatic routes"),
            move |profile| profile.ip(family).uses_automatic_routes,
            move |profile, is_on| edit_ip(profile, family, &|ip| ip.uses_automatic_routes = is_on),
        );
    }
    let own_network_switch = editor.switch(
        &routing_section,
        "alt_route",
        &tr("Only for its own network"),
        move |profile| profile.ip(family).is_never_default,
        move |profile, is_on| edit_ip(profile, family, &|ip| ip.is_never_default = is_on),
    );
    editor.hold(page.unkept_tip(
        &own_network_switch.button,
        &tr("Never the default route: only traffic for this network's addresses and routes goes through it"),
    ));
    let routes_section = editor.subsection(
        page,
        &section,
        &tr("Routes"),
        &tr("Separated by commas, written like ip route: 10.0.0.0/8 via 192.168.1.1 metric 100"),
    );
    editor.field(
        &routes_section,
        &tr("Routes"),
        &ipconfig::format_routes(&ip.routes),
        move |text, profile| {
            let routes = ipconfig::parse_routes(text, family)?;
            edit_ip(profile, family, &|ip| ip.routes = routes.clone());
            Ok(())
        },
    );
    section
}

pub(super) fn edit_ip(profile: &mut Profile, family: Family, change: &dyn Fn(&mut ipconfig::Ip)) {
    let mut ip = profile.ip(family);
    change(&mut ip);
    profile.set_ip(family, &ip);
}
