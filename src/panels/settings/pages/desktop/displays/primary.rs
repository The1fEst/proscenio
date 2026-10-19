use crate::services::hardware::displays::Monitor;

pub(super) fn can_turn_off(monitor: &Monitor, others: &[Monitor]) -> bool {
    monitor.is_disabled || others.iter().any(|other| !other.is_disabled)
}

pub(super) fn find_primary_for_mirror<'a>(
    monitor: &Monitor,
    others: &'a [Monitor],
    target: &str,
) -> Option<&'a Monitor> {
    let source_id = monitor.id.to_string();
    let target_display = others
        .iter()
        .find(|other| other.name == target && !other.is_disabled);
    target_display
        .filter(|display| display.mirror_of == "none" || display.mirror_of == source_id)
        .or_else(|| pick_primary(others.iter().filter(|other| other.name != target)))
        .or(target_display)
}

pub(super) fn next_primary(others: &[Monitor]) -> Option<&Monitor> {
    pick_primary(others.iter())
}

fn pick_primary<'a>(candidates: impl Iterator<Item = &'a Monitor>) -> Option<&'a Monitor> {
    candidates
        .filter(|candidate| !candidate.is_disabled)
        .min_by_key(|candidate| candidate.mirror_of != "none")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(name: &str, is_disabled: bool) -> Monitor {
        Monitor {
            id: 0,
            name: name.to_owned(),
            model: String::new(),
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
            scale: 1.0,
            transform: 0,
            refresh_rate: 60.0,
            mirror_of: "none".to_owned(),
            current_format: String::new(),
            color_preset: String::new(),
            available_modes: Vec::new(),
            is_disabled,
            active_workspace_id: 0,
        }
    }

    #[test]
    fn the_main_display_moves_to_a_display_that_is_on() {
        let others = [monitor("DP-1", true), monitor("HDMI-A-1", false)];
        assert_eq!(
            next_primary(&others).map(|next| next.name.as_str()),
            Some("HDMI-A-1")
        );
        assert_eq!(next_primary(&others[..1]), None);
    }

    #[test]
    fn a_main_display_that_starts_mirroring_hands_the_main_role_to_its_target() {
        let main = Monitor {
            id: 7,
            ..monitor("eDP-1", false)
        };
        let make_mirror = |id: &str, name: &str| Monitor {
            mirror_of: id.to_owned(),
            ..monitor(name, false)
        };
        let others = [
            monitor("DP-1", true),
            monitor("HDMI-A-1", false),
            make_mirror("5", "DP-3"),
            make_mirror("7", "DP-4"),
            monitor("DP-2", false),
        ];
        let primary_name = |target: &str| {
            find_primary_for_mirror(&main, &others, target).map(|next| next.name.as_str())
        };
        assert_eq!(primary_name("DP-2"), Some("DP-2"));
        assert_eq!(primary_name("DP-1"), Some("HDMI-A-1"));
        assert_eq!(primary_name("DP-3"), Some("HDMI-A-1"));
        assert_eq!(primary_name("DP-4"), Some("DP-4"));

        let turned_off_source = Monitor {
            id: 5,
            ..monitor("DP-1", true)
        };
        let only_others = [turned_off_source, make_mirror("5", "DP-3")];
        assert_eq!(
            find_primary_for_mirror(&main, &only_others, "DP-3").map(|next| next.name.as_str()),
            Some("DP-3")
        );

        let turned_off_target = [monitor("DP-3", true)];
        assert_eq!(
            find_primary_for_mirror(&main, &turned_off_target, "DP-3"),
            None
        );
    }

    #[test]
    fn the_main_display_moves_to_a_display_that_is_not_mirroring_first() {
        let mirror = Monitor {
            mirror_of: "1".to_owned(),
            ..monitor("DP-2", false)
        };
        let others = [mirror, monitor("HDMI-A-1", false)];
        assert_eq!(
            next_primary(&others).map(|next| next.name.as_str()),
            Some("HDMI-A-1")
        );
        assert_eq!(
            next_primary(&others[..1]).map(|next| next.name.as_str()),
            Some("DP-2")
        );
    }
}
