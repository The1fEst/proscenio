use gtk4::prelude::*;
use serde_json::Value;
use std::any::Any;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::i18n::tr;
use crate::core::{config, watch};
use crate::panels::settings::content::{Context, Page};
use crate::platform::hypr;
use crate::services::notifications;
use crate::ui::theme::pixel_size;
use crate::ui::widgets::centred::Centred;
use crate::ui::widgets::controls::ConfigSwitch;
use crate::ui::widgets::text;

const NOTE_START: i32 = 8;
const CARD_HEIGHT: i32 = 52;
const CARD_START: i32 = 12;
const CARD_END: i32 = 4;
const CARD_SPACING: i32 = 10;
const ICON: i32 = 24;
const SWITCHES_WIDTH: i32 = 300;

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);

    let main = page.section("", "");
    let notifications = context.services.notifications.clone();
    let silent = page.switch(&main, "notifications_paused", &tr("Do not disturb"), {
        let notifications = notifications.clone();
        move |on| notifications.set_silent(on)
    });
    silent.bind(move || notifications.silent.get());
    page.refresh_on("/notifications/silent", &silent);
    page.tip(
        &silent.button,
        &tr("Notifications still arrive and are kept; they just do not pop up."),
    );

    let sharing = page.config_switch(
        &main,
        "screen_share",
        &tr("Hide popups while sharing the screen"),
        notifications::HIDE_WHILE_SHARING,
        true,
    );
    page.tip(
        &sharing.button,
        &tr("While an app shares the screen, notifications go to the sidebar without popping up"),
    );

    let (timeout, _) = page.config_spin(
        &main,
        "av_timer",
        &tr("Stays on screen for (ms)"),
        "/notifications/timeout",
        config::NOTIFICATION_TIMEOUT,
        (1000, 60000),
        1000,
    );
    page.tip(
        &timeout,
        &tr("Used for notifications that do not ask for a time of their own"),
    );

    let placement = page.subsection(&main, &tr("Placement"), "");
    let forced = page.config_switch(
        &placement,
        "monitor",
        &tr("Always on one display"),
        "/notifications/forceMonitor/enable",
        false,
    );
    page.tip(
        &forced.button,
        &tr("With multiple monitors, keeps notifications on the one picked below"),
    );

    let listed = Rc::new(RefCell::new(Vec::new()));
    let combo = page.combo(&placement, "monitor");
    let show = Rc::new({
        let combo = Rc::downgrade(&combo);
        let listed = listed.clone();
        move || {
            let Some(combo) = combo.upgrade() else {
                return;
            };
            let monitors = monitors();
            let chosen = config::value_str("/notifications/forceMonitor/name");
            let index = monitors
                .iter()
                .position(|(_, name)| Some(name) == chosen.as_ref())
                .unwrap_or(0);
            let names: Vec<String> = monitors.iter().map(|(label, _)| label.clone()).collect();
            combo.set_items(&names, index as i32);
            combo.set_enabled(config::value_bool(
                "/notifications/forceMonitor/enable",
                false,
            ));
            listed.replace(monitors);
        }
    });
    show();
    combo.connect_activated(move |index| {
        if let Some((_, name)) = listed.borrow().get(index) {
            config::store_value(
                "/notifications/forceMonitor/name",
                Value::from(name.as_str()),
            );
        }
    });
    page.watch("/notifications/forceMonitor", {
        let show = show.clone();
        move || show()
    });
    page.keep(context.services.events.subscribe(move |event, _| {
        if matches!(
            event,
            "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
        ) {
            show();
        }
    }));

    applications(context, &page);

    let osd = page.section("voting_chip", &tr("On-screen display"));
    page.config_spin(
        &osd,
        "av_timer",
        &tr("Stays on screen for (ms)"),
        "/osd/timeout",
        config::OSD_TIMEOUT,
        (100, 3000),
        100,
    );
    page
}

fn set_member(pointer: &'static str, app_name: &str, member: bool) {
    let mut names: Vec<String> = config::value(pointer)
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .filter_map(|name| name.as_str().map(str::to_owned))
        .filter(|name| name != app_name)
        .collect();
    if member {
        names.push(app_name.to_owned());
    }
    config::store_value(pointer, Value::from(names));
}

fn applications(context: &Context, page: &Rc<Page>) {
    let section = page.section("apps", &tr("Applications"));
    let list = gtk4::Box::new(gtk4::Orientation::Vertical, section.spacing());
    section.append(&list);
    let shown: RefCell<Option<Vec<String>>> = RefCell::new(None);
    let held: RefCell<Vec<Box<dyn Any>>> = RefCell::new(Vec::new());
    let fill = {
        let page = Rc::downgrade(page);
        move || {
            let Some(page) = page.upgrade() else {
                return;
            };
            let apps = notifications::known_apps();
            if shown.borrow().as_ref() == Some(&apps) {
                return;
            }
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let mut kept = held.borrow_mut();
            kept.clear();
            if apps.is_empty() {
                let empty =
                    text::styled(&tr("Apps show up here once they have sent a notification"));
                text::set_color(&empty, "colSubtext");
                empty.set_xalign(0.0);
                empty.set_margin_start(NOTE_START);
                list.append(&empty);
            }
            for app_name in &apps {
                list.append(&app_card(&page, app_name, &mut kept));
            }
            shown.replace(Some(apps));
        }
    };
    fill();
    page.keep(context.services.notifications.subscribe(fill));
    let note = text::styled_sized(
        &tr(
            "Pop up off keeps the notifications in the sidebar only; Keep off drops them once their popup ends",
        ),
        pixel_size::SMALLER,
    );
    text::set_color(&note, "colSubtext");
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_margin_start(NOTE_START);
    section.append(&note);
}

fn app_card(page: &Page, app_name: &str, kept: &mut Vec<Box<dyn Any>>) -> gtk4::Box {
    let card = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
    card.add_css_class("settings-row-card");
    card.set_size_request(-1, CARD_HEIGHT);
    let inside = gtk4::Box::new(gtk4::Orientation::Horizontal, CARD_SPACING);
    inside.set_margin_start(CARD_START);
    inside.set_margin_end(CARD_END);
    inside.set_hexpand(true);
    let icon = notifications::app_icon_name(app_name);
    let image = if icon.starts_with('/') {
        gtk4::Image::from_file(&icon)
    } else if icon.is_empty() {
        gtk4::Image::from_icon_name("dialog-information")
    } else {
        gtk4::Image::from_icon_name(&icon)
    };
    image.set_pixel_size(ICON);
    inside.append(&image);
    let name = text::styled(app_name);
    text::set_color(&name, "colOnLayer2");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let name = Centred::filling_width(&name);
    name.set_hexpand(true);
    inside.append(&name);
    let controls = gtk4::Box::new(gtk4::Orientation::Horizontal, CARD_SPACING);
    controls.set_homogeneous(true);
    controls.set_size_request(SWITCHES_WIDTH, -1);
    controls.set_hexpand(false);
    for (label, pointer) in [
        (tr("Pop up"), notifications::QUIET_APPS),
        (tr("Keep"), notifications::FORGOTTEN_APPS),
    ] {
        let app = app_name.to_owned();
        let switch = ConfigSwitch::new(&page.theme, "", &label, move |on| {
            set_member(pointer, &app, !on);
        });
        let app = app_name.to_owned();
        switch.bind(move || !notifications::app_in(pointer, &app));
        controls.append(&switch.button);
        let weak = Rc::downgrade(&switch);
        kept.push(Box::new(watch::config(pointer, move || {
            if let Some(switch) = weak.upgrade() {
                switch.refresh();
            }
        })));
        kept.push(Box::new(switch));
    }
    inside.append(&controls);
    card.append(&inside);
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
