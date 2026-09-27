use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::panels::notifications::list::Placeholder;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::network::manager_running;
use crate::services::wifi::{Saved, Wifi};
use crate::ui::shapes::Shape;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::{ConfigSwitch, icon_button};
use crate::ui::widgets::row::Row;
use crate::ui::widgets::text;

const PLACEHOLDER_HEIGHT: i32 = 220;
const ROW_HEIGHT: i32 = 56;
const ROW_START: i32 = 12;
const ROW_END: i32 = 8;
const ROW_SPACING: i32 = 10;

type Shown = Option<Vec<Saved>>;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !manager_running(&page, &status) {
        return page;
    }
    if let Some(section) = status.parent() {
        section.set_visible(false);
    }
    let wifi = Wifi::new();

    let placeholder = Placeholder::build(
        &page.theme,
        "wifi_off",
        Some("No Saved Networks"),
        Some("Saved Wi-Fi networks will appear here"),
        Shape::Clover4Leaf,
    );
    let empty = gtk4::CenterBox::new();
    empty.set_orientation(gtk4::Orientation::Vertical);
    empty.set_size_request(-1, PLACEHOLDER_HEIGHT);
    empty.set_center_widget(Some(&placeholder.widget));
    page.append(&empty);
    page.keep(placeholder);

    let rows = page.section("", "");
    let section = rows.parent();
    let shown: RefCell<Shown> = RefCell::new(None);
    let held: RefCell<Vec<Box<dyn Any>>> = RefCell::new(Vec::new());
    let follow = {
        let wifi = Rc::downgrade(&wifi);
        let page = Rc::downgrade(&page);
        move || {
            let (Some(wifi), Some(page)) = (wifi.upgrade(), page.upgrade()) else {
                return;
            };
            let saved = wifi.state.borrow().saved.clone();
            if shown.borrow().as_ref() == Some(&saved) {
                return;
            }
            shown.replace(Some(saved.clone()));
            empty.set_visible(saved.is_empty());
            if let Some(section) = &section {
                section.set_visible(!saved.is_empty());
            }
            while let Some(child) = rows.first_child() {
                rows.remove(&child);
            }
            let mut kept = held.borrow_mut();
            kept.clear();
            for network in &saved {
                kept.extend(row(&page, &wifi, &rows, network));
            }
        }
    };
    follow();
    wifi.connect_changed(follow);
    page.keep(wifi);
    page
}

fn row(page: &Page, wifi: &Rc<Wifi>, parent: &gtk4::Box, network: &Saved) -> Vec<Box<dyn Any>> {
    let connected = network.active;
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, ROW_HEIGHT);
    let inside = Row::new(ROW_SPACING);
    inside.set_margin_start(ROW_START);
    inside.set_margin_end(ROW_END);
    inside.set_hexpand(true);

    let symbol = text::symbol(
        if connected { "wifi" } else { "wifi_lock" },
        pixel_size::LARGER as f64,
    );
    text::set_color(&symbol, "colOnLayer2");
    inside.append(&Centred::integral(&symbol));

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let name = text::styled(&network.name);
    text::set_color(&name, "colOnLayer2");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centred::filling_width(&name));
    let status = text::styled_sized(
        if connected {
            "Connected"
        } else if network.automatic {
            "Joins on its own"
        } else {
            "Only when chosen"
        },
        pixel_size::SMALLER,
    );
    text::set_color(&status, "colSubtext");
    status.set_xalign(0.0);
    lines.append(&Centred::filling_width(&status));
    inside.append(&lines);

    let uuid = network.uuid.clone();
    let automatic = ConfigSwitch::new(&page.theme, "autorenew", "Automatic", {
        let wifi = Rc::downgrade(wifi);
        let uuid = uuid.clone();
        move |wanted| {
            if let Some(wifi) = wifi.upgrade() {
                wifi.set_automatic(&uuid, wanted);
            }
        }
    });
    automatic.button.set_hexpand(true);
    automatic.set(network.automatic);
    inside.append(&automatic.button);
    let tip = page.unkept_tip(
        &automatic.button,
        "Join this network whenever it is in range",
    );

    let (forget, _) = icon_button(&page.theme, "delete", true, "Forget");
    forget.connect_clicked({
        let wifi = Rc::downgrade(wifi);
        move |_| {
            if let Some(wifi) = wifi.upgrade() {
                wifi.forget(&uuid);
            }
        }
    });
    inside.append(&forget);
    card.append(&inside);
    parent.append(&card);
    vec![Box::new(automatic), Box::new(tip)]
}
