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
use crate::platform::dbus::powersettings;
use crate::services::hardware::battery::{PROFILE_ON_BATTERY, PROFILE_ON_CHARGER};
use crate::services::hyprland::idleoptions::IdleOptions;
use crate::ui::widgets::display::text;
use crate::ui::widgets::input::controls::ConfigSwitch;
use crate::ui::widgets::input::selection::Selection;
use crate::ui::widgets::input::spinbox::SpinBox;

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

pub fn check_hypridle_available(page: &Page, parent: &impl Parent) -> bool {
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
    let read_current = {
        let options = Rc::downgrade(options);
        move || options.upgrade().is_some_and(|options| read(&options))
    };
    let switch = page.switch(parent, icon, &tr(label), {
        let options = Rc::downgrade(options);
        let read_current = read_current.clone();
        move |is_on| {
            if read_current() == is_on {
                return;
            }
            if let Some(options) = options.upgrade() {
                write(&options, is_on);
            }
        }
    });
    switch.bind(read_current);
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

fn timeout_minutes(seconds: u64) -> Option<i64> {
    (seconds > 0).then(|| ((seconds as f64 / 60.0).round() as i64).max(1))
}

pub fn idle_timeout_row(
    page: &Page,
    parent: &impl Parent,
    options: &Rc<IdleOptions>,
    row: &IdleTimeout,
) {
    let group = page.subsection(parent, &tr(row.title), &tr(row.tip));
    let timeout_row = page.row(&group);
    timeout_row.set_halign(gtk4::Align::Start);
    let what = row.what;
    let read_minutes = {
        let options = Rc::downgrade(options);
        move || {
            options
                .upgrade()
                .and_then(|options| timeout_minutes(options.seconds(what)))
        }
    };
    let fallback = row.fallback_minutes;
    let switch = page.switch(&timeout_row, row.switch_icon, &tr(row.switch_text), {
        let options = Rc::downgrade(options);
        let read_minutes = read_minutes.clone();
        move |is_on| {
            if read_minutes().is_some() == is_on {
                return;
            }
            if let Some(options) = options.upgrade() {
                options.set(what, if is_on { fallback * 60 } else { 0 });
            }
        }
    });
    switch.bind({
        let read_minutes = read_minutes.clone();
        move || read_minutes().is_some()
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
    let minutes_row = page.spin_row(&timeout_row, "timer", &tr("after (min)"), &spin);
    let update_row = move || {
        let minutes = read_minutes();
        switch.refresh();
        spin.set_value(minutes.unwrap_or(0));
        Page::set_spin_row_enabled(&minutes_row, &spin, minutes.is_some());
    };
    update_row();
    options.connect_changed(update_row);
}

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let options = IdleOptions::new();

    let saving_section = page.section("energy_savings_leaf", &tr("Power Saving"));
    let is_hypridle_available = check_hypridle_available(&page, &saving_section);
    if is_hypridle_available {
        idle_timeout_row(
            &page,
            &saving_section,
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
        let inhibit_row = page.row(&saving_section);
        inhibit_row.set_halign(gtk4::Align::Start);
        let apps_switch = general_switch(
            &page,
            &inhibit_row,
            &options,
            "smart_display",
            "Apps can keep the screen on",
            |options| {
                INHIBIT_KEYS
                    .iter()
                    .all(|key| options.general(key).as_deref() != Some("true"))
            },
            |options, is_on| {
                for key in INHIBIT_KEYS {
                    options.set_general(key, if is_on { None } else { Some("true") });
                }
            },
        );
        page.tip(
            &apps_switch.button,
            &tr(
                "Video players, calls and games can hold off blanking, locking and suspend while they play",
            ),
        );
    }

    buttons_section(&page);

    let battery_section = page.section("battery_android_full", &tr("Battery"));
    if !tools::has_system_service(UPOWER) {
        page.notice(&battery_section, "info", &tr(UPOWER_MISSING));
    }
    let warnings_row = page.uniform_row(&battery_section);
    page.config_spin(
        &warnings_row,
        "warning",
        &tr("Low warning"),
        "/battery/low",
        20,
        (0, 100),
        5,
    );
    page.config_spin(
        &warnings_row,
        "dangerous",
        &tr("Critical warning"),
        "/battery/critical",
        5,
        (0, 100),
        5,
    );
    let suspend_row = page.row(&battery_section);
    suspend_row.set_halign(gtk4::Align::Start);
    let automatic_switch = page.config_switch(
        &suspend_row,
        "pause",
        &tr("Automatic suspend"),
        AUTOMATIC_SUSPEND,
        true,
    );
    page.tip(
        &automatic_switch.button,
        &tr("Automatically suspends the system when battery is low"),
    );
    let (at_row, at_spin) = page.config_spin(
        &suspend_row,
        "",
        &tr("at"),
        "/battery/suspend",
        3,
        (0, 100),
        5,
    );
    let update_at_row = move || {
        Page::set_spin_row_enabled(
            &at_row,
            &at_spin,
            config::value_bool(AUTOMATIC_SUSPEND, true),
        );
    };
    update_at_row();
    page.watch(AUTOMATIC_SUSPEND, update_at_row);
    let full_row = page.uniform_row(&battery_section);
    page.config_spin(
        &full_row,
        "charger",
        &tr("Full warning"),
        "/battery/full",
        101,
        (0, 101),
        5,
    );
    charge_limit(&page, &battery_section);
    if tools::has_system_service(POWER_PROFILES) {
        profile_choice(
            &page,
            &battery_section,
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
            &battery_section,
            "Power profile on the charger",
            PROFILE_ON_CHARGER,
            [
                ("Unchanged", ""),
                ("Balanced", "balanced"),
                ("Performance", "performance"),
            ],
        );
    }

    if is_hypridle_available {
        let sleep_section = page.section("bedtime", &tr("Automatic Suspend"));
        idle_timeout_row(
            &page,
            &sleep_section,
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
    let problem_label = note("colError");
    let page = Rc::downgrade(page);
    glib::spawn_future_local(async move {
        let buttons = powersettings::buttons().await;
        let Some(page) = page.upgrade() else {
            return;
        };
        let mut keys = vec![(powersettings::POWER_KEY, "Power button")];
        if buttons.has_lid {
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
                    **action != "hibernate" || buttons.can_hibernate || current == **action
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
            let selection_slot: Rc<RefCell<Option<Weak<Selection>>>> = Rc::default();
            let selection = Selection::new(&page.theme, choices, {
                let selection_slot = selection_slot.clone();
                let problem_label = problem_label.downgrade();
                move |value| {
                    let Some(selection) = selection_slot.borrow().as_ref().and_then(Weak::upgrade)
                    else {
                        return;
                    };
                    let action = value.as_str().unwrap_or("ignore").to_owned();
                    selection.set_current(&value);
                    let problem_label = problem_label.clone();
                    let selection = Rc::downgrade(&selection);
                    glib::spawn_future_local(async move {
                        let result = powersettings::set_button(key, &action).await;
                        let current_action = powersettings::buttons().await.action(key);
                        if let Some(selection) = selection.upgrade() {
                            selection.set_current(&Value::from(current_action));
                        }
                        show_result(&problem_label, result);
                    });
                }
            });
            selection.set_current(&Value::from(current));
            group.append(&selection.root);
            selection_slot.replace(Some(Rc::downgrade(&selection)));
            page.keep(selection);
        }
        section.append(&problem_label);
    });
}

fn charge_limit(page: &Rc<Page>, parent: &gtk4::Box) {
    let Some((battery, limit)) = powersettings::battery_with_limit() else {
        return;
    };
    let spin = SpinBox::new(
        &page.theme,
        powersettings::LIMIT_RANGE_PERCENT.0,
        powersettings::LIMIT_RANGE_PERCENT.1,
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
    let problem_label = note("colError");
    parent.append(&problem_label);
    let settle_timer: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    spin.connect_changed({
        let problem_label = problem_label.downgrade();
        move |percent| {
            if let Some(source) = settle_timer.take() {
                source.remove();
            }
            let (battery, problem_label, timer_slot) =
                (battery.clone(), problem_label.clone(), settle_timer.clone());
            settle_timer.set(Some(glib::timeout_add_local_once(
                LIMIT_SETTLE,
                move || {
                    timer_slot.set(None);
                    glib::spawn_future_local(async move {
                        let result = powersettings::set_charge_limit(&battery, percent).await;
                        show_result(&problem_label, result);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_timeout_under_half_a_minute_is_on_and_shown_as_one_minute() {
        assert_eq!(timeout_minutes(0), None);
        assert_eq!(timeout_minutes(20), Some(1));
        assert_eq!(timeout_minutes(90), Some(2));
        assert_eq!(timeout_minutes(900), Some(15));
    }
}
