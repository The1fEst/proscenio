pub mod connection;
pub mod firewall;
pub mod proxy;

use gtk4::glib;
use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;

use crate::core::i18n::{tr, trf};
use crate::core::{process, tools};
use crate::panels::settings::content::{Context, Page, Parent};
use crate::panels::settings::pages::connectivity::network::connection::{
    NEW_BOND, NEW_BRIDGE, NEW_OPENVPN, NEW_VLAN, NEW_WIRED, NEW_WIREGUARD,
};
use crate::platform::network::firewall as platform_firewall;
use crate::platform::network::proxy as platform_proxy;
use crate::services::network::net::{Connection, Connections, VIRTUAL_KINDS, VPN_KINDS, WIRED};
use crate::services::network::nmsettings;
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::Switch;
use crate::ui::widgets::input::ripple::RippleButton;
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

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

pub fn check_manager_running(page: &Page, parent: &impl Parent) -> bool {
    if !page.tools_notice(
        parent,
        &[&tools::NMCLI],
        &tr("connections are not listed or switched"),
    ) {
        return false;
    }
    if tools::has_system_service(NETWORK_MANAGER) {
        return true;
    }
    page.notice(parent, "info", &tr(MANAGER_STOPPED));
    false
}

struct Group {
    rows: gtk4::Box,
    empty_label: gtk4::Label,
    owned_items: RefCell<Vec<Box<dyn Any>>>,
    last_list: RefCell<Option<Vec<Connection>>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !check_manager_running(&page, &status) {
        protection_links(&page, context);
        return page;
    }
    if let Some(section) = status.parent() {
        section.set_visible(false);
    }
    let connections = Connections::new();
    let open: Open = Rc::new(context.subpage_opener_with("connection"));

    let wired_section = page.section("lan", &tr("Wired"));
    let wired_group = group(&wired_section, &tr("No wired connection is set up"));
    let (add_wired_button, _) = page.icon_button("add", true, &tr("Add wired connection"), {
        let open = open.clone();
        move || open(NEW_WIRED)
    });
    add_wired_button.set_margin_top(BUTTON_TOP);
    wired_section.append(&add_wired_button);

    let vpn_section = page.section("vpn_key", &tr("VPN"));
    let vpn_group = group(&vpn_section, &tr("No VPN is set up"));
    let vpn_buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
    vpn_buttons.set_margin_top(BUTTON_TOP);
    let (add_wireguard_button, _) = page.icon_button("add", true, &tr("Add WireGuard"), {
        let open = open.clone();
        move || open(NEW_WIREGUARD)
    });
    vpn_buttons.append(&add_wireguard_button);
    let (add_openvpn_button, _) = page.icon_button("add", true, &tr("Add OpenVPN"), {
        let open = open.clone();
        move || open(NEW_OPENVPN)
    });
    if !Path::new(OPENVPN_PLUGIN).exists() {
        add_openvpn_button.set_sensitive(false);
        page.tip(
            &add_openvpn_button,
            &tr("OpenVPN needs the networkmanager-openvpn package installed"),
        );
    }
    vpn_buttons.append(&add_openvpn_button);
    let problem_label = text::styled("");
    text::set_color(&problem_label, "colError");
    problem_label.set_xalign(0.0);
    problem_label.set_wrap(true);
    problem_label.set_margin_start(EMPTY_MARGIN);
    problem_label.set_visible(false);
    let (import_button, _) = page.icon_button("file_open", false, &tr("Import from a file…"), {
        let open = open.clone();
        let problem_label = problem_label.downgrade();
        move || import_file(open.clone(), problem_label.clone())
    });
    vpn_buttons.append(&import_button);
    let missing_tools = tools::find_missing(&[&tools::KDIALOG]);
    if missing_tools.is_empty() {
        page.tip(
            &import_button,
            &tr("A WireGuard .conf file, or an OpenVPN .ovpn file with networkmanager-openvpn installed"),
        );
    } else {
        import_button.set_sensitive(false);
        page.tip(
            &import_button,
            &tools::missing_message(&missing_tools, &tr("there is no file picker")),
        );
    }
    vpn_section.append(&vpn_buttons);
    vpn_section.append(&problem_label);

    let virtual_section = page.section("device_hub", &tr("Virtual interfaces"));
    let virtual_group = group(&virtual_section, &tr("No VLAN, bridge or bond is set up"));
    let virtual_buttons = gtk4::Box::new(gtk4::Orientation::Horizontal, BUTTON_SPACING);
    virtual_buttons.set_margin_top(BUTTON_TOP);
    for (label, kind) in [
        ("Add VLAN", NEW_VLAN),
        ("Add bridge", NEW_BRIDGE),
        ("Add bond", NEW_BOND),
    ] {
        let (add_button, _) = page.icon_button("add", true, &tr(label), {
            let open = open.clone();
            move || open(kind)
        });
        virtual_buttons.append(&add_button);
    }
    virtual_section.append(&virtual_buttons);
    protection_links(&page, context);

    let show_connections = {
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
                .filter(|connection| connection.kind == WIRED && !connection.is_port)
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
            fill(&theme, &page, &connections, &open, &wired_group, wired_list);
            fill(&theme, &page, &connections, &open, &vpn_group, vpn_list);
            fill(
                &theme,
                &page,
                &connections,
                &open,
                &virtual_group,
                virtual_list,
            );
        }
    };
    show_connections();
    connections.connect_changed(show_connections);
    page.keep(connections);
    page
}

fn protection_links(page: &Page, context: &Context) {
    let section = page.section("security", &tr("Firewall & Proxy"));
    let firewall_status = if tools::find_missing(&[&tools::UFW]).is_empty() {
        if platform_firewall::read().is_enabled {
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
        &firewall_status,
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

fn import_file(open: Open, problem_label: glib::WeakRef<gtk4::Label>) {
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
        let is_openvpn = Path::new(&file)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("ovpn"));
        let import_result = if is_openvpn && !Path::new(OPENVPN_PLUGIN).exists() {
            Err(tr(
                "OpenVPN files need the networkmanager-openvpn package installed",
            ))
        } else {
            let kind = if is_openvpn { "openvpn" } else { "wireguard" };
            nmsettings::import(kind, &file).await
        };
        let Some(problem_label) = problem_label.upgrade() else {
            return;
        };
        match import_result {
            Ok(uuid) => {
                problem_label.set_visible(false);
                open(&uuid);
            }
            Err(message) => {
                problem_label.set_text(&message);
                problem_label.set_visible(true);
            }
        }
    });
}

fn group(section: &gtk4::Box, empty_text: &str) -> Rc<Group> {
    let empty_label = text::styled(empty_text);
    text::set_color(&empty_label, "colSubtext");
    empty_label.set_xalign(0.0);
    empty_label.set_margin_start(EMPTY_MARGIN);
    section.append(&empty_label);
    let rows = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    section.append(&rows);
    Rc::new(Group {
        rows,
        empty_label,
        owned_items: RefCell::new(Vec::new()),
        last_list: RefCell::new(None),
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
    if group.last_list.borrow().as_ref() == Some(&list) {
        return;
    }
    group.empty_label.set_visible(list.is_empty());
    group.rows.set_visible(!list.is_empty());
    while let Some(child) = group.rows.first_child() {
        group.rows.remove(&child);
    }
    let mut owned_items = group.owned_items.borrow_mut();
    owned_items.clear();
    for connection in &list {
        owned_items.extend(row(theme, page, connections, open, &group.rows, connection));
    }
    group.last_list.replace(Some(list));
}

pub fn edit_button(theme: &SharedTheme, open: &Open, uuid: &str) -> RippleButton {
    let button = RippleButton::new(theme);
    button.set_radius(EDIT_SIZE as f64 / 2.0);
    button.set_size_request(EDIT_SIZE, EDIT_SIZE);
    button.set_valign(gtk4::Align::Center);
    let symbol = text::symbol("edit", EDIT_ICON);
    text::set_color(&symbol, "colOnLayer2");
    button.set_content(&Centered::integral(&symbol), 0, 0);
    button.connect_clicked({
        let open = open.clone();
        let uuid = uuid.to_owned();
        move |_| open(&uuid)
    });
    button
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
    let content = Row::new(ROW_SPACING);
    content.set_margin_start(ROW_START);
    content.set_margin_end(ROW_END);
    content.set_hexpand(true);

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let name = text::styled(&connection.name);
    text::set_color(&name, "colOnLayer2");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centered::filling_width(&name));
    let status = text::styled_sized(
        &if connection.is_active {
            trf("Connected · %1", &[&connection.device])
        } else {
            tr("Not connected")
        },
        pixel_size::SMALLER,
    );
    text::set_color(&status, "colSubtext");
    status.set_xalign(0.0);
    status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centered::filling_width(&status));
    content.append(&lines);

    let edit_control = edit_button(theme, open, &connection.uuid);
    let tip = page.unkept_tip(&edit_control, &tr("Edit"));
    content.append(&edit_control);

    let active_switch = Switch::new(theme);
    active_switch.set(connection.is_active);
    active_switch.connect_clicked({
        let connections = Rc::downgrade(connections);
        let (uuid, is_active) = (connection.uuid.clone(), connection.is_active);
        move || {
            if let Some(connections) = connections.upgrade() {
                connections.set_active(&uuid, !is_active);
            }
        }
    });
    content.append(&active_switch.area);
    card.append(&content);
    parent.append(&card);
    vec![Box::new(active_switch), Box::new(tip)]
}
