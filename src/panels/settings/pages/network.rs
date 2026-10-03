use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::core::{process, tools};
use crate::panels::settings::content::{Context, Page, Parent};
use crate::panels::settings::pages::connection::{
    NEW_BOND, NEW_BRIDGE, NEW_OPENVPN, NEW_VLAN, NEW_WIRED, NEW_WIREGUARD,
};
use crate::panels::settings::pages::proxy;
use crate::platform::firewall;
use crate::platform::proxy as platform_proxy;
use crate::services::net::{Connection, Connections, VIRTUAL_KINDS, VPN_KINDS, WIRED};
use crate::services::nmsettings;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::Switch;
use crate::ui::widgets::ripple::RippleButton;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;

const ROW_HEIGHT: i32 = 52;
const ROW_START: i32 = 12;
const ROW_END: i32 = 8;
const ROW_SPACING: i32 = 10;
const EMPTY_MARGIN: i32 = 8;
const BUTTON_TOP: i32 = 4;
const BUTTON_SPACING: i32 = 5;
const EDIT_SIZE: i32 = 32;
const EDIT_ICON: f64 = 20.0;
const NETWORK_MANAGER: &str = "org.freedesktop.NetworkManager";
const MANAGER_STOPPED: &str = "NetworkManager is not on the system bus, so connections are not listed or switched. Enable NetworkManager.service.";
const OPENVPN_PLUGIN: &str = "/usr/lib/NetworkManager/VPN/nm-openvpn-service.name";

type Open = Rc<dyn Fn(&str)>;

pub fn manager_running(page: &Page, parent: &impl Parent) -> bool {
    if !page.tools_notice(
        parent,
        &[&tools::NMCLI],
        &tr("connections are not listed or switched"),
    ) {
        return false;
    }
    if tools::system_service(NETWORK_MANAGER) {
        return true;
    }
    page.notice(parent, "info", &tr(MANAGER_STOPPED));
    false
}

struct Group {
    rows: gtk4::Box,
    empty: gtk4::Label,
    held: RefCell<Vec<Box<dyn Any>>>,
    shown: RefCell<Option<Vec<Connection>>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !manager_running(&page, &status) {
        protection_links(&page, context);
        return page;
    }
    if let Some(section) = status.parent() {
        section.set_visible(false);
    }
    let connections = Connections::new();
    let open: Open = Rc::new(context.subpage_opener_with("connection"));

    let wired_section = page.section("lan", &tr("Wired"));
    let wired = group(&wired_section, &tr("No wired connection is set up"));
    let (add_wired, _) = page.icon_button("add", true, &tr("Add wired connection"), {
        let open = open.clone();
        move || open(NEW_WIRED)
    });
    add_wired.set_margin_top(BUTTON_TOP);
    wired_section.append(&add_wired);

    let vpn_section = page.section("vpn_key", &tr("VPN"));
    let vpn = group(&vpn_section, &tr("No VPN is set up"));
    let adding = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
    adding.set_margin_top(BUTTON_TOP);
    let (add_wireguard, _) = page.icon_button("add", true, &tr("Add WireGuard"), {
        let open = open.clone();
        move || open(NEW_WIREGUARD)
    });
    adding.append(&add_wireguard);
    let (add_openvpn, _) = page.icon_button("add", true, &tr("Add OpenVPN"), {
        let open = open.clone();
        move || open(NEW_OPENVPN)
    });
    if !Path::new(OPENVPN_PLUGIN).exists() {
        add_openvpn.set_sensitive(false);
        page.tip(
            &add_openvpn,
            &tr("OpenVPN needs the networkmanager-openvpn package installed"),
        );
    }
    adding.append(&add_openvpn);
    let problem = text::styled("");
    text::set_color(&problem, "colError");
    problem.set_xalign(0.0);
    problem.set_wrap(true);
    problem.set_margin_start(EMPTY_MARGIN);
    problem.set_visible(false);
    let (import, _) = page.icon_button("file_open", false, &tr("Import from a file…"), {
        let open = open.clone();
        let problem = problem.downgrade();
        move || import_file(open.clone(), problem.clone())
    });
    adding.append(&import);
    let missing = tools::missing(&[&tools::KDIALOG]);
    if missing.is_empty() {
        page.tip(
            &import,
            &tr("A WireGuard .conf file, or an OpenVPN .ovpn file with networkmanager-openvpn installed"),
        );
    } else {
        import.set_sensitive(false);
        page.tip(
            &import,
            &tools::missing_message(&missing, &tr("there is no file picker")),
        );
    }
    vpn_section.append(&adding);
    vpn_section.append(&problem);

    let virtual_section = page.section("device_hub", &tr("Virtual interfaces"));
    let virtuals = group(&virtual_section, &tr("No VLAN, bridge or bond is set up"));
    let creating = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
    creating.set_margin_top(BUTTON_TOP);
    for (label, kind) in [
        ("Add VLAN", NEW_VLAN),
        ("Add bridge", NEW_BRIDGE),
        ("Add bond", NEW_BOND),
    ] {
        let (add, _) = page.icon_button("add", true, &tr(label), {
            let open = open.clone();
            move || open(kind)
        });
        creating.append(&add);
    }
    virtual_section.append(&creating);
    protection_links(&page, context);

    let follow = {
        let connections = Rc::downgrade(&connections);
        let theme = page.theme.clone();
        let page = Rc::downgrade(&page);
        move || {
            let (Some(connections), Some(page)) = (connections.upgrade(), page.upgrade()) else {
                return;
            };
            let list = connections.list.borrow().clone();
            let wired_list: Vec<Connection> = list
                .iter()
                .filter(|connection| connection.kind == WIRED && !connection.port)
                .cloned()
                .collect();
            let vpn_list: Vec<Connection> = list
                .iter()
                .filter(|connection| VPN_KINDS.contains(&connection.kind.as_str()))
                .cloned()
                .collect();
            let virtual_list: Vec<Connection> = list
                .iter()
                .filter(|connection| VIRTUAL_KINDS.contains(&connection.kind.as_str()))
                .cloned()
                .collect();
            fill(&theme, &page, &connections, &open, &wired, wired_list);
            fill(&theme, &page, &connections, &open, &vpn, vpn_list);
            fill(&theme, &page, &connections, &open, &virtuals, virtual_list);
        }
    };
    follow();
    connections.connect_changed(follow);
    page.keep(connections);
    page
}

fn protection_links(page: &Page, context: &Context) {
    let section = page.section("security", &tr("Firewall & Proxy"));
    let firewall = if tools::missing(&[&tools::UFW]).is_empty() {
        if firewall::read().enabled {
            tr("On")
        } else {
            tr("Off")
        }
    } else {
        tr("Not installed")
    };
    page.link_row(
        &section,
        "shield",
        &tr("Firewall"),
        &firewall,
        context.subpage_opener("firewall"),
    );
    page.link_row(
        &section,
        "travel_explore",
        &tr("Proxy"),
        &proxy::mode_name(platform_proxy::read().mode),
        context.subpage_opener("proxy"),
    );
}

fn import_file(open: Open, problem: glib::WeakRef<gtk4::Label>) {
    glib::spawn_future_local(async move {
        let home = glib::home_dir().to_string_lossy().into_owned();
        let picker = process::command(&[
            "kdialog",
            "--getopenfilename",
            &home,
            &format!("*.conf *.ovpn|{}", tr("WireGuard and OpenVPN files")),
            "--title",
            &tr("Import a VPN"),
        ]);
        let Some(file) = process::capture_text(picker)
            .await
            .map(|path| path.trim().to_owned())
            .filter(|path| !path.is_empty())
        else {
            return;
        };
        let openvpn = Path::new(&file)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ovpn"));
        let imported = if openvpn && !Path::new(OPENVPN_PLUGIN).exists() {
            Err(tr(
                "OpenVPN files need the networkmanager-openvpn package installed",
            ))
        } else {
            let kind = if openvpn { "openvpn" } else { "wireguard" };
            nmsettings::import(kind, &file).await
        };
        let Some(problem) = problem.upgrade() else {
            return;
        };
        match imported {
            Ok(uuid) => {
                problem.set_visible(false);
                open(&uuid);
            }
            Err(message) => {
                problem.set_text(&message);
                problem.set_visible(true);
            }
        }
    });
}

fn group(section: &gtk4::Box, empty_text: &str) -> Rc<Group> {
    let empty = text::styled(empty_text);
    text::set_color(&empty, "colSubtext");
    empty.set_xalign(0.0);
    empty.set_margin_start(EMPTY_MARGIN);
    section.append(&empty);
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    section.append(&rows);
    Rc::new(Group {
        rows,
        empty,
        held: RefCell::new(Vec::new()),
        shown: RefCell::new(None),
    })
}

fn fill(
    theme: &SharedTheme,
    page: &Page,
    connections: &Rc<Connections>,
    open: &Open,
    group: &Group,
    list: Vec<Connection>,
) {
    if group.shown.borrow().as_ref() == Some(&list) {
        return;
    }
    group.empty.set_visible(list.is_empty());
    group.rows.set_visible(!list.is_empty());
    while let Some(child) = group.rows.first_child() {
        group.rows.remove(&child);
    }
    let mut held = group.held.borrow_mut();
    held.clear();
    for connection in &list {
        held.extend(row(theme, page, connections, open, &group.rows, connection));
    }
    group.shown.replace(Some(list));
}

pub fn edit_button(theme: &SharedTheme, open: &Open, uuid: &str) -> RippleButton {
    let edit = RippleButton::new(theme);
    edit.set_radius(EDIT_SIZE as f64 / 2.0);
    edit.set_size_request(EDIT_SIZE, EDIT_SIZE);
    edit.set_valign(gtk4::Align::Center);
    let symbol = text::symbol("edit", EDIT_ICON);
    text::set_color(&symbol, "colOnLayer2");
    edit.set_content(&Centred::integral(&symbol), 0, 0);
    edit.connect_clicked({
        let open = open.clone();
        let uuid = uuid.to_owned();
        move |_| open(&uuid)
    });
    edit
}

fn row(
    theme: &SharedTheme,
    page: &Page,
    connections: &Rc<Connections>,
    open: &Open,
    parent: &gtk4::Box,
    connection: &Connection,
) -> Vec<Box<dyn Any>> {
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, ROW_HEIGHT);
    let inside = Row::new(ROW_SPACING);
    inside.set_margin_start(ROW_START);
    inside.set_margin_end(ROW_END);
    inside.set_hexpand(true);

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let name = text::styled(&connection.name);
    text::set_color(&name, "colOnLayer2");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centred::filling_width(&name));
    let status = text::styled_sized(
        &if connection.active {
            trf("Connected · %1", &[&connection.device])
        } else {
            tr("Not connected")
        },
        pixel_size::SMALLER,
    );
    text::set_color(&status, "colSubtext");
    status.set_xalign(0.0);
    status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centred::filling_width(&status));
    inside.append(&lines);

    let edit = edit_button(theme, open, &connection.uuid);
    let tip = page.unkept_tip(&edit, &tr("Edit"));
    inside.append(&edit);

    let switch = Switch::new(theme);
    switch.set(connection.active);
    switch.connect_clicked({
        let connections = Rc::downgrade(connections);
        let (uuid, active) = (connection.uuid.clone(), connection.active);
        move || {
            if let Some(connections) = connections.upgrade() {
                connections.activate(&uuid, !active);
            }
        }
    });
    inside.append(&switch.area);
    card.append(&inside);
    parent.append(&card);
    vec![Box::new(switch), Box::new(tip)]
}
