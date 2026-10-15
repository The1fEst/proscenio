pub mod hiddennetwork;
pub mod hotspot;
pub mod savednetworks;

use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::core::i18n::tr;
use crate::panels::notifications::list::Placeholder;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::connectivity::network::connection::NEW_ENTERPRISE;
use crate::panels::wifinetwork::{NetworkList, Options};
use crate::services::network::wifi::Wifi;
use crate::ui::shapes::Shape;
use crate::ui::widgets::display::text;

const RESCAN_INTERVAL: Duration = Duration::from_secs(15);
const PLACEHOLDER_HEIGHT: i32 = 220;
const SEARCHING_MARGIN: i32 = 8;
const ITEM_HEIGHT: i32 = 56;
const PROMPT_HEIGHT: i32 = 110;

struct Rescanning(RefCell<Option<glib::SourceId>>);

impl Drop for Rescanning {
    fn drop(&mut self) {
        if let Some(source) = self.0.take() {
            source.remove();
        }
    }
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let wifi = Wifi::new();

    let controls = page.section("", "");
    let wifi_switch = page.switch(&controls, "wifi", &tr("Wi-Fi"), {
        let wifi = Rc::downgrade(&wifi);
        move |is_on| {
            if let Some(wifi) = wifi.upgrade() {
                wifi.set_enabled(is_on);
            }
        }
    });
    wifi_switch.bind({
        let wifi = Rc::downgrade(&wifi);
        move || {
            wifi.upgrade()
                .is_some_and(|wifi| wifi.state.borrow().is_enabled)
        }
    });
    page.link_row(
        &controls,
        "bookmark",
        &tr("Saved Networks"),
        "",
        context.subpage_opener("savednetworks"),
    );
    page.link_row(
        &controls,
        "wifi_password",
        &tr("Connect to Hidden Network…"),
        "",
        context.subpage_opener("hiddennetwork"),
    );
    page.link_row(
        &controls,
        "wifi_tethering",
        &tr("Hotspot"),
        "",
        context.subpage_opener("hotspot"),
    );

    let missing_placeholder = placeholder(
        &page,
        &tr("No Wi-Fi Found"),
        &tr("Plug in an adapter and make sure NetworkManager is running"),
    );
    let off_placeholder = placeholder(&page, &tr("Wi-Fi Off"), &tr("Turn on to use Wi-Fi"));

    let (networks_section, loading_indicator) =
        page.busy_section("wifi_find", &tr("Visible Networks"));
    let searching_label = text::styled(&tr("Searching for networks…"));
    text::set_color(&searching_label, "colSubtext");
    searching_label.set_xalign(0.0);
    searching_label.set_margin_start(SEARCHING_MARGIN);
    networks_section.append(&searching_label);
    let open = context.subpage_opener_with("connection");
    let list = NetworkList::new(
        &page.theme,
        &wifi,
        Options {
            shows_actions: true,
            heights: Some((ITEM_HEIGHT, PROMPT_HEIGHT)),
        },
        move |ssid| open(&format!("{NEW_ENTERPRISE}{ssid}")),
    );
    networks_section.append(&list.root);

    let rescan_timer = Rc::new(Rescanning(RefCell::new(None)));
    let show_state = {
        let wifi = Rc::downgrade(&wifi);
        let controls_card = controls.parent();
        let networks_card = networks_section.parent();
        let rescan_timer = Rc::downgrade(&rescan_timer);
        move || {
            let (Some(wifi), Some(rescan_timer)) = (wifi.upgrade(), rescan_timer.upgrade()) else {
                return;
            };
            let (is_available, is_enabled, is_scanning, is_empty) = {
                let state = wifi.state.borrow();
                (
                    state.is_available,
                    state.is_enabled,
                    state.is_scanning,
                    state.networks.is_empty(),
                )
            };
            if let Some(controls_card) = &controls_card {
                controls_card.set_visible(is_available);
            }
            missing_placeholder.set_visible(!is_available);
            off_placeholder.set_visible(is_available && !is_enabled);
            if let Some(networks_card) = &networks_card {
                networks_card.set_visible(is_enabled);
            }
            loading_indicator.area.set_visible(is_scanning);
            loading_indicator.set_loading(is_scanning);
            searching_label.set_visible(is_empty);
            wifi_switch.refresh();
            let is_rescan_running = rescan_timer.0.borrow().is_some();
            if is_enabled && !is_rescan_running {
                let weak_wifi = Rc::downgrade(&wifi);
                rescan_timer
                    .0
                    .replace(Some(glib::timeout_add_local(RESCAN_INTERVAL, move || {
                        if let Some(wifi) = weak_wifi.upgrade() {
                            wifi.rescan();
                        }
                        glib::ControlFlow::Continue
                    })));
                wifi.rescan();
            } else if !is_enabled && let Some(source) = rescan_timer.0.take() {
                source.remove();
            }
        }
    };
    show_state();
    wifi.connect_changed(show_state);
    page.keep(rescan_timer);
    page.keep(list);
    page.keep(wifi);
    page
}

fn placeholder(page: &Page, title: &str, description: &str) -> gtk4::CenterBox {
    let placeholder = Placeholder::build(
        &page.theme,
        "signal_wifi_off",
        Some(title),
        Some(description),
        Shape::Clover4Leaf,
    );
    let holder = gtk4::CenterBox::new();
    holder.set_orientation(gtk4::Orientation::Vertical);
    holder.set_size_request(-1, PLACEHOLDER_HEIGHT);
    holder.set_center_widget(Some(&placeholder.widget));
    holder.set_visible(false);
    page.append(&holder);
    page.keep(placeholder);
    holder
}
