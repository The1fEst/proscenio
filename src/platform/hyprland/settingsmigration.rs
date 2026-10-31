use crate::platform::hyprland::hyprconfig::{
    Area, Lines, NO_DIM_FULLSCREEN, OPAQUE_FULLSCREEN, SMART_GAPS, config_dir, prefix_for,
    read_for_edit, write,
};

const LEGACY_HEADER: [&str; 2] = [
    "-- Written by the settings app.",
    "-- before the files in custom/,",
];

const AREAS: [Area; 10] = [
    Area::Appearance,
    Area::Displays,
    Area::Multitasking,
    Area::Keyboard,
    Area::Accessibility,
    Area::Mouse,
    Area::Devices,
    Area::Apps,
    Area::Binds,
    Area::Other,
];

fn statements(text: &str) -> Vec<String> {
    let mut statements: Vec<String> = Vec::new();
    for line in text.lines() {
        let is_continuation = line.starts_with(char::is_whitespace)
            || line.starts_with('}')
            || line.starts_with(')')
            || line.starts_with("end");
        match statements.last_mut() {
            Some(last) if is_continuation => {
                last.push('\n');
                last.push_str(line);
            }
            _ if line.trim().is_empty() => {}
            _ => statements.push(line.to_owned()),
        }
    }
    statements
}

fn option_of(statement: &str) -> Option<String> {
    let mut rest = statement.strip_prefix("hl.config({")?;
    let mut keys = Vec::new();
    loop {
        rest = rest.trim_start();
        let end =
            rest.find(|character: char| !(character.is_alphanumeric() || character == '_'))?;
        keys.push(&rest[..end]);
        rest = rest[end..].trim_start().strip_prefix('=')?.trim_start();
        match rest.strip_prefix('{') {
            Some(inner) => rest = inner,
            None => return Some(keys.join(":")),
        }
    }
}

fn area_of(statement: &str, option_area: &impl Fn(&str) -> Option<Area>) -> Option<Area> {
    let first = statement.lines().next().unwrap_or("").trim();
    if LEGACY_HEADER.iter().any(|header| first.starts_with(header)) {
        return None;
    }
    let is_in = |line_set: &Lines| line_set.lines.contains(&first);
    Some(if let Some(option) = option_of(first) {
        option_area(&option).unwrap_or(Area::Other)
    } else if first.starts_with("hl.monitor(")
        || first.starts_with("hl.env(\"WAYLANDDRV_PRIMARY_MONITOR\"")
    {
        Area::Displays
    } else if first.starts_with("hl.env(") || first.starts_with("hl.on(\"hyprland.start\"") {
        Area::Appearance
    } else if is_in(&SMART_GAPS) {
        Area::Multitasking
    } else if is_in(&NO_DIM_FULLSCREEN) || is_in(&OPAQUE_FULLSCREEN) {
        Area::Appearance
    } else if first.starts_with("hl.window_rule(") {
        Area::Apps
    } else if first.starts_with("hl.device(") {
        Area::Devices
    } else if first.starts_with("hl.gesture(") {
        Area::Mouse
    } else if first.starts_with("shortcut(") {
        Area::Binds
    } else {
        Area::Other
    })
}

fn split(text: &str, option_area: &impl Fn(&str) -> Option<Area>) -> Vec<(Area, String)> {
    let mut parts: Vec<(Area, String)> = Vec::new();
    for statement in statements(text) {
        let Some(area) = area_of(&statement, option_area) else {
            continue;
        };
        match parts.iter_mut().find(|(known, _)| *known == area) {
            Some((_, part)) => {
                part.push_str(&statement);
                part.push('\n');
            }
            None => parts.push((area, format!("{statement}\n"))),
        }
    }
    parts
}

pub fn split_legacy(option_area: impl Fn(&str) -> Option<Area>) -> std::io::Result<()> {
    let legacy_path = config_dir().join("settings.lua");
    let Ok(text) = std::fs::read_to_string(&legacy_path) else {
        return Ok(());
    };
    let mut area_texts = Vec::new();
    for (area, part) in split(&text, &option_area) {
        let mut area_text = read_for_edit(area)?;
        if !area_text.is_empty() && !area_text.ends_with('\n') {
            area_text.push('\n');
        }
        area_text.push_str(&part);
        area_texts.push((area, area_text));
    }
    for (area, area_text) in area_texts {
        write(area, &area_text)?;
    }
    std::fs::rename(&legacy_path, config_dir().join("settings.lua.bak"))
}

fn move_lines(
    mut files: Vec<(Area, String)>,
    option_area: &impl Fn(&str) -> Option<Area>,
) -> Vec<(Area, String)> {
    let mut misplaced_lines: Vec<(Area, String, String)> = Vec::new();
    for (area, text) in files.iter_mut() {
        let mut remainder = String::new();
        for line in text.split_inclusive('\n') {
            let statement = line.trim_end_matches('\n');
            match option_of(statement).and_then(|option| Some((option_area(&option)?, option))) {
                Some((owner, option)) if owner != *area => {
                    misplaced_lines.push((owner, option, statement.to_owned()));
                }
                _ => remainder.push_str(line),
            }
        }
        *text = remainder;
    }
    for (owner, option, statement) in misplaced_lines {
        let index = match files.iter().position(|(area, _)| *area == owner) {
            Some(index) => index,
            None => {
                files.push((owner, String::new()));
                files.len() - 1
            }
        };
        let text = &mut files[index].1;
        let prefix = prefix_for(&option);
        if text.lines().any(|line| line.starts_with(&prefix)) {
            continue;
        }
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&statement);
        text.push('\n');
    }
    files
}

pub fn move_misplaced(option_area: impl Fn(&str) -> Option<Area>) -> std::io::Result<()> {
    let files: Vec<(Area, String)> = AREAS
        .iter()
        .map(|area| Ok((*area, read_for_edit(*area)?)))
        .collect::<std::io::Result<_>>()?;
    for (area, text) in move_lines(files.clone(), &option_area) {
        if !files.contains(&(area, text.clone())) {
            write(area, &text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_single_settings_file_splits_into_its_areas() {
        let legacy_text = concat!(
            "-- Written by the settings app. Hyprland sources it after its own configuration and\n",
            "-- before the files in custom/, so anything set here can still be overridden there.\n",
            "hl.env(\"XCURSOR_SIZE\", \"36\")\n",
            "\n",
            "hl.on(\"hyprland.start\", function()\n",
            "\thl.exec_cmd(\"hyprctl setcursor Bibata 36\")\n",
            "end)\n",
            "\n",
            "hl.monitor({\n",
            "\toutput = \"DP-1\",\n",
            "\treserved_area = { top = 0, right = 0 },\n",
            "})\n",
            "hl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"DP-1\")\n",
            "hl.config({ general = { gaps_in = 2 } })\n",
            "hl.config({ decoration = { shadow = { range = 10 } } })\n",
            "hl.config({ misc = { mystery = 1 } })\n",
            "hl.window_rule({ name = \"no-dim-fullscreen\", match = { fullscreen = true }, no_dim = true })\n",
            "hl.window_rule({ match = { class = \"^(code)$\" }, workspace = \"special:magic\" })\n",
            "hl.device({ name = \"mouse\", sensitivity = 0.5 })\n",
            "hl.gesture({\n",
            "\tfingers = 3,\n",
            "\tdirection = \"swipe\",\n",
            "})\n",
            "shortcut(\"Window: Close\", \"SUPER + X\", false)\n",
        );
        let option_area = |option: &str| match option {
            "general:gaps_in" => Some(Area::Multitasking),
            "decoration:shadow:range" => Some(Area::Appearance),
            _ => None,
        };
        let parts = split(legacy_text, &option_area);
        let part = |area| {
            parts
                .iter()
                .find(|(known, _)| *known == area)
                .map(|(_, text)| text.as_str())
                .unwrap_or("")
        };
        assert_eq!(
            part(Area::Appearance),
            "hl.env(\"XCURSOR_SIZE\", \"36\")\nhl.on(\"hyprland.start\", function()\n\thl.exec_cmd(\"hyprctl setcursor Bibata 36\")\nend)\nhl.config({ decoration = { shadow = { range = 10 } } })\nhl.window_rule({ name = \"no-dim-fullscreen\", match = { fullscreen = true }, no_dim = true })\n"
        );
        assert_eq!(
            part(Area::Displays),
            "hl.monitor({\n\toutput = \"DP-1\",\n\treserved_area = { top = 0, right = 0 },\n})\nhl.env(\"WAYLANDDRV_PRIMARY_MONITOR\", \"DP-1\")\n"
        );
        assert_eq!(
            part(Area::Multitasking),
            "hl.config({ general = { gaps_in = 2 } })\n"
        );
        assert_eq!(part(Area::Other), "hl.config({ misc = { mystery = 1 } })\n");
        assert_eq!(
            part(Area::Apps),
            "hl.window_rule({ match = { class = \"^(code)$\" }, workspace = \"special:magic\" })\n"
        );
        assert_eq!(
            part(Area::Devices),
            "hl.device({ name = \"mouse\", sensitivity = 0.5 })\n"
        );
        assert_eq!(
            part(Area::Mouse),
            "hl.gesture({\n\tfingers = 3,\n\tdirection = \"swipe\",\n})\n"
        );
        assert_eq!(
            part(Area::Binds),
            "shortcut(\"Window: Close\", \"SUPER + X\", false)\n"
        );
        assert_eq!(parts.len(), 8);
    }

    #[test]
    fn an_option_moved_to_another_page_moves_to_its_file() {
        let files = vec![
            (
                Area::Multitasking,
                "hl.config({ general = { gaps_in = 2 } })\nhl.config({ general = { border_size = 3 } })\nhl.config({ misc = { mystery = 1 } })\n".to_owned(),
            ),
            (
                Area::Appearance,
                "hl.config({ decoration = { rounding = 10 } })".to_owned(),
            ),
            (
                Area::Accessibility,
                "hl.config({ misc = { animate_manual_resizes = true } })\n".to_owned(),
            ),
            (
                Area::Displays,
                "hl.config({ misc = { animate_manual_resizes = false } })\n".to_owned(),
            ),
        ];
        let option_area = |option: &str| match option {
            "general:gaps_in" => Some(Area::Multitasking),
            "general:border_size" | "decoration:rounding" => Some(Area::Appearance),
            "misc:animate_manual_resizes" => Some(Area::Displays),
            "decoration:shadow:range" => Some(Area::Mouse),
            _ => None,
        };
        assert_eq!(
            move_lines(files, &option_area),
            vec![
                (
                    Area::Multitasking,
                    "hl.config({ general = { gaps_in = 2 } })\nhl.config({ misc = { mystery = 1 } })\n".to_owned(),
                ),
                (
                    Area::Appearance,
                    "hl.config({ decoration = { rounding = 10 } })\nhl.config({ general = { border_size = 3 } })\n".to_owned(),
                ),
                (Area::Accessibility, String::new()),
                (
                    Area::Displays,
                    "hl.config({ misc = { animate_manual_resizes = false } })\n".to_owned(),
                ),
            ]
        );
        let unchanged_files = vec![(
            Area::Mouse,
            "hl.config({ decoration = { shadow = { range = 10 } } })\n".to_owned(),
        )];
        assert_eq!(
            move_lines(unchanged_files.clone(), &option_area),
            unchanged_files
        );
    }

    #[test]
    fn the_workspace_keys_block_stays_where_it_is_when_misplaced_options_move() {
        let keys_text =
            crate::platform::hyprland::workspacekeys::with_keys("", 5, &[("DP-1".to_owned(), 1)]);
        let binds_text = format!(
            "{keys_text}hl.config({{ general = {{ gaps_in = 2 }} }})\nshortcut(\"Window: Close\", \"SUPER + Q\", false)\n"
        );
        let option_area =
            |option: &str| (option == "general:gaps_in").then_some(Area::Multitasking);
        assert_eq!(
            move_lines(vec![(Area::Binds, binds_text)], &option_area),
            vec![
                (
                    Area::Binds,
                    format!("{keys_text}shortcut(\"Window: Close\", \"SUPER + Q\", false)\n")
                ),
                (
                    Area::Multitasking,
                    "hl.config({ general = { gaps_in = 2 } })\n".to_owned()
                ),
            ]
        );
    }
}
