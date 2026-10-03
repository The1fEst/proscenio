use std::rc::Rc;

use crate::core::i18n::tr;
use crate::panels::settings::content::{Context, Page};
use crate::panels::settings::pages::displays::{
    ColorWidgets, DEPTHS, DISPLAY_RULES, EOTFS, FORCED, SDR_RULES, State, Widgets, color_profiles,
    keyed_combo, rule_spin,
};

pub fn build(context: &Context) -> Rc<Page> {
    let page = Page::new(&context.theme, true);
    let selected = context.argument.borrow().clone().unwrap_or_default();
    let state = State::new(&page.theme, selected);
    if let (Some(heading), Some(monitor)) = (context.heading.upgrade(), state.monitor()) {
        let shown = if monitor.model.is_empty() {
            monitor.name.clone()
        } else {
            format!("{} ({})", monitor.model, monitor.name)
        };
        heading.set_text(&format!("{} · {shown}", tr("Color")));
    }

    let color = page.section("palette", &tr("Color"));
    let profile_group = page.subsection(&color, &tr("Color profile"), "");
    let profile = page.combo(&profile_group, "colors");
    profile.connect_activated({
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
    let depth = keyed_combo(&page, &state, &color, &tr("Bit depth"), "", "gradient", {
        move |index| ("bitdepth", DEPTHS[index].1.to_string())
    });
    let wide = keyed_combo(
        &page,
        &state,
        &color,
        &tr("Force wide color"),
        "",
        "invert_colors",
        move |index| ("supports_wide_color", FORCED[index].1.to_string()),
    );
    let hdr = keyed_combo(
        &page,
        &state,
        &color,
        &tr("Force HDR"),
        &tr("Forcing this on a display that does not report HDR can leave the screen black."),
        "hdr_on",
        move |index| ("supports_hdr", FORCED[index].1.to_string()),
    );
    let eotf = keyed_combo(
        &page,
        &state,
        &color,
        &tr("SDR transfer function"),
        "",
        "functions",
        move |index| ("sdr_eotf", EOTFS[index].1.to_owned()),
    );
    let icc = keyed_combo(
        &page,
        &state,
        &color,
        &tr("ICC profile"),
        "",
        "description",
        {
            let state = Rc::downgrade(&state);
            move |index| {
                let path = match index {
                    0 => String::new(),
                    other => state
                        .upgrade()
                        .and_then(|state| state.displays.icc_profiles.get(other - 1).cloned())
                        .unwrap_or_default(),
                };
                ("icc", path)
            }
        },
    );

    let mut spins = Vec::new();
    let luminance = page.section("brightness_6", &tr("Luminance"));
    for rule in &SDR_RULES {
        let (row, spin) = rule_spin(&page, &state, &luminance, rule);
        if rule.key == "sdrbrightness" {
            page.tip(
                &row,
                &tr("How bright content that is not HDR is drawn while the display is in HDR"),
            );
        }
        spins.push(spin);
    }
    let display_group = page.subsection(
        &luminance,
        &tr("Display"),
        &tr("A luminance of −1 leaves the figure to what the display reports."),
    );
    for rule in &DISPLAY_RULES {
        spins.push(rule_spin(&page, &state, &display_group, rule).1);
    }

    state.widgets.replace(Some(Widgets {
        main: None,
        color: Some(ColorWidgets {
            profile,
            depth,
            wide,
            hdr,
            eotf,
            icc,
        }),
        spins,
    }));
    state.follow_changes();
    page.keep(state);
    page
}
