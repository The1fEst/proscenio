use crate::platform::hyprland::hyprconfig::{Area, lua_string, read_lua_string};

pub const AREA: Area = Area::Appearance;
pub const SIZE_FILE: &str = "workspaces.lua";
const RULE_PREFIX: &str = "hl.workspace_rule({ workspace = \"";
const MONITOR_KEY: &str = ", monitor = ";
const PERSISTENT_KEY: &str = ", persistent = true";
const DEFAULT_KEY: &str = ", default = true";
const RULE_END: &str = " })";

pub fn render_size_file(size: i32) -> String {
    format!("workspaceGroupSize = {size}\n")
}

fn parse_rule(line: &str) -> Option<(i32, String)> {
    let (workspace, rest) = line.strip_prefix(RULE_PREFIX)?.split_once('"')?;
    let (output, rest) = read_lua_string(rest.strip_prefix(MONITOR_KEY)?)?;
    let rest = rest.strip_prefix(PERSISTENT_KEY)?;
    let is_generated = rest == RULE_END || rest.strip_prefix(DEFAULT_KEY) == Some(RULE_END);
    is_generated.then_some((workspace.parse().ok()?, output))
}

fn find_grouped_outputs(text: &str) -> Vec<String> {
    let mut rules: Vec<(i32, String)> = text.lines().filter_map(parse_rule).collect();
    rules.sort_by_key(|(workspace, _)| *workspace);
    let mut outputs: Vec<String> = Vec::new();
    for (_, output) in rules {
        if !outputs.contains(&output) {
            outputs.push(output);
        }
    }
    outputs
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

#[cfg(test)]
mod tests {
    use super::*;

    fn to_names(list: &[&str]) -> Vec<String> {
        list.iter().map(|name| (*name).to_owned()).collect()
    }

    const OTHER_RULES: &str = "hl.workspace_rule({ workspace = \"w[tv1]\", gaps_out = 0, gaps_in = 0 })\nhl.workspace_rule({ workspace = \"42\", monitor = \"DP-3\", persistent = true, gaps_out = 0 })";

    #[test]
    fn the_group_size_file_sets_only_the_size() {
        assert_eq!(render_size_file(5), "workspaceGroupSize = 5\n");
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
    fn a_display_name_is_written_as_a_lua_string() {
        let text = with_groups("", 1, &to_names(&["odd \"name\""]));
        assert!(text.contains("monitor = \"odd \\\"name\\\"\", persistent = true"));
        assert_eq!(with_groups(&text, 1, &to_names(&["odd \"name\""])), text);
    }
}
