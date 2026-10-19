use crate::platform::hyprland::hyprconfig::lua_string;

pub const BEGIN_LINE: &str = "-- proscenio workspace keys";
pub const END_LINE: &str = "-- end proscenio workspace keys";
const MAX_KEYS: i32 = 10;
const TEMPLATE: &str = r#"-- proscenio workspace keys
do
    local size = $SIZE
    local starts = $STARTS
    local numpad = { 87, 88, 89, 83, 84, 85, 79, 80, 81, 90 }
    local function find_group_start()
        local monitor = hl.get_active_monitor()
        return monitor and starts[monitor.name] or math.floor((hl.get_active_workspace().id - 1) / size) * size + 1
    end
    local function find_workspace_in_group(i)
        return find_group_start() + i - 1
    end
    local function find_adjacent_workspace(delta)
        local first = find_group_start()
        local current = hl.get_active_workspace().id
        if current < first or current >= first + size then
            return delta > 0 and first or first + size - 1
        end
        return first + (current - first + delta) % size
    end
    local function send(i)
        hl.dispatch(hl.dsp.window.move({ workspace = find_workspace_in_group(i), follow = false }))
    end
    local function focus(i)
        hl.dispatch(hl.dsp.focus({ workspace = find_workspace_in_group(i) }))
    end
    for i = 1, $COUNT do
        hl.bind("SUPER + ALT + " .. (i % 10), function() send(i) end, { description = "Window: Send to workspace " .. i })
    end
    for i = 1, $COUNT do
        hl.bind("SUPER + ALT + code:" .. numpad[i], function() send(i) end, { description = "Window: Send to workspace " .. i })
    end
    for i = 1, $COUNT do
        hl.bind("SUPER + " .. (i % 10), function() focus(i) end, { description = "Workspace: Focus " .. i })
    end
    for i = 1, $COUNT do
        hl.bind("SUPER + code:" .. numpad[i], function() focus(i) end, { description = "Workspace: Focus " .. i })
    end
    for _, side in ipairs({ { "Left", -1, "left" }, { "H", -1, "left" }, { "Right", 1, "right" }, { "L", 1, "right" } }) do
        hl.bind("CTRL + SUPER + " .. side[1], function()
            hl.dispatch(hl.dsp.focus({ workspace = find_adjacent_workspace(side[2]) }))
        end, { description = "Workspace: Focus " .. side[3] .. " adjacent" })
        hl.bind("CTRL + SUPER + ALT + " .. side[1], function()
            hl.dispatch(hl.dsp.window.move({ workspace = find_adjacent_workspace(side[2]), follow = true }))
        end, { description = "Window: Send to " .. side[3] .. " adjacent workspace and follow" })
    end
    hl.bind("CTRL + SUPER + Up", function() focus(1) end, { description = "Workspace: Focus first in group" })
    hl.bind("CTRL + SUPER + Down", function() focus(size) end, { description = "Workspace: Focus last in group" })
end
-- end proscenio workspace keys
"#;

fn render_keys(size: i32, starts: &[(String, i32)]) -> String {
    let entries: Vec<String> = starts
        .iter()
        .map(|(output, first)| format!("[{}] = {first}", lua_string(output)))
        .collect();
    let table = if entries.is_empty() {
        "{}".to_owned()
    } else {
        format!("{{ {} }}", entries.join(", "))
    };
    TEMPLATE
        .replace("$SIZE", &size.to_string())
        .replace("$STARTS", &table)
        .replace("$COUNT", &size.clamp(1, MAX_KEYS).to_string())
}

pub fn with_keys(text: &str, size: i32, starts: &[(String, i32)]) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let block_range = lines
        .iter()
        .position(|line| line.trim_end() == END_LINE)
        .and_then(|end| {
            let begin = lines[..end]
                .iter()
                .rposition(|line| line.trim_end() == BEGIN_LINE)?;
            Some(begin..end + 1)
        });
    let mut remaining_lines = lines;
    if let Some(range) = block_range {
        remaining_lines.drain(range);
    }
    let mut new_text = render_keys(size, starts);
    for line in remaining_lines
        .into_iter()
        .skip_while(|line| line.trim().is_empty())
    {
        new_text.push_str(line);
        new_text.push('\n');
    }
    new_text
}

#[cfg(test)]
mod tests {
    use super::*;

    const TWO_DISPLAYS: &str = r#"-- proscenio workspace keys
do
    local size = 5
    local starts = { ["DP-1"] = 1, ["DP-2"] = 6 }
    local numpad = { 87, 88, 89, 83, 84, 85, 79, 80, 81, 90 }
    local function find_group_start()
        local monitor = hl.get_active_monitor()
        return monitor and starts[monitor.name] or math.floor((hl.get_active_workspace().id - 1) / size) * size + 1
    end
    local function find_workspace_in_group(i)
        return find_group_start() + i - 1
    end
    local function find_adjacent_workspace(delta)
        local first = find_group_start()
        local current = hl.get_active_workspace().id
        if current < first or current >= first + size then
            return delta > 0 and first or first + size - 1
        end
        return first + (current - first + delta) % size
    end
    local function send(i)
        hl.dispatch(hl.dsp.window.move({ workspace = find_workspace_in_group(i), follow = false }))
    end
    local function focus(i)
        hl.dispatch(hl.dsp.focus({ workspace = find_workspace_in_group(i) }))
    end
    for i = 1, 5 do
        hl.bind("SUPER + ALT + " .. (i % 10), function() send(i) end, { description = "Window: Send to workspace " .. i })
    end
    for i = 1, 5 do
        hl.bind("SUPER + ALT + code:" .. numpad[i], function() send(i) end, { description = "Window: Send to workspace " .. i })
    end
    for i = 1, 5 do
        hl.bind("SUPER + " .. (i % 10), function() focus(i) end, { description = "Workspace: Focus " .. i })
    end
    for i = 1, 5 do
        hl.bind("SUPER + code:" .. numpad[i], function() focus(i) end, { description = "Workspace: Focus " .. i })
    end
    for _, side in ipairs({ { "Left", -1, "left" }, { "H", -1, "left" }, { "Right", 1, "right" }, { "L", 1, "right" } }) do
        hl.bind("CTRL + SUPER + " .. side[1], function()
            hl.dispatch(hl.dsp.focus({ workspace = find_adjacent_workspace(side[2]) }))
        end, { description = "Workspace: Focus " .. side[3] .. " adjacent" })
        hl.bind("CTRL + SUPER + ALT + " .. side[1], function()
            hl.dispatch(hl.dsp.window.move({ workspace = find_adjacent_workspace(side[2]), follow = true }))
        end, { description = "Window: Send to " .. side[3] .. " adjacent workspace and follow" })
    end
    hl.bind("CTRL + SUPER + Up", function() focus(1) end, { description = "Workspace: Focus first in group" })
    hl.bind("CTRL + SUPER + Down", function() focus(size) end, { description = "Workspace: Focus last in group" })
end
-- end proscenio workspace keys
"#;

    fn to_starts(list: &[(&str, i32)]) -> Vec<(String, i32)> {
        list.iter()
            .map(|(output, first)| ((*output).to_owned(), *first))
            .collect()
    }

    #[test]
    fn the_keys_follow_the_start_of_the_focused_displays_group() {
        assert_eq!(
            with_keys("", 5, &to_starts(&[("DP-1", 1), ("DP-2", 6)])),
            TWO_DISPLAYS
        );
    }

    #[test]
    fn a_group_larger_than_ten_still_gets_ten_keys_and_a_display_name_is_a_lua_string() {
        let text = with_keys("", 20, &to_starts(&[("odd \"name\"", 1)]));
        assert!(text.contains("local size = 20\n"));
        assert!(text.contains("local starts = { [\"odd \\\"name\\\"\"] = 1 }\n"));
        assert_eq!(text.matches("for i = 1, 10 do").count(), 4);
        assert!(with_keys("", 0, &[]).contains("local starts = {}\n"));
        assert_eq!(with_keys("", 0, &[]).matches("for i = 1, 1 do").count(), 4);
    }

    #[test]
    fn the_block_is_rewritten_on_top_and_a_second_write_changes_nothing() {
        let shortcuts = "shortcut(\"Window: Close\", \"SUPER + Q\", false)\nshortcut(\"Workspace: Focus 1\", \"SUPER + Z\", false)\n";
        let text = format!("{shortcuts}{}", TWO_DISPLAYS.replace("= 6 }", "= 9 }"));
        let rewritten_text = with_keys(&text, 5, &to_starts(&[("DP-1", 1), ("DP-2", 6)]));
        assert_eq!(rewritten_text, format!("{TWO_DISPLAYS}{shortcuts}"));
        assert_eq!(
            with_keys(&rewritten_text, 5, &to_starts(&[("DP-1", 1), ("DP-2", 6)])),
            rewritten_text
        );
    }

    #[test]
    fn a_marker_without_its_end_is_not_taken_for_a_block() {
        let text = format!("{BEGIN_LINE}\nshortcut(\"Window: Close\", \"SUPER + Q\", false)\n");
        let new_text = with_keys(&text, 5, &[]);
        assert!(new_text.ends_with(&text));
        assert_eq!(new_text.matches(BEGIN_LINE).count(), 2);
    }

    #[test]
    fn a_stray_marker_before_the_block_or_trailing_spaces_on_a_marker_do_not_cost_other_lines() {
        let shortcut_line = "shortcut(\"Window: Close\", \"SUPER + Q\", false)\n";
        let starts = to_starts(&[("DP-1", 1), ("DP-2", 6)]);
        let text = format!("{BEGIN_LINE}\n{shortcut_line}{TWO_DISPLAYS}");
        assert_eq!(
            with_keys(&text, 5, &starts),
            format!("{TWO_DISPLAYS}{BEGIN_LINE}\n{shortcut_line}")
        );
        let padded_text = TWO_DISPLAYS
            .replace(BEGIN_LINE, &format!("{BEGIN_LINE} "))
            .replace(END_LINE, &format!("{END_LINE}  "));
        assert_eq!(with_keys(&padded_text, 5, &starts), TWO_DISPLAYS);
    }
}
