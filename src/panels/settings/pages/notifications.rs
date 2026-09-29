use serde_json::Value;
use std::cell::RefCell;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::platform::hypr;

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
