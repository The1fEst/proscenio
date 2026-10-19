use std::path::{Path, PathBuf};

use crate::platform::hyprland::hyprconfig;

pub fn path() -> PathBuf {
    hyprconfig::settings_path("hyprlock.conf")
}

fn render(is_twelve_hour: bool) -> String {
    let time_variable = if is_twelve_hour { "$TIME12" } else { "$TIME" };
    format!("$clock_text = {time_variable}\n")
}

pub fn write(path: &Path, is_twelve_hour: bool) -> std::io::Result<bool> {
    let text = render(is_twelve_hour);
    if hyprconfig::read_file_for_edit(path)? == text {
        return Ok(false);
    }
    hyprconfig::replace_file(path, &text)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_clock_file_names_the_time_variable_of_the_format() {
        assert_eq!(render(true), "$clock_text = $TIME12\n");
        assert_eq!(render(false), "$clock_text = $TIME\n");
    }

    #[test]
    fn the_clock_file_is_created_and_rewritten_only_when_the_format_changes() {
        let root =
            std::env::temp_dir().join(format!("proscenio-hyprlockclock-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("settings/hyprlock.conf");

        assert!(write(&path, false).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "$clock_text = $TIME\n"
        );
        assert!(!write(&path, false).unwrap());
        assert!(write(&path, true).unwrap());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "$clock_text = $TIME12\n"
        );

        std::fs::remove_dir_all(&root).unwrap();
    }
}
