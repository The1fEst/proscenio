use gtk4::prelude::*;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::notifications::list::Placeholder;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::connectivity::network::{check_manager_running, edit_button};
use crate::services::network::wifi::{SavedNetwork, Wifi};
use crate::ui::shapes::Shape;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::{ConfigSwitch, icon_button};
use crate::ui::widgets::layout::centered::Centered;
use crate::ui::widgets::layout::row::Row;

const PLACEHOLDER_HEIGHT: i32 = 220;
const ROW_HEIGHT: i32 = 56;
const ROW_START: i32 = 12;
const ROW_END: i32 = 8;
const ROW_SPACING: i32 = 10;

type Shown = Option<Vec<SavedNetwork>>;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let status = page.section("", "");
    if !check_manager_running(&page, &status) {
        return page;
    }
    if let Some(section) = status.parent() {
        section.set_visible(false);
    }
    let wifi = Wifi::new();
    let open: Rc<dyn Fn(&str)> = Rc::new(context.subpage_opener_with("connection"));

    let placeholder = Placeholder::build(
        &page.theme,
        "wifi_off",
        Some(&tr("No Saved Networks")),
        Some(&tr("Saved Wi-Fi networks will appear here")),
        Shape::Clover4Leaf,
    );
    let empty_view = gtk4::CenterBox::new();
    empty_view.set_orientation(gtk4::Orientation::Vertical);
    empty_view.set_size_request(-1, PLACEHOLDER_HEIGHT);
    empty_view.set_center_widget(Some(&placeholder.widget));
    page.append(&empty_view);
    page.keep(placeholder);

    let rows = page.section("", "");
    let section = rows.parent();
    let last_networks: RefCell<Shown> = RefCell::new(None);
    let owned_items: RefCell<Vec<Box<dyn Any>>> = RefCell::new(Vec::new());
    let show_networks = {
        let wifi = Rc::downgrade(&wifi);
        let page = Rc::downgrade(&page);
        move || {
            let (Some(wifi), Some(page)) = (wifi.upgrade(), page.upgrade()) else {
                return;
            };
            let saved_networks = wifi.state.borrow().saved_networks.clone();
            if last_networks.borrow().as_ref() == Some(&saved_networks) {
                return;
            }
            last_networks.replace(Some(saved_networks.clone()));
            empty_view.set_visible(saved_networks.is_empty());
            if let Some(section) = &section {
                section.set_visible(!saved_networks.is_empty());
            }
            while let Some(child) = rows.first_child() {
                rows.remove(&child);
            }
            let mut items = owned_items.borrow_mut();
            items.clear();
            for network in &saved_networks {
                items.extend(row(&page, &wifi, &open, &rows, network));
            }
        }
    };
    show_networks();
    wifi.connect_changed(show_networks);
    page.keep(wifi);
    page
}

fn row(
    page: &Page,
    wifi: &Rc<Wifi>,
    open: &Rc<dyn Fn(&str)>,
    parent: &gtk4::Box,
    network: &SavedNetwork,
) -> Vec<Box<dyn Any>> {
    let is_connected = network.is_active;
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, ROW_HEIGHT);
    let content = Row::new(ROW_SPACING);
    content.set_margin_start(ROW_START);
    content.set_margin_end(ROW_END);
    content.set_hexpand(true);

    let symbol = text::symbol(
        if is_connected { "wifi" } else { "wifi_lock" },
        pixel_size::LARGER as f64,
    );
    text::set_color(&symbol, "colOnLayer2");
    content.append(&Centered::integral(&symbol));

    let lines = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    lines.set_hexpand(true);
    lines.set_valign(gtk4::Align::Center);
    let name_label = text::styled(&network.name);
    text::set_color(&name_label, "colOnLayer2");
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    lines.append(&Centered::filling_width(&name_label));
    let status = text::styled_sized(
        &if is_connected {
            tr("Connected")
        } else if network.should_autoconnect {
            tr("Joins on its own")
        } else {
            tr("Only when chosen")
        },
        pixel_size::SMALLER,
    );
    text::set_color(&status, "colSubtext");
    status.set_xalign(0.0);
    lines.append(&Centered::filling_width(&status));
    content.append(&lines);

    let uuid = network.uuid.clone();
    let automatic_switch = ConfigSwitch::new(&page.theme, "autorenew", &tr("Automatic"), {
        let wifi = Rc::downgrade(wifi);
        let uuid = uuid.clone();
        move |is_on| {
            if let Some(wifi) = wifi.upgrade() {
                wifi.set_autoconnect(&uuid, is_on);
            }
        }
    });
    automatic_switch.button.set_hexpand(true);
    automatic_switch.set(network.should_autoconnect);
    content.append(&automatic_switch.button);
    let automatic_tip = page.unkept_tip(
        &automatic_switch.button,
        &tr("Join this network whenever it is in range"),
    );

    let edit_control = edit_button(&page.theme, open, &uuid);
    let edit_tip = page.unkept_tip(&edit_control, &tr("Edit"));
    content.append(&edit_control);

    let (forget_button, _) = icon_button(&page.theme, "delete", true, &tr("Forget"));
    forget_button.connect_clicked({
        let wifi = Rc::downgrade(wifi);
        move |_| {
            if let Some(wifi) = wifi.upgrade() {
                wifi.forget(&uuid);
            }
        }
    });
    content.append(&forget_button);
    card.append(&content);
    parent.append(&card);
    vec![
        Box::new(automatic_switch),
        Box::new(automatic_tip),
        Box::new(edit_tip),
    ]
}
