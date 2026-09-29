use gtk4::prelude::*;
use std::rc::Rc;

use crate::core::config;
use crate::core::i18n::tr;
use crate::core::tools;
use crate::panels::settings::content::{Context, Page, Parent};
use crate::services::idleoptions::IdleOptions;
use crate::ui::widgets::controls::ConfigSwitch;
use crate::ui::widgets::spinbox::SpinBox;

const AUTOMATIC_SUSPEND: &str = "/battery/automaticSuspend";
const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_MISSING: &str = "UPower is not running, so the battery level is unknown and none of these fire. It comes with the upower package.";
const INHIBIT_KEYS: [&str; 3] = [
    "ignore_dbus_inhibit",
    "ignore_systemd_inhibit",
    "ignore_wayland_inhibit",
];

pub fn hypridle_available(page: &Page, parent: &impl Parent) -> bool {
    page.tools_notice(
        parent,
        &[&tools::HYPRIDLE],
        &tr("the session never blanks, locks or suspends on its own"),
    )
}

pub fn general_switch(
    page: &Page,
    parent: &impl Parent,
    options: &Rc<IdleOptions>,
    icon: &str,
    label: &str,
    read: impl Fn(&IdleOptions) -> bool + Clone + 'static,
    write: impl Fn(&Rc<IdleOptions>, bool) + 'static,
) -> Rc<ConfigSwitch> {
    let current = {
        let options = Rc::downgrade(options);
        move || options.upgrade().is_some_and(|options| read(&options))
    };
    let switch = page.switch(parent, icon, &tr(label), {
        let options = Rc::downgrade(options);
        let current = current.clone();
        move |wanted| {
            if current() == wanted {
                return;
            }
            if let Some(options) = options.upgrade() {
                write(&options, wanted);
            }
        }
    });
    switch.bind(current);
    options.connect_changed({
        let switch = Rc::downgrade(&switch);
        move || {
            if let Some(switch) = switch.upgrade() {
                switch.refresh();
            }
        }
    });
    switch
}

pub struct IdleTimeout {
    pub what: &'static str,
    pub title: &'static str,
    pub tip: &'static str,
    pub switch_icon: &'static str,
    pub switch_text: &'static str,
    pub fallback_minutes: i64,
}

pub fn idle_timeout_row(
    page: &Page,
    parent: &impl Parent,
    options: &Rc<IdleOptions>,
    row: &IdleTimeout,
) {
    let group = page.subsection(parent, &tr(row.title), &tr(row.tip));
    let line = page.row(&group);
    line.set_halign(gtk4::Align::Start);
    let what = row.what;
    let minutes = {
        let options = Rc::downgrade(options);
        move || {
            options.upgrade().map_or(0, |options| {
                (options.seconds(what) as f64 / 60.0).round() as i64
            })
        }
    };
    let fallback = row.fallback_minutes;
    let switch = page.switch(&line, row.switch_icon, &tr(row.switch_text), {
        let options = Rc::downgrade(options);
        let minutes = minutes.clone();
        move |wanted| {
            if (minutes() > 0) == wanted {
                return;
            }
            if let Some(options) = options.upgrade() {
                options.set(what, if wanted { fallback * 60 } else { 0 });
            }
        }
    });
    switch.bind({
        let minutes = minutes.clone();
        move || minutes() > 0
    });
    let spin = SpinBox::new(&page.theme, 1, 600, 5, 0);
    spin.connect_changed({
        let options = Rc::downgrade(options);
        move |value| {
            if let Some(options) = options.upgrade() {
                options.set(what, value * 60);
            }
        }
    });
    let spin_line = page.spin_row(&line, "timer", &tr("after (min)"), &spin);
    let follow = move || {
        let current = minutes();
        switch.refresh();
        spin.set_value(current);
        Page::set_spin_row_enabled(&spin_line, &spin, current > 0);
    };
    follow();
    options.connect_changed(follow);
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = IdleOptions::new();

    let saving = page.section("energy_savings_leaf", &tr("Power Saving"));
    let idle = hypridle_available(&page, &saving);
    if idle {
        idle_timeout_row(
            &page,
            &saving,
            &options,
            &IdleTimeout {
                what: "screen",
                title: "Automatic Screen Blank",
                tip: "Turns the screens off after a period of inactivity",
                switch_icon: "brightness_low",
                switch_text: "Blank the screen",
                fallback_minutes: 15,
            },
        );
        let inhibit = page.row(&saving);
        inhibit.set_halign(gtk4::Align::Start);
        let apps = general_switch(
            &page,
            &inhibit,
            &options,
            "smart_display",
            "Apps can keep the screen on",
            |options| {
                INHIBIT_KEYS
                    .iter()
                    .all(|key| options.general(key).as_deref() != Some("true"))
            },
            |options, on| {
                for key in INHIBIT_KEYS {
                    options.set_general(key, if on { None } else { Some("true") });
                }
            },
        );
        page.tip(
            &apps.button,
            &tr(
                "Video players, calls and games can hold off blanking, locking and suspend while they play",
            ),
        );
    }

    let battery = page.section("battery_android_full", &tr("Battery"));
    if !tools::system_service(UPOWER) {
        page.notice(&battery, "info", &tr(UPOWER_MISSING));
    }
    let warnings = page.uniform_row(&battery);
    page.config_spin(
        &warnings,
        "warning",
        &tr("Low warning"),
        "/battery/low",
        20,
        (0, 100),
        5,
    );
    page.config_spin(
        &warnings,
        "dangerous",
        &tr("Critical warning"),
        "/battery/critical",
        5,
        (0, 100),
        5,
    );
    let suspend = page.row(&battery);
    suspend.set_halign(gtk4::Align::Start);
    let automatic = page.config_switch(
        &suspend,
        "pause",
        &tr("Automatic suspend"),
        AUTOMATIC_SUSPEND,
        true,
    );
    page.tip(
        &automatic.button,
        &tr("Automatically suspends the system when battery is low"),
    );
    let (at_row, at) =
        page.config_spin(&suspend, "", &tr("at"), "/battery/suspend", 3, (0, 100), 5);
    let follow = move || {
        Page::set_spin_row_enabled(&at_row, &at, config::value_bool(AUTOMATIC_SUSPEND, true));
    };
    follow();
    page.watch(AUTOMATIC_SUSPEND, follow);
    let full = page.uniform_row(&battery);
    page.config_spin(
        &full,
        "charger",
        &tr("Full warning"),
        "/battery/full",
        101,
        (0, 101),
        5,
    );

    if idle {
        let sleeping = page.section("bedtime", &tr("Automatic Suspend"));
        idle_timeout_row(
            &page,
            &sleeping,
            &options,
            &IdleTimeout {
                what: "suspend",
                title: "Suspend when idle",
                tip: "Turning automatic suspend off means the machine keeps drawing power while nobody is at it",
                switch_icon: "pause",
                switch_text: "Suspend",
                fallback_minutes: 45,
            },
        );
    }
    page.keep(options);
    page
}
