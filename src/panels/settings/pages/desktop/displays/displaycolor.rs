use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::desktop::displays::rules::{
    DEPTHS, DISPLAY_RULES, EOTFS, FORCE_MODES, SDR_RULES, keyed_combo, rule_spin,
};
use crate::panels::settings::pages::desktop::displays::{
    ColorWidgets, State, Widgets, color_profiles, show_write_errors,
};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let selected_name = context.argument.borrow().clone().unwrap_or_default();
    let state = State::new(&page.theme, selected_name);
    if let (Some(heading), Some(monitor)) = (context.heading.upgrade(), state.monitor()) {
        let title = if monitor.model.is_empty() {
            monitor.name.clone()
        } else {
            format!("{} ({})", monitor.model, monitor.name)
        };
        heading.set_text(&format!("{} · {title}", tr("Color")));
    }

    let color_section = page.section("palette", &tr("Color"));
    show_write_errors(&state.displays, &color_section);
    let profile_section = page.subsection(&color_section, &tr("Color profile"), "");
    let profile_combo = page.combo(&profile_section, "colors");
    profile_combo.connect_activated({
        let state = Rc::downgrade(&state);
        move |index| {
            let Some(state) = state.upgrade() else {
                return;
            };
            let Some(monitor) = state.monitor() else {
                return;
            };
            if let Some((_, value)) = color_profiles(&monitor).get(index) {
                state.displays.set_color_profile(&monitor, value);
            }
        }
    });
    let depth_combo = keyed_combo(
        &page,
        &state,
        &color_section,
        &tr("Bit depth"),
        "",
        "gradient",
        move |index| ("bitdepth", DEPTHS[index].1.to_string()),
    );
    let wide_color_combo = keyed_combo(
        &page,
        &state,
        &color_section,
        &tr("Force wide color"),
        "",
        "invert_colors",
        move |index| ("supports_wide_color", FORCE_MODES[index].1.to_string()),
    );
    let hdr_combo = keyed_combo(
        &page,
        &state,
        &color_section,
        &tr("Force HDR"),
        &tr("Forcing this on a display that does not report HDR can leave the screen black."),
        "hdr_on",
        move |index| ("supports_hdr", FORCE_MODES[index].1.to_string()),
    );
    let eotf_combo = keyed_combo(
        &page,
        &state,
        &color_section,
        &tr("SDR transfer function"),
        "",
        "functions",
        move |index| ("sdr_eotf", EOTFS[index].1.to_owned()),
    );
    let icc_combo = keyed_combo(
        &page,
        &state,
        &color_section,
        &tr("ICC profile"),
        "",
        "description",
        {
            let state = Rc::downgrade(&state);
            move |index| {
                let path = match index {
                    0 => String::new(),
                    item_index => state
                        .upgrade()
                        .and_then(|state| state.displays.icc_profiles.get(item_index - 1).cloned())
                        .unwrap_or_default(),
                };
                ("icc", path)
            }
        },
    );

    let mut spins = Vec::new();
    let luminance_section = page.section("brightness_6", &tr("Luminance"));
    for rule in &SDR_RULES {
        let (row, spin) = rule_spin(&page, &state, &luminance_section, rule);
        if rule.key == "sdrbrightness" {
            page.tip(
                &row,
                &tr("How bright content that is not HDR is drawn while the display is in HDR"),
            );
        }
        spins.push(spin);
    }
    let display_section = page.subsection(
        &luminance_section,
        &tr("Display"),
        &tr("A luminance of −1 leaves the figure to what the display reports."),
    );
    for rule in &DISPLAY_RULES {
        spins.push(rule_spin(&page, &state, &display_section, rule).1);
    }

    state.widgets.replace(Some(Widgets {
        main: None,
        color: Some(ColorWidgets {
            profile_combo,
            depth_combo,
            wide_color_combo,
            hdr_combo,
            eotf_combo,
            icc_combo,
        }),
        spins,
    }));
    state.follow_changes();
    page.keep(state);
    page
}
