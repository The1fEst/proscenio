use gtk4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::primary::{can_turn_off, find_primary_for_mirror, next_primary};
use super::rules::{keyed_combo, keyed_combo_in, option_spin};
use super::{
    AUTO_HDR, AUTO_HDR_MODES, GLOBAL_VRR, GLOBAL_VRR_MODES, MainWidgets, ROTATIONS, State, TEARING,
    VRR, Widgets, ZERO_SCALING, show_write_errors,
};
use crate::core::i18n::tr;
use crate::panels::settings::arrangement::Arrangement;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::hyprrows;
use crate::platform::hyprland::hypr;
use crate::services::hardware::displays::{format_number, rates_of, shown_modes_of};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let state = State::new(&page.theme, String::new());

    let arrangement_section = page.section("", "");
    let arrangement = Arrangement::new(&page.theme);
    arrangement_section.append(&arrangement.paint);
    arrangement.connect_picked({
        let state = Rc::downgrade(&state);
        move |name| {
            if let Some(state) = state.upgrade() {
                state.selected_name.replace(name.to_owned());
                state.refresh();
            }
        }
    });
    arrangement.connect_moved({
        let state = Rc::downgrade(&state);
        move |name, x, y| {
            if let Some(state) = state.upgrade() {
                state.displays.move_to(name, x, y);
            }
        }
    });

    page.keep(context.services.events.subscribe({
        let state = Rc::downgrade(&state);
        move |event, _| {
            if !matches!(
                event,
                "monitoradded" | "monitoraddedv2" | "monitorremoved" | "monitorremovedv2"
            ) {
                return;
            }
            if let Some(state) = state.upgrade() {
                state.displays.reload();
            }
        }
    }));

    let main_section = page.section("", "");
    let choices = gtk4::Box::new(gtk4::Orientation::Horizontal, 4);
    main_section.append(&choices);
    show_write_errors(&state.displays, &main_section);
    let (rescan_button, _) = page.icon_button("refresh", true, &tr("Rescan displays"), {
        let state = Rc::downgrade(&state);
        move || {
            hypr::request("dispatch hl.dsp.force_renderer_reload()");
            if let Some(state) = state.upgrade() {
                state.displays.reload();
            }
        }
    });
    rescan_button.set_valign(gtk4::Align::Center);
    choices.append(&rescan_button);
    page.tip(
        &rescan_button,
        &tr("Asks every monitor what it can do again. Modes a display only reports after it is fully awake show up after this."),
    );

    let use_as_section = page.subsection(&main_section, &tr("Use as"), "");
    let use_as_combo = page.combo(&use_as_section, "desktop_windows");
    use_as_combo.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            let others = state.others(&monitor);
            let value = match index {
                0 => "main".to_owned(),
                1 => "extend".to_owned(),
                item_index => match others.get(item_index - 2) {
                    Some(other) => other.name.clone(),
                    None if item_index == others.len() + 2 && can_turn_off(&monitor, &others) => {
                        "off".to_owned()
                    }
                    None => return,
                },
            };
            let is_main = state.displays.primary() == monitor.name;
            let next_primary_name = if is_main {
                next_primary(&others).map(|other| other.name.clone())
            } else {
                None
            };
            if value == "off" {
                state
                    .displays
                    .set_disabled(&monitor, true, next_primary_name.as_deref());
                return;
            }
            if monitor.is_disabled {
                if !state.displays.set_disabled(&monitor, false, None) {
                    return;
                }
                if value == "main" {
                    state.displays.set_primary(&monitor.name);
                } else if value != "extend" {
                    state.apply(&[("mirror", value)]);
                }
                return;
            }
            if value == "main" {
                state.displays.set_primary(&monitor.name);
                return;
            }
            if value != "extend" {
                let mirror_primary = is_main
                    .then(|| find_primary_for_mirror(&monitor, &others, &value))
                    .flatten()
                    .map(|other| other.name.clone());
                state.apply_with_primary(&[("mirror", value)], mirror_primary.as_deref());
                return;
            }
            if monitor.mirror_of == "none" {
                state
                    .apply_with_primary(&[("mirror", String::new())], next_primary_name.as_deref());
                return;
            }
            if let Some(next) = &next_primary_name
                && !state.displays.set_primary(next)
            {
                return;
            }
            let right_edge = state
                .others(&monitor)
                .iter()
                .filter(|other| !other.is_disabled && other.mirror_of == "none")
                .map(|other| {
                    let span = if other.transform % 2 == 1 {
                        other.height
                    } else {
                        other.width
                    };
                    other.x + (span as f64 / other.scale).round() as i64
                })
                .max()
                .unwrap_or(0);
            state.apply(&[
                ("mirror", String::new()),
                ("position", format!("{right_edge}x0")),
            ]);
        }
    });

    let resolution_section = page.subsection(&main_section, &tr("Resolution"), "");
    let resolution_combo = page.combo(&resolution_section, "aspect_ratio");
    resolution_combo.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            let Some(mode) = shown_modes_of(&monitor, state.shows_all_resolutions.get())
                .into_iter()
                .nth(index)
            else {
                return;
            };
            state.apply(&[
                ("size", format!("{}x{}", mode.mode.width, mode.mode.height)),
                ("scale", format_number(mode.scale)),
                (
                    "rate",
                    format_number(mode.rate.unwrap_or(monitor.refresh_rate)),
                ),
            ]);
        }
    });
    let all_resolutions_switch = page.switch(&main_section, "list", &tr("Show all resolutions"), {
        let state = Rc::downgrade(&state);
        move |is_on| {
            if let Some(state) = state.upgrade() {
                state.shows_all_resolutions.set(is_on);
                state.refresh();
            }
        }
    });
    all_resolutions_switch.bind({
        let state = Rc::downgrade(&state);
        move || {
            state
                .upgrade()
                .is_some_and(|state| state.shows_all_resolutions.get())
        }
    });

    let rate_section = page.subsection(&main_section, &tr("Refresh rate"), "");
    let rate_combo = page.combo(&rate_section, "refresh");
    rate_combo.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            if let Some(rate) = rates_of(&monitor).get(index) {
                state.apply(&[("rate", format_number(rate.round()))]);
            }
        }
    });

    let rotation_combo = keyed_combo(
        &page,
        &state,
        &main_section,
        &tr("Rotation"),
        "",
        "screen_rotation",
        move |index| ("transform", ROTATIONS[index].1.to_string()),
    );
    let vrr_group = page.subsection(&main_section, &tr("Variable refresh rate"), "");
    let vrr_combo = keyed_combo_in(&page, &state, &vrr_group, "sync", move |index| {
        ("vrr", VRR[index].1.to_string())
    });
    let vrr_notice = page.notice(
        &vrr_group,
        "info",
        &tr("This display doesn't support variable refresh rate."),
    );
    let open_color = context.subpage_opener_with("displaycolor");
    page.link_row(
        &main_section,
        "palette",
        &tr("Color"),
        &tr("Color profile, HDR and luminance of this display"),
        {
            let state = Rc::downgrade(&state);
            move || {
                if let Some(monitor) = state.upgrade().and_then(|state| state.monitor()) {
                    open_color(&monitor.name);
                }
            }
        },
    );

    let mut spins = Vec::new();
    let reserved_section = page.section("border_outer", &tr("Reserved area"));
    for (side, icon, label) in [
        ("top", "vertical_align_top", "Top"),
        ("right", "align_horizontal_right", "Right"),
        ("bottom", "vertical_align_bottom", "Bottom"),
        ("left", "align_horizontal_left", "Left"),
    ] {
        let (_, spin) = option_spin(
            &page,
            &state,
            &reserved_section,
            icon,
            &tr(label),
            (0, 2000),
            1,
            0,
            Box::new(move |displays, name| displays.reserved_side_of(name, side)),
            Box::new(move |displays, monitor, value| {
                displays.set_reserved_side(monitor, side, value)
            }),
        );
        spins.push(spin);
    }

    let all_displays_section = page.section("desktop_windows", &tr("All displays"));
    let auto_hdr_section = page.subsection(
        &all_displays_section,
        &tr("Auto HDR"),
        &tr("Switches to HDR while a fullscreen window has HDR content."),
    );
    let auto_hdr_combo = page.combo(&auto_hdr_section, "hdr_auto");
    auto_hdr_combo.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            if let Some(state) = state.upgrade() {
                state
                    .options
                    .set(AUTO_HDR, &AUTO_HDR_MODES[index].1.to_string());
            }
        }
    });
    let global_vrr_section = page.subsection(
        &all_displays_section,
        &tr("Variable refresh rate"),
        &tr("What a display set to follow the global setting does."),
    );
    let global_vrr_combo = hyprrows::combo(
        &page,
        &global_vrr_section,
        &state.options,
        "sync",
        (GLOBAL_VRR, "0"),
        &GLOBAL_VRR_MODES,
    );
    let global_vrr_notice = page.notice(
        &global_vrr_section,
        "info",
        &tr("No display that is on supports variable refresh rate."),
    );
    let tearing_switch = hyprrows::switch(
        &page,
        &all_displays_section,
        &state.options,
        "screenshot_monitor",
        &tr("Allow tearing"),
        TEARING,
    );
    page.tip(
        &tearing_switch.button,
        &tr("Lets a game draw a frame before the display is ready for it, trading a torn line for latency"),
    );
    let xwayland_section = page.subsection(&all_displays_section, &tr("X11 apps"), "");
    let zero_scaling_switch = hyprrows::switch(
        &page,
        &xwayland_section,
        &state.options,
        "high_density",
        &tr("Keep X11 apps sharp on scaled displays"),
        ZERO_SCALING,
    );
    page.tip(
        &zero_scaling_switch.button,
        &tr("X11 apps are drawn unscaled instead of stretched. They look sharp, and small unless they scale themselves (GDK_SCALE, QT_SCALE_FACTOR)."),
    );

    let night_section = page.section("", "");
    page.link_row(
        &night_section,
        "nightlight",
        &tr("Night light"),
        &tr("Schedule and color temperature"),
        context.subpage_opener("nightlight"),
    );

    state.widgets.replace(Some(Widgets {
        main: Some(MainWidgets {
            arrangement_root: arrangement_section.parent(),
            arrangement,
            choices,
            selection: RefCell::new(None),
            use_as_root: use_as_section.parent(),
            use_as_combo,
            resolution_combo,
            rate_combo,
            rotation_combo,
            vrr_combo,
            vrr_notice,
            auto_hdr_root: Page::subsection_root(&auto_hdr_section),
            auto_hdr_combo,
            global_vrr_combo,
            global_vrr_notice,
        }),
        color: None,
        spins,
    }));
    state.follow_changes();
    page.keep(state);
    page
}
