use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, watch};
use crate::panels::settings::content::{Context, Page};
use crate::platform::hyprland::hypr;
use crate::services::overlay::notifications;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ConfigSwitch;
use crate::ui::widgets::layout::centered::Centered;

const NOTE_START: i32 = 8;
const CARD_HEIGHT: i32 = 52;
const CARD_START: i32 = 12;
const CARD_END: i32 = 4;
const CARD_SPACING: i32 = 10;
const ICON: i32 = 24;
const SWITCHES_WIDTH: i32 = 300;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let main_section = page.section("", "");
    let notification_service = context.services.notifications.clone();
    let silent_switch = page.switch(
        &main_section,
        "notifications_paused",
        &tr("Do not disturb"),
        {
            let notification_service = notification_service.clone();
            move |is_on| notification_service.set_silent(is_on)
        },
    );
    silent_switch.bind(move || notification_service.is_silent.get());
    page.refresh_on("/notifications/silent", &silent_switch);
    page.tip(
        &silent_switch.button,
        &tr("Notifications still arrive and are kept; they just do not pop up."),
    );

    let sharing_switch = page.config_switch(
        &main_section,
        "screen_share",
        &tr("Hide popups while sharing the screen"),
        notifications::HIDE_WHILE_SHARING,
        true,
    );
    page.tip(
        &sharing_switch.button,
        &tr("While an app shares the screen, notifications go to the sidebar without popping up"),
    );

    let (timeout_row, _) = page.config_spin(
        &main_section,
        "av_timer",
        &tr("Stays on screen for (ms)"),
        "/notifications/timeout",
        config::NOTIFICATION_TIMEOUT_MILLIS,
        config::NOTIFICATION_TIMEOUT_RANGE_MILLIS,
        1000,
    );
    page.tip(
        &timeout_row,
        &tr("Used for notifications that do not ask for a time of their own"),
    );

    let placement_section = page.subsection(&main_section, &tr("Placement"), "");
    let force_monitor_switch = page.config_switch(
        &placement_section,
        "monitor",
        &tr("Always on one display"),
        "/notifications/forceMonitor/enable",
        false,
    );
    page.tip(
        &force_monitor_switch.button,
        &tr("With multiple monitors, keeps notifications on the one picked below"),
    );

    let monitor_entries = Rc::new(RefCell::new(Vec::new()));
    let monitor_combo = page.combo(&placement_section, "monitor");
    let update_monitor_combo = Rc::new({
        let monitor_combo = Rc::downgrade(&monitor_combo);
        let monitor_entries = monitor_entries.clone();
        move || {
            let Some(monitor_combo) = monitor_combo.upgrade() else {
                return;
            };
            let monitors = monitors();
            let selected_name = config::value_str("/notifications/forceMonitor/name");
            let index = monitors
                .iter()
                .position(|(_, name)| Some(name) == selected_name.as_ref())
                .unwrap_or(0);
            let labels: Vec<String> = monitors.iter().map(|(label, _)| label.clone()).collect();
            monitor_combo.set_items(&labels, index as i32);
            monitor_combo.set_enabled(config::value_bool(
                "/notifications/forceMonitor/enable",
                false,
            ));
            monitor_entries.replace(monitors);
        }
    });
    update_monitor_combo();
    monitor_combo.connect_activated(move |index| {
        if let Some((_, name)) = monitor_entries.borrow().get(index) {
            config::store_value(
                "/notifications/forceMonitor/name",
                Value::from(name.as_str()),
            );
        }
    });
    page.watch("/notifications/forceMonitor", {
        let update_monitor_combo = update_monitor_combo.clone();
        move || update_monitor_combo()
    });
    page.keep(context.services.events.subscribe(move |event, _| {
        if matches!(
            event,
            "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
        ) {
            update_monitor_combo();
        }
    }));

    applications(context, &page);

    let osd_section = page.section("voting_chip", &tr("On-screen display"));
    page.config_spin(
        &osd_section,
        "av_timer",
        &tr("Stays on screen for (ms)"),
        "/osd/timeout",
        config::OSD_TIMEOUT_MILLIS,
        (100, 3000),
        100,
    );
    page
}

fn set_member(pointer: &'static str, app_name: &str, is_member: bool) {
    let mut names: Vec<String> = config::value(pointer)
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .filter(|name| name != app_name)
        .collect();
    if is_member {
        names.push(app_name.to_owned());
    }
    config::store_value(pointer, Value::from(names));
}

fn applications(context: &Context, page: &Rc<Page>) {
    let applications_section = page.section("apps", &tr("Applications"));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, applications_section.spacing());
    applications_section.append(&list);
    let last_apps: RefCell<Option<Vec<String>>> = RefCell::new(None);
    let owned_items: RefCell<Vec<Box<dyn Any>>> = RefCell::new(Vec::new());
    let fill = {
        let page = Rc::downgrade(page);
        move || {
            let Some(page) = page.upgrade() else {
                return;
            };
            let apps = notifications::known_apps();
            if last_apps.borrow().as_ref() == Some(&apps) {
                return;
            }
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let mut items = owned_items.borrow_mut();
            items.clear();
            if apps.is_empty() {
                let empty_label =
                    text::styled(&tr("Apps show up here once they have sent a notification"));
                text::set_color(&empty_label, "colSubtext");
                empty_label.set_xalign(0.0);
                empty_label.set_margin_start(NOTE_START);
                list.append(&empty_label);
            }
            for app_name in &apps {
                list.append(&app_card(&page, app_name, &mut items));
            }
            last_apps.replace(Some(apps));
        }
    };
    fill();
    page.keep(context.services.notifications.subscribe(fill));
    let note_label = text::styled_sized(
        &tr(
            "Pop up off keeps the notifications in the sidebar only; Keep off drops them once their popup ends",
        ),
        pixel_size::SMALLER,
    );
    text::set_color(&note_label, "colSubtext");
    note_label.set_xalign(0.0);
    note_label.set_wrap(true);
    note_label.set_margin_start(NOTE_START);
    applications_section.append(&note_label);
}

fn app_card(page: &Page, app_name: &str, owned_items: &mut Vec<Box<dyn Any>>) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, CARD_HEIGHT);
    let content = gtk4::Box::new(gtk4::Orientation::Horizontal, CARD_SPACING);
    content.set_margin_start(CARD_START);
    content.set_margin_end(CARD_END);
    content.set_hexpand(true);
    let icon = notifications::app_icon_name(app_name);
    let image = if icon.starts_with('/') {
        gtk4::Image::from_file(&icon)
    } else if icon.is_empty() {
        gtk4::Image::from_icon_name("dialog-information")
    } else {
        gtk4::Image::from_icon_name(&icon)
    };
    image.set_pixel_size(ICON);
    content.append(&image);
    let name_label = text::styled(app_name);
    text::set_color(&name_label, "colOnLayer2");
    name_label.set_xalign(0.0);
    name_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let name_box = Centered::filling_width(&name_label);
    name_box.set_hexpand(true);
    content.append(&name_box);
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, CARD_SPACING);
    controls.set_homogeneous(true);
    controls.set_size_request(SWITCHES_WIDTH, -1);
    controls.set_hexpand(false);
    for (label, pointer) in [
        (tr("Pop up"), notifications::QUIET_APPS),
        (tr("Keep"), notifications::FORGOTTEN_APPS),
    ] {
        let app = app_name.to_owned();
        let switch = ConfigSwitch::new(&page.theme, "", &label, move |is_on| {
            set_member(pointer, &app, !is_on);
        });
        let app = app_name.to_owned();
        switch.bind(move || !notifications::is_app_listed(pointer, &app));
        controls.append(&switch.button);
        let weak_switch = Rc::downgrade(&switch);
        owned_items.push(Box::new(watch::config(pointer, move || {
            if let Some(switch) = weak_switch.upgrade() {
                switch.refresh();
            }
        })));
        owned_items.push(Box::new(switch));
    }
    content.append(&controls);
    card.append(&content);
    card
}

pub fn monitors() -> Vec<(String, String)> {
    hypr::json("monitors")
        .and_then(|monitors| monitors.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|monitor| {
            let name = monitor.get("name")?.as_str()?.to_owned();
            let model = monitor
                .get("model")
                .and_then(Value::as_str)
                .filter(|model| !model.is_empty())
                .unwrap_or(&name)
                .to_owned();
            Some((format!("{model} ({name})"), name))
        })
        .collect()
}
