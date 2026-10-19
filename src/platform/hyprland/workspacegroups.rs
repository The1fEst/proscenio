use crate::platform::hyprland::hyprconfig::{Area, lua_string, read_lua_string};

pub const AREA: Area = Area::Appearance;
pub const OBSOLETE_SIZE_FILE: &str = "workspaces.lua";
const RULE_PREFIX: &str = "hl.workspace_rule({ workspace = \"";
const MONITOR_KEY: &str = ", monitor = ";
const PERSISTENT_KEY: &str = ", persistent = true";
const DEFAULT_KEY: &str = ", default = true";
const RULE_END: &str = " })";

fn parse_rule(line: &str) -> Option<(i32, String)> {
    let (workspace, rest) = line.strip_prefix(RULE_PREFIX)?.split_once('"')?;
    let (output, rest) = read_lua_string(rest.strip_prefix(MONITOR_KEY)?)?;
    let rest = rest.strip_prefix(PERSISTENT_KEY)?;
    let is_generated = rest == RULE_END || rest.strip_prefix(DEFAULT_KEY) == Some(RULE_END);
    is_generated.then_some((workspace.parse().ok()?, output))
}

pub fn find_group_starts(text: &str) -> Vec<(String, i32)> {
    let mut rules: Vec<(i32, String)> = text.lines().filter_map(parse_rule).collect();
    rules.sort_by_key(|(workspace, _)| *workspace);
    let mut starts: Vec<(String, i32)> = Vec::new();
    for (workspace, output) in rules {
        if !starts.iter().any(|(known, _)| *known == output) {
            starts.push((output, workspace));
        }
    }
    starts
}

fn find_grouped_outputs(text: &str) -> Vec<String> {
    find_group_starts(text)
        .into_iter()
        .map(|(output, _)| output)
        .collect()
}

fn build_group_lines(size: i32, outputs: &[String]) -> Vec<String> {
    let mut lines = Vec::new();
    for (index, output) in (0..).zip(outputs) {
        let first = index * size + 1;
        for workspace in first..first + size {
            let default_key = if workspace == first { DEFAULT_KEY } else { "" };
            lines.push(format!(
                "{RULE_PREFIX}{workspace}\"{MONITOR_KEY}{}{PERSISTENT_KEY}{default_key}{RULE_END}",
                lua_string(output)
            ));
        }
    }
    lines
}

pub fn with_groups(text: &str, size: i32, outputs: &[String]) -> String {
    let mut group_outputs = find_grouped_outputs(text);
    for output in outputs {
        if !group_outputs.contains(output) {
            group_outputs.push(output.clone());
        }
    }
    let mut group_block = Some(build_group_lines(size, &group_outputs));
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        if parse_rule(line).is_none() {
            lines.push(line.to_owned());
        } else if let Some(block) = group_block.take() {
            lines.extend(block);
        }
    }
    lines.extend(group_block.into_iter().flatten());
    if lines.is_empty() {
        return String::new();
    }
    let mut new_text = lines.join("\n");
    new_text.push('\n');
    new_text
}

pub fn find_misplaced_outputs(
    text: &str,
    active_workspaces: &[(String, i64)],
) -> Vec<(String, i32)> {
    let rules: Vec<(i32, String)> = text.lines().filter_map(parse_rule).collect();
    active_workspaces
        .iter()
        .filter_map(|(output, active)| {
            let block = || {
                rules
                    .iter()
                    .filter(|(_, rule_output)| rule_output == output)
                    .map(|(workspace, _)| *workspace)
            };
            let (first, last) = (block().min()?, block().max()?);
            let is_inside = (i64::from(first)..=i64::from(last)).contains(active);
            (!is_inside).then(|| (output.clone(), first))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    const OTHER_RULES: &str = "hl.workspace_rule({ workspace = \"w[tv1]\", gaps_out = 0, gaps_in = 0 })\nhl.workspace_rule({ workspace = \"42\", monitor = \"DP-3\", persistent = true, gaps_out = 0 })";

    #[test]
    fn the_start_of_each_group_is_read_back_in_group_order() {
        let text = format!(
            "{OTHER_RULES}\n{}",
            with_groups("", 5, &to_names(&["DP-2", "odd \"name\"", "DP-1"]))
        );
        assert_eq!(
            find_group_starts(&text),
            vec![
                ("DP-2".to_owned(), 1),
                ("odd \"name\"".to_owned(), 6),
                ("DP-1".to_owned(), 11)
            ]
        );
        assert_eq!(find_group_starts(OTHER_RULES), Vec::new());
    }

    #[test]
    fn each_display_gets_a_group_of_persistent_workspaces_with_its_first_as_default() {
        let text = format!("hl.config({{ general = {{ border_size = 2 }} }})\n{OTHER_RULES}\n");
        assert_eq!(
            with_groups(&text, 2, &to_names(&["DP-1", "DP-2"])),
            format!(
                "hl.config({{ general = {{ border_size = 2 }} }})\n{OTHER_RULES}\nhl.workspace_rule({{ workspace = \"1\", monitor = \"DP-1\", persistent = true, default = true }})\nhl.workspace_rule({{ workspace = \"2\", monitor = \"DP-1\", persistent = true }})\nhl.workspace_rule({{ workspace = \"3\", monitor = \"DP-2\", persistent = true, default = true }})\nhl.workspace_rule({{ workspace = \"4\", monitor = \"DP-2\", persistent = true }})\n"
            )
        );
        assert_eq!(with_groups("", 3, &[]), "");
    }

    #[test]
    fn displays_keep_their_groups_and_a_new_display_gets_the_next_one() {
        let text = with_groups("", 5, &to_names(&["DP-2", "DP-1"]));
        let regrouped_text = with_groups(&text, 2, &to_names(&["HDMI-A-1", "DP-1", "DP-2"]));
        assert_eq!(
            regrouped_text,
            "hl.workspace_rule({ workspace = \"1\", monitor = \"DP-2\", persistent = true, default = true })\nhl.workspace_rule({ workspace = \"2\", monitor = \"DP-2\", persistent = true })\nhl.workspace_rule({ workspace = \"3\", monitor = \"DP-1\", persistent = true, default = true })\nhl.workspace_rule({ workspace = \"4\", monitor = \"DP-1\", persistent = true })\nhl.workspace_rule({ workspace = \"5\", monitor = \"HDMI-A-1\", persistent = true, default = true })\nhl.workspace_rule({ workspace = \"6\", monitor = \"HDMI-A-1\", persistent = true })\n"
        );
        assert_eq!(
            with_groups(&regrouped_text, 2, &to_names(&["DP-1"])),
            regrouped_text
        );
    }

    #[test]
    fn the_groups_are_rewritten_where_they_stand_and_a_second_write_changes_nothing() {
        let text = format!(
            "-- mine\n{}hl.config({{ decoration = {{ rounding = 12 }} }})\n",
            with_groups("", 1, &to_names(&["DP-1"]))
        );
        let grown_text = with_groups(&text, 2, &to_names(&["DP-1"]));
        assert_eq!(
            grown_text,
            "-- mine\nhl.workspace_rule({ workspace = \"1\", monitor = \"DP-1\", persistent = true, default = true })\nhl.workspace_rule({ workspace = \"2\", monitor = \"DP-1\", persistent = true })\nhl.config({ decoration = { rounding = 12 } })\n"
        );
        assert_eq!(
            with_groups(&grown_text, 2, &to_names(&["DP-1"])),
            grown_text
        );
    }

    #[test]
    fn only_displays_whose_active_workspace_left_their_block_are_sent_to_its_first_workspace() {
        let text = format!(
            "{OTHER_RULES}\n{}",
            with_groups("", 5, &to_names(&["DP-1", "DP-2", "HDMI-A-1"]))
        );
        let to_active_workspaces = |list: &[(&str, i64)]| -> Vec<(String, i64)> {
            list.iter()
                .map(|(output, workspace)| ((*output).to_owned(), *workspace))
                .collect()
        };
        assert_eq!(
            find_misplaced_outputs(
                &text,
                &to_active_workspaces(&[("DP-1", 3), ("DP-2", 11), ("HDMI-A-1", 11), ("DP-3", 42)])
            ),
            vec![("DP-2".to_owned(), 6)]
        );
        assert_eq!(
            find_misplaced_outputs(
                &text,
                &to_active_workspaces(&[("DP-1", 5), ("DP-2", 6), ("HDMI-A-1", 15)])
            ),
            Vec::<(String, i32)>::new()
        );
        assert_eq!(
            find_misplaced_outputs(&text, &to_active_workspaces(&[("DP-1", 0), ("DP-2", -1)])),
            vec![("DP-1".to_owned(), 1), ("DP-2".to_owned(), 6)]
        );
        assert_eq!(
            find_misplaced_outputs("", &to_active_workspaces(&[("DP-1", 11)])),
            Vec::new()
        );
    }

    #[test]
    fn a_display_name_is_written_as_a_lua_string() {
        let text = with_groups("", 1, &to_names(&["odd \"name\""]));
        assert!(text.contains("monitor = \"odd \\\"name\\\"\", persistent = true"));
        assert_eq!(with_groups(&text, 1, &to_names(&["odd \"name\""])), text);
    }
}
