use gtk4::glib;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use crate::panels::notifications::list::Placeholder;
use crate::panels::settings::content::{Context, Page};
use crate::panels::wifinetwork::{NetworkList, Options};
use crate::services::wifi::Wifi;
use crate::ui::shapes::Shape;
use crate::ui::widgets::text;

const RESCAN: Duration = Duration::from_secs(15);
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
    let switch = page.switch(&controls, "wifi", "Wi-Fi", {
        let wifi = Rc::downgrade(&wifi);
        move |wanted| {
            if let Some(wifi) = wifi.upgrade() {
                wifi.enable(wanted);
            }
        }
    });
    switch.bind({
        let wifi = Rc::downgrade(&wifi);
        move || {
            wifi.upgrade()
                .is_some_and(|wifi| wifi.state.borrow().enabled)
        }
    });
    page.link_row(
        &controls,
        "bookmark",
        "Saved Networks",
        "",
        context.subpage_opener("savednetworks"),
    );
    page.link_row(
        &controls,
        "wifi_password",
        "Connect to Hidden Network…",
        "",
        context.subpage_opener("hiddennetwork"),
    );

    let missing = placeholder(
        &page,
        "No Wi-Fi Found",
        "Plug in an adapter and make sure NetworkManager is running",
    );
    let off = placeholder(&page, "Wi-Fi Off", "Turn on to use Wi-Fi");

    let (visible, busy) = page.busy_section("wifi_find", "Visible Networks");
    let searching = text::styled("Searching for networks…");
    text::set_color(&searching, "colSubtext");
    searching.set_xalign(0.0);
    searching.set_margin_start(SEARCHING_MARGIN);
    visible.append(&searching);
    let list = NetworkList::new(
        &page.theme,
        &wifi,
        Options {
            show_actions: true,
            height: Some((ITEM_HEIGHT, PROMPT_HEIGHT)),
        },
    );
    visible.append(&list.root);

    let rescanning = Rc::new(Rescanning(RefCell::new(None)));
    let follow = {
        let wifi = Rc::downgrade(&wifi);
        let controls = controls.parent();
        let visible_section = visible.parent();
        let rescanning = Rc::downgrade(&rescanning);
        move || {
            let (Some(wifi), Some(rescanning)) = (wifi.upgrade(), rescanning.upgrade()) else {
                return;
            };
            let (available, enabled, scanning, empty) = {
                let state = wifi.state.borrow();
                (
                    state.available,
                    state.enabled,
                    state.scanning,
                    state.networks.is_empty(),
                )
            };
            if let Some(controls) = &controls {
                controls.set_visible(available);
            }
            missing.set_visible(!available);
            off.set_visible(available && !enabled);
            if let Some(section) = &visible_section {
                section.set_visible(enabled);
            }
            busy.area.set_visible(scanning);
            busy.set_loading(scanning);
            searching.set_visible(empty);
            switch.refresh();
            let running = rescanning.0.borrow().is_some();
            if enabled && !running {
                let weak = Rc::downgrade(&wifi);
                rescanning
                    .0
                    .replace(Some(glib::timeout_add_local(RESCAN, move || {
                        if let Some(wifi) = weak.upgrade() {
                            wifi.rescan();
                        }
                        glib::ControlFlow::Continue
                    })));
                wifi.rescan();
            } else if !enabled && let Some(source) = rescanning.0.take() {
                source.remove();
            }
        }
    };
    follow();
    wifi.connect_changed(follow);
    page.keep(rescanning);
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
