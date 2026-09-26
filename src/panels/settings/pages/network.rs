use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::core::process::detach;
use crate::panels::settings::content::{Context, Page};
use crate::services::net::{Connection, Connections, VPN_KINDS, WIRED};
use crate::ui::theme::{SharedTheme, pixel_size};
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::Switch;
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;

const ROW_HEIGHT: i32 = 52;
const ROW_START: i32 = 12;
const ROW_END: i32 = 8;
const ROW_SPACING: i32 = 10;
const EMPTY_MARGIN: i32 = 8;
const BUTTON_TOP: i32 = 4;
const NETWORK_APP: &str = "kcmshell6 kcm_networkmanagement";

struct Group {
    rows: gtk4::Box,
    empty: gtk4::Label,
    held: RefCell<Vec<Box<dyn Any>>>,
    shown: RefCell<Option<Vec<Connection>>>,
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let connections = Connections::new();

    let wired_section = page.section("lan", "Wired");
    let wired = group(&wired_section, "No wired connection is set up");

    let vpn_section = page.section("vpn_key", "VPN");
    let vpn = group(&vpn_section, "No VPN is set up");
    let (set_up, _) = page.icon_button("settings_ethernet", true, "Set up connections", || {
        let command = config::value_str("/apps/network").unwrap_or_else(|| NETWORK_APP.to_owned());
        detach(&["bash", "-c", &command]);
    });
    set_up.set_margin_top(BUTTON_TOP);
    vpn_section.append(&set_up);
    page.tip(
        &set_up,
        "Adding and editing connections is NetworkManager's own job",
    );

    let follow = {
        let connections = Rc::downgrade(&connections);
        let theme = page.theme.clone();
        move || {
            let Some(connections) = connections.upgrade() else {
                return;
            };
            let list = connections.list.borrow().clone();
            let wired_list: Vec<Connection> = list
                .iter()
                .filter(|connection| connection.kind == WIRED)
                .cloned()
                .collect();
            let vpn_list: Vec<Connection> = list
                .iter()
                .filter(|connection| VPN_KINDS.contains(&connection.kind.as_str()))
                .cloned()
                .collect();
            fill(&theme, &connections, &wired, wired_list);
            fill(&theme, &connections, &vpn, vpn_list);
        }
    };
    follow();
    connections.connect_changed(follow);
    page.keep(connections);
    page
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

fn fill(theme: &SharedTheme, connections: &Rc<Connections>, group: &Group, list: Vec<Connection>) {
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
        held.push(Box::new(row(theme, connections, &group.rows, connection)));
    }
    group.shown.replace(Some(list));
}

fn row(
    theme: &SharedTheme,
    connections: &Rc<Connections>,
    parent: &gtk4::Box,
    connection: &Connection,
) -> Rc<Switch> {
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
            format!("Connected · {}", connection.device)
        } else {
            "Not connected".to_owned()
        },
        pixel_size::SMALLER,
    );
    text::set_color(&status, "colSubtext");
    status.set_xalign(0.0);
    status.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centred::filling_width(&status));
    inside.append(&lines);

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
    switch
}
