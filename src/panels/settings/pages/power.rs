use gtk4::glib;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};
use std::time::Duration;

use crate::core::config;
use crate::core::i18n::tr;
use crate::core::tools;
use crate::panels::settings::content::{Choice, Context, Page, Parent};
use crate::platform::powersettings;
use crate::services::battery::{PROFILE_ON_BATTERY, PROFILE_ON_CHARGER};
use crate::services::idleoptions::IdleOptions;
use crate::ui::widgets::controls::ConfigSwitch;
use crate::ui::widgets::selection::Selection;
use crate::ui::widgets::spinbox::SpinBox;
use crate::ui::widgets::text;

const NOTE_START: i32 = 8;
const LIMIT_SETTLE: Duration = Duration::from_secs(1);
const POWER_PROFILES: &str = "net.hadess.PowerProfiles";
const ACTION_NAMES: [&str; 5] = ["Nothing", "Lock", "Suspend", "Hibernate", "Power off"];
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

    buttons_section(&page);

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
    charge_limit(&page, &battery);
    if tools::system_service(POWER_PROFILES) {
        profile_choice(
            &page,
            &battery,
            "Power profile on battery",
            PROFILE_ON_BATTERY,
            [
                ("Unchanged", ""),
                ("Power saver", "power-saver"),
                ("Balanced", "balanced"),
            ],
        );
        profile_choice(
            &page,
            &battery,
            "Power profile on the charger",
            PROFILE_ON_CHARGER,
            [
                ("Unchanged", ""),
                ("Balanced", "balanced"),
                ("Performance", "performance"),
            ],
        );
    }

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

fn note(color: &str) -> gtk4::Label {
    let label = text::styled("");
    text::set_color(&label, color);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_margin_start(NOTE_START);
    label.set_visible(false);
    label
}

fn show_result(label: &glib::WeakRef<gtk4::Label>, result: Result<(), String>) {
    if let Some(label) = label.upgrade() {
        let message = result.err().unwrap_or_default();
        label.set_visible(!message.is_empty());
        label.set_text(&message);
    }
}

fn buttons_section(page: &Rc<Page>) {
    let section = page.section("power_settings_new", &tr("Buttons & Lid"));
    let problem = note("colError");
    let page = Rc::downgrade(page);
    glib::spawn_future_local(async move {
        let buttons = powersettings::buttons().await;
        let Some(page) = page.upgrade() else {
            return;
        };
        let mut keys = vec![(powersettings::POWER_KEY, "Power button")];
        if buttons.lid {
            keys.extend(powersettings::LID_KEYS.iter().copied().zip([
                "Lid closed",
                "Lid closed on the charger",
                "Lid closed with an external display",
            ]));
        }
        for (key, title) in keys {
            let current = buttons.action(key);
            let mut choices: Vec<Choice> = powersettings::ACTIONS
                .iter()
                .zip(ACTION_NAMES)
                .filter(|(action, _)| {
                    **action != "hibernate" || buttons.hibernate || current == **action
                })
                .map(|(action, name)| Choice {
                    label: tr(name),
                    icon: "",
                    value: Value::from(*action),
                })
                .collect();
            if !powersettings::ACTIONS.contains(&current.as_str()) && !current.is_empty() {
                choices.push(Choice {
                    label: current.clone(),
                    icon: "",
                    value: Value::from(current.clone()),
                });
            }
            let group = page.subsection(&section, &tr(title), "");
            let holder: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
            let selection = Selection::new(&page.theme, choices, {
                let holder = holder.clone();
                let problem = problem.downgrade();
                move |value| {
                    let Some(selection) = holder.borrow().as_ref().and_then(Weak::upgrade) else {
                        return;
                    };
                    let action = value.as_str().unwrap_or("ignore").to_owned();
                    selection.set_current(&value);
                    let problem = problem.clone();
                    let selection = Rc::downgrade(&selection);
                    glib::spawn_future_local(async move {
                        let result = powersettings::set_button(key, &action).await;
                        let now = powersettings::buttons().await.action(key);
                        if let Some(selection) = selection.upgrade() {
                            selection.set_current(&Value::from(now));
                        }
                        show_result(&problem, result);
                    });
                }
            });
            selection.set_current(&Value::from(current));
            group.append(&selection.root);
            holder.replace(Some(Rc::downgrade(&selection)));
            page.keep(selection);
        }
        section.append(&problem);
    });
}

fn charge_limit(page: &Rc<Page>, parent: &gtk4::Box) {
    let Some((battery, limit)) = powersettings::battery_with_limit() else {
        return;
    };
    let spin = SpinBox::new(
        &page.theme,
        powersettings::LIMIT_RANGE.0,
        powersettings::LIMIT_RANGE.1,
        5,
        0,
    );
    spin.set_value(limit);
    let row = page.spin_row(
        parent,
        "battery_charging_80",
        &tr("Stop charging at (%)"),
        &spin,
    );
    page.tip(
        &row,
        &tr("A battery that stays plugged in lasts longer when it is not kept full"),
    );
    let problem = note("colError");
    parent.append(&problem);
    let queued: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    spin.connect_changed({
        let problem = problem.downgrade();
        move |percent| {
            if let Some(source) = queued.take() {
                source.remove();
            }
            let (battery, problem, pending) = (battery.clone(), problem.clone(), queued.clone());
            queued.set(Some(glib::timeout_add_local_once(
                LIMIT_SETTLE,
                move || {
                    pending.set(None);
                    glib::spawn_future_local(async move {
                        let result = powersettings::set_charge_limit(&battery, percent).await;
                        show_result(&problem, result);
                    });
                },
            )));
        }
    });
}

fn profile_choice<const N: usize>(
    page: &Page,
    parent: &gtk4::Box,
    title: &str,
    pointer: &'static str,
    options: [(&str, &str); N],
) {
    let group = page.subsection(
        parent,
        &tr(title),
        &tr("Switched to when the charger is plugged in or out"),
    );
    let choices = options
        .iter()
        .map(|(name, value)| Choice {
            label: tr(name),
            icon: "",
            value: Value::from(*value),
        })
        .collect();
    page.selection(&group, choices, pointer, Value::from(""), move |value| {
        config::store_value(pointer, value);
    });
}
