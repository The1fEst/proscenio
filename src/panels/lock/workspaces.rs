use serde_json::Value;

const HIDDEN_WORKSPACE: i64 = 2_147_483_647;
const INVALID_WORKSPACE: i64 = -1;

fn resolve_original_workspace(active_workspace: i64) -> i64 {
    if active_workspace > HIDDEN_WORKSPACE / 2 {
        return HIDDEN_WORKSPACE - active_workspace;
    }
    active_workspace
}

pub(super) fn plan_hiding(
    workspaces_to_restore: &mut Vec<(String, i64)>,
    active_workspaces: &[(String, i64)],
) -> Vec<(String, i64)> {
    let mut hidden_workspaces = Vec::new();
    for (name, active_workspace) in active_workspaces {
        if *active_workspace == INVALID_WORKSPACE {
            continue;
        }
        let stored_workspace = workspaces_to_restore
            .iter()
            .find(|(stored_name, _)| stored_name == name)
            .map(|(_, workspace)| *workspace);
        let original_workspace = stored_workspace.unwrap_or_else(|| {
            let workspace = resolve_original_workspace(*active_workspace);
            workspaces_to_restore.push((name.clone(), workspace));
            workspace
        });
        let hidden_workspace = HIDDEN_WORKSPACE - original_workspace;
        if *active_workspace != hidden_workspace {
            hidden_workspaces.push((name.clone(), hidden_workspace));
        }
    }
    hidden_workspaces
}

pub(super) fn needs_hiding(
    active_workspaces: &[(String, i64)],
    previous_names: &[String],
    workspaces_to_restore: &[(String, i64)],
) -> bool {
    active_workspaces.iter().any(|(name, active_workspace)| {
        let is_stored = workspaces_to_restore
            .iter()
            .any(|(stored_name, _)| stored_name == name);
        !previous_names.contains(name) || (*active_workspace != INVALID_WORKSPACE && !is_stored)
    })
}

pub(super) fn parse_active_workspaces(monitors: &[Value]) -> Vec<(String, i64)> {
    monitors
        .iter()
        .filter_map(|monitor| {
            let name = monitor.get("name")?.as_str()?;
            let active_workspace = monitor.pointer("/activeWorkspace/id")?.as_i64()?;
            Some((name.to_owned(), active_workspace))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_pairs(entries: &[(&str, i64)]) -> Vec<(String, i64)> {
        entries
            .iter()
            .map(|(name, workspace)| (name.to_string(), *workspace))
            .collect()
    }

    #[test]
    fn a_workspace_hidden_by_an_earlier_lock_resolves_to_the_original_one() {
        let cases = [
            (1, 1),
            (-98, -98),
            (HIDDEN_WORKSPACE - 3, 3),
            (HIDDEN_WORKSPACE + 98, -98),
        ];
        for (active_workspace, original_workspace) in cases {
            assert_eq!(
                resolve_original_workspace(active_workspace),
                original_workspace,
                "{active_workspace}"
            );
        }
    }

    #[test]
    fn every_monitor_not_on_its_hidden_workspace_is_hidden_and_keeps_its_first_original() {
        let to_hidden = |workspace: i64| HIDDEN_WORKSPACE - workspace;
        let cases = [
            (
                to_pairs(&[]),
                to_pairs(&[("DP-1", 1), ("DP-2", 5)]),
                to_pairs(&[("DP-1", 1), ("DP-2", 5)]),
                to_pairs(&[("DP-1", to_hidden(1)), ("DP-2", to_hidden(5))]),
            ),
            (
                to_pairs(&[("DP-1", 1), ("DP-2", 5)]),
                to_pairs(&[("DP-1", to_hidden(1)), ("DP-2", to_hidden(5))]),
                to_pairs(&[("DP-1", 1), ("DP-2", 5)]),
                to_pairs(&[]),
            ),
            (
                to_pairs(&[("DP-1", 1), ("DP-2", 5)]),
                to_pairs(&[("DP-1", 3), ("DP-2", to_hidden(5))]),
                to_pairs(&[("DP-1", 1), ("DP-2", 5)]),
                to_pairs(&[("DP-1", to_hidden(1))]),
            ),
            (
                to_pairs(&[("DP-1", 1)]),
                to_pairs(&[("DP-1", to_hidden(7))]),
                to_pairs(&[("DP-1", 1)]),
                to_pairs(&[("DP-1", to_hidden(1))]),
            ),
            (
                to_pairs(&[("DP-2", 5)]),
                to_pairs(&[("DP-2", to_hidden(5)), ("HDMI-A-1", 2)]),
                to_pairs(&[("DP-2", 5), ("HDMI-A-1", 2)]),
                to_pairs(&[("HDMI-A-1", to_hidden(2))]),
            ),
            (
                to_pairs(&[("DP-2", 5)]),
                to_pairs(&[("DP-2", to_hidden(5)), ("HDMI-A-1", INVALID_WORKSPACE)]),
                to_pairs(&[("DP-2", 5)]),
                to_pairs(&[]),
            ),
        ];
        for (stored, active, restored, hidden_workspaces) in cases {
            let mut workspaces_to_restore = stored.clone();
            assert_eq!(
                plan_hiding(&mut workspaces_to_restore, &active),
                hidden_workspaces,
                "{stored:?} {active:?}"
            );
            assert_eq!(workspaces_to_restore, restored, "{stored:?} {active:?}");
        }
    }

    #[test]
    fn a_monitor_needs_hiding_when_it_appears_or_has_a_workspace_but_none_stored() {
        let to_names = |entries: &[&str]| -> Vec<String> {
            entries.iter().map(|name| name.to_string()).collect()
        };
        let to_hidden = |workspace: i64| HIDDEN_WORKSPACE - workspace;
        let stored = to_pairs(&[("DP-1", 1), ("DP-2", 5)]);
        let both_names = to_names(&["DP-1", "DP-2"]);
        let names_with_hdmi = to_names(&["DP-1", "HDMI-A-1"]);
        let cases = [
            (
                to_pairs(&[("DP-1", to_hidden(1)), ("DP-2", to_hidden(5))]),
                both_names.clone(),
                false,
            ),
            (
                to_pairs(&[("DP-1", to_hidden(1))]),
                both_names.clone(),
                false,
            ),
            (
                to_pairs(&[("DP-1", 1), ("DP-2", 3)]),
                to_names(&["DP-2"]),
                true,
            ),
            (
                to_pairs(&[("DP-1", to_hidden(1)), ("HDMI-A-1", 2)]),
                names_with_hdmi.clone(),
                true,
            ),
            (
                to_pairs(&[("DP-1", to_hidden(1)), ("HDMI-A-1", INVALID_WORKSPACE)]),
                names_with_hdmi,
                false,
            ),
        ];
        for (active_workspaces, previous_names, should_hide) in cases {
            assert_eq!(
                needs_hiding(&active_workspaces, &previous_names, &stored),
                should_hide,
                "{active_workspaces:?} {previous_names:?}"
            );
        }
    }

    #[test]
    fn monitors_without_a_name_or_a_numeric_workspace_are_left_out() {
        let monitors = [
            serde_json::json!({"name": "DP-1", "activeWorkspace": {"id": 3}}),
            serde_json::json!({"name": "DP-2", "activeWorkspace": {"id": "3"}}),
            serde_json::json!({"activeWorkspace": {"id": 4}}),
            serde_json::json!({"name": "HDMI-A-1"}),
        ];
        assert_eq!(parse_active_workspaces(&monitors), to_pairs(&[("DP-1", 3)]));
    }
}
