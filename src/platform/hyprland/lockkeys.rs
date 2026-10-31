use crate::platform::hyprland::workspacekeys::find_block;

pub const EVENT: &str = "proscenio-lock-key";
const BEGIN_LINE: &str = "-- proscenio lock keys";
const END_LINE: &str = "-- end proscenio lock keys";
const KEYS: [&str; 2] = ["Caps_Lock", "Num_Lock"];

fn render_block() -> String {
    let mut block = format!("{BEGIN_LINE}\n");
    for key in KEYS {
        block.push_str(&format!(
            "hl.bind(\"{key}\", hl.dsp.event(\"{EVENT}\"), {{ release = true, non_consuming = true, ignore_mods = true }})\n"
        ));
    }
    block.push_str(END_LINE);
    block.push('\n');
    block
}

pub fn with_lock_keys(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let block = render_block();
    let (before_lines, after_lines) = match find_block(&lines, BEGIN_LINE, END_LINE) {
        Some(range) => (&lines[..range.start], &lines[range.end..]),
        None => (&lines[..0], lines.as_slice()),
    };
    let mut new_text = String::new();
    for line in before_lines {
        new_text.push_str(line);
        new_text.push('\n');
    }
    new_text.push_str(&block);
    for line in after_lines
        .iter()
        .skip_while(|line| before_lines.is_empty() && line.trim().is_empty())
    {
        new_text.push_str(line);
        new_text.push('\n');
    }
    new_text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::hyprland::workspacekeys;

    const BLOCK: &str = "-- proscenio lock keys\nhl.bind(\"Caps_Lock\", hl.dsp.event(\"proscenio-lock-key\"), { release = true, non_consuming = true, ignore_mods = true })\nhl.bind(\"Num_Lock\", hl.dsp.event(\"proscenio-lock-key\"), { release = true, non_consuming = true, ignore_mods = true })\n-- end proscenio lock keys\n";
    const SHORTCUT: &str = "shortcut(\"Window: Close\", \"SUPER + Q\", false)\n";

    #[test]
    fn a_missing_block_is_written_on_top_and_a_second_write_changes_nothing() {
        let text = with_lock_keys(&format!("\n{SHORTCUT}"));
        assert_eq!(text, format!("{BLOCK}{SHORTCUT}"));
        assert_eq!(with_lock_keys(&text), text);
        assert_eq!(with_lock_keys(""), BLOCK);
    }

    #[test]
    fn a_different_block_is_replaced_where_it_stands() {
        let stale_block = BLOCK.replace("release = true, ", "");
        let text = format!("{SHORTCUT}{stale_block}hl.env(\"A\", \"b\")\n");
        assert_eq!(
            with_lock_keys(&text),
            format!("{SHORTCUT}{BLOCK}hl.env(\"A\", \"b\")\n")
        );
    }

    #[test]
    fn a_marker_without_its_end_is_not_taken_for_a_block() {
        let text = format!("{BEGIN_LINE}\n{SHORTCUT}");
        let new_text = with_lock_keys(&text);
        assert_eq!(new_text, format!("{BLOCK}{text}"));
    }

    #[test]
    fn the_block_and_the_workspace_keys_leave_each_other_in_place() {
        let starts = [("DP-1".to_owned(), 1)];
        let keys_text = workspacekeys::with_keys(&with_lock_keys(SHORTCUT), 5, &starts);
        assert!(keys_text.starts_with(workspacekeys::BEGIN_LINE));
        assert!(keys_text.contains(&format!("{BLOCK}{SHORTCUT}")));
        assert_eq!(with_lock_keys(&keys_text), keys_text);
        assert_eq!(workspacekeys::with_keys(&keys_text, 5, &starts), keys_text);
    }
}
