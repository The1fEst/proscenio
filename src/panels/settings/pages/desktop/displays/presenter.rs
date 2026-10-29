use gtk4::prelude::*;
use serde_json::Value;
use std::rc::Rc;

use super::primary::can_turn_off;
use super::rules::{DEPTHS, EOTFS, FORCE_MODES};
use super::{
    AUTO_HDR, AUTO_HDR_MODES, ColorWidgets, GLOBAL_VRR, MainWidgets, ROTATIONS, State, VRR,
    can_change_global_vrr, color_profiles,
};
use crate::core::i18n::{tr, trf};
use crate::panels::settings::content::Choice;
use crate::services::hardware::displays::{Monitor, rates_of, shown_modes_of};
use crate::ui::widgets::input::selection::Selection;

fn position<T: PartialEq>(values: &[T], current: &T) -> i32 {
    values
        .iter()
        .position(|value| value == current)
        .map_or(0, |index| index as i32)
}

fn labels<T>(list: &[(&str, T)]) -> Vec<String> {
    list.iter().map(|(label, _)| tr(label)).collect()
}

impl State {
    pub(super) fn refresh(self: &Rc<Self>) {
        let widgets = self.widgets.borrow();
        let Some(widgets) = widgets.as_ref() else {
            return;
        };
        let Some(monitor) = self.monitor() else {
            return;
        };
        if let Some(main) = &widgets.main {
            self.refresh_main(main, &monitor);
        }
        if let Some(color) = &widgets.color {
            self.refresh_color(color, &monitor);
        }
        for spin in &widgets.spins {
            spin.spin
                .set_value((spin.read_current)(&self.displays, &monitor.name));
        }
    }

    fn refresh_main(self: &Rc<Self>, widgets: &MainWidgets, monitor: &Monitor) {
        let monitors = self.monitors();
        let name = monitor.name.clone();

        if let Some(root) = &widgets.arrangement_root {
            root.set_visible(monitors.len() > 1);
        }
        widgets.arrangement.set(
            monitors
                .iter()
                .filter(|monitor| !monitor.is_disabled)
                .cloned()
                .collect(),
            &name,
        );

        let names: Vec<String> = monitors
            .iter()
            .map(|monitor| monitor.name.clone())
            .collect();
        let should_rebuild = widgets
            .selection
            .borrow()
            .as_ref()
            .is_none_or(|(last_names, _)| *last_names != names);
        if should_rebuild {
            if let Some((_, old_selection)) = widgets.selection.take() {
                widgets.choices.remove(&old_selection.root);
            }
            let choices = monitors
                .iter()
                .map(|monitor| Choice {
                    label: format!(
                        "{} ({})",
                        if monitor.model.is_empty() {
                            &monitor.name
                        } else {
                            &monitor.model
                        },
                        monitor.name
                    ),
                    icon: "",
                    value: Value::from(monitor.name.clone()),
                })
                .collect();
            let state = Rc::downgrade(self);
            let selection = Selection::new(&self.theme, choices, move |value| {
                if let (Some(state), Some(name)) = (state.upgrade(), value.as_str()) {
                    state.selected_name.replace(name.to_owned());
                    state.refresh();
                }
            });
            selection.root.set_hexpand(true);
            widgets.choices.prepend(&selection.root);
            widgets.selection.replace(Some((names, selection)));
        }
        if let Some((_, selection)) = widgets.selection.borrow().as_ref() {
            selection.set_current(&Value::from(name.clone()));
        }

        let others = self.others(&monitor);
        if let Some(root) = &widgets.use_as_root {
            root.set_visible(!others.is_empty());
        }
        let primary_name = self.displays.primary();
        let mirror_target_name = self.mirror_target_name(&monitor);
        let mut use_as_values = vec!["main".to_owned(), "extend".to_owned()];
        let mut use_as_labels = vec![tr("Main display"), tr("Extended display")];
        for other in &others {
            use_as_values.push(other.name.clone());
            use_as_labels.push(trf(
                "Mirror for %1",
                &[if other.model.is_empty() {
                    other.name.as_str()
                } else {
                    other.model.as_str()
                }],
            ));
        }
        if can_turn_off(&monitor, &others) {
            use_as_values.push("off".to_owned());
            use_as_labels.push(tr("Off"));
        }
        let current_use = if monitor.is_disabled {
            "off".to_owned()
        } else if primary_name == name {
            "main".to_owned()
        } else {
            others
                .iter()
                .find(|other| other.name == mirror_target_name)
                .map_or_else(|| "extend".to_owned(), |other| other.name.clone())
        };
        widgets
            .use_as_combo
            .set_items(&use_as_labels, position(&use_as_values, &current_use));

        let modes = shown_modes_of(&monitor, self.shows_all_resolutions.get());
        let scale = monitor.scale.max(0.01);
        let logical_size = format!(
            "{}x{}",
            (monitor.width as f64 / scale).round(),
            (monitor.height as f64 / scale).round()
        );
        let mode_values: Vec<String> = modes
            .iter()
            .map(|mode| format!("{}x{}", mode.width, mode.height))
            .collect();
        let mode_labels: Vec<String> = modes
            .iter()
            .map(|mode| {
                if mode.is_native {
                    trf(
                        "%1 × %2 (Default)",
                        &[&mode.width.to_string(), &mode.height.to_string()],
                    )
                } else {
                    format!("{} × {}", mode.width, mode.height)
                }
            })
            .collect();
        widgets
            .resolution_combo
            .set_items(&mode_labels, position(&mode_values, &logical_size));

        let rates: Vec<i64> = rates_of(&monitor)
            .iter()
            .map(|rate| rate.round() as i64)
            .collect();
        let rate_labels: Vec<String> = rates
            .iter()
            .map(|rate| trf("%1 Hz", &[&rate.to_string()]))
            .collect();
        widgets.rate_combo.set_items(
            &rate_labels,
            position(&rates, &(monitor.refresh_rate.round() as i64)),
        );
        widgets.rotation_combo.set_items(
            &labels(&ROTATIONS),
            position(&ROTATIONS.map(|(_, value)| value), &monitor.transform),
        );
        let rule_number = |key: &str| self.displays.number_of(&name, key) as i64;
        let enabled_monitors = || monitors.iter().filter(|other| !other.is_disabled);
        let is_vrr_enabled = self.allows_vrr(monitor);
        widgets.vrr_combo.set_enabled(is_vrr_enabled);
        widgets.vrr_notice.set_visible(!is_vrr_enabled);
        let can_change_displays: Vec<bool> = enabled_monitors()
            .map(|other| self.allows_vrr(other))
            .collect();
        let is_global_vrr_enabled =
            can_change_global_vrr(self.options.number(GLOBAL_VRR) as i64, &can_change_displays);
        widgets.global_vrr_combo.set_enabled(is_global_vrr_enabled);
        widgets
            .global_vrr_notice
            .set_visible(!is_global_vrr_enabled);
        let is_auto_hdr_changed = self.options.number_or(AUTO_HDR, 1.0) as i64 != 1;
        widgets.auto_hdr_root.set_visible(
            is_auto_hdr_changed || enabled_monitors().any(|other| self.offers_hdr(other)),
        );
        widgets.vrr_combo.set_items(
            &labels(&VRR),
            position(&VRR.map(|(_, value)| value), &rule_number("vrr")),
        );
        widgets.auto_hdr_combo.set_items(
            &labels(&AUTO_HDR_MODES),
            position(
                &AUTO_HDR_MODES.map(|(_, value)| value),
                &(self.options.number_or(AUTO_HDR, 1.0) as i64),
            ),
        );
    }

    fn refresh_color(&self, widgets: &ColorWidgets, monitor: &Monitor) {
        let name = monitor.name.clone();
        let rule_number = |key: &str| self.displays.number_of(&name, key) as i64;
        let offers_hdr = self.offers_hdr(monitor);
        if let Some(root) = &widgets.luminance_root {
            root.set_visible(offers_hdr);
        }
        let profiles = color_profiles(monitor, offers_hdr);
        let profile_labels: Vec<String> = profiles.iter().map(|(label, _)| label.clone()).collect();
        let profile_values: Vec<&str> = profiles.iter().map(|(_, value)| *value).collect();
        let current_profile = self.displays.color_profile_of(monitor);
        widgets.profile_combo.set_items(
            &profile_labels,
            position(&profile_values, &current_profile.as_str()),
        );
        widgets.depth_combo.set_items(
            &labels(&DEPTHS),
            position(&DEPTHS.map(|(_, value)| value), &rule_number("bitdepth")),
        );
        widgets.wide_color_combo.set_items(
            &labels(&FORCE_MODES),
            position(
                &FORCE_MODES.map(|(_, value)| value),
                &rule_number("supports_wide_color"),
            ),
        );
        widgets.hdr_combo.set_items(
            &labels(&FORCE_MODES),
            position(
                &FORCE_MODES.map(|(_, value)| value),
                &rule_number("supports_hdr"),
            ),
        );
        let current_eotf = self.displays.value_of(&name, "sdr_eotf");
        widgets.eotf_combo.set_items(
            &labels(&EOTFS),
            position(&EOTFS.map(|(_, value)| value), &current_eotf.as_str()),
        );
        let current_icc = self.displays.value_of(&name, "icc");
        let mut icc_values = vec![String::new()];
        let mut icc_labels = vec![tr("None")];
        for path in &self.displays.icc_profiles {
            icc_values.push(path.clone());
            icc_labels.push(path.rsplit('/').next().unwrap_or(path).to_owned());
        }
        widgets
            .icc_combo
            .set_items(&icc_labels, position(&icc_values, &current_icc));
    }
}
