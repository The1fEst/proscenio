use std::path::{Path, PathBuf};

use crate::services::files::cliphist::escape;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Copy,
    Edit,
    Record,
    RecordWithSound,
}

pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub has_shadow: bool,
    pub radius: f64,
}

pub fn temp_dir() -> PathBuf {
    crate::core::paths::runtime().join("screenshot")
}

pub fn temp_path(screen: &str) -> PathBuf {
    temp_dir().join(format!("image-{screen}"))
}

pub fn temp_command(screen: &str, path: &Path, shows_pointer: bool) -> String {
    format!(
        "mkdir -p '{}' && grim {}-o '{}' '{}'",
        escape(&temp_dir().to_string_lossy()),
        cursor_flag(shows_pointer),
        escape(screen),
        escape(&path.to_string_lossy()),
    )
}

pub fn command(
    crop: &Crop,
    path: &Path,
    action: Action,
    save_dir: &str,
    record_region: &str,
    recorder: &str,
) -> String {
    let (x, y) = (crop.x.round(), crop.y.round());
    let (width, height) = (crop.width.round(), crop.height.round());
    let radius = crop.radius.round();
    let path = escape(&path.to_string_lossy());
    let corner_filter = format!(
        " \\( +clone -alpha extract -draw \"fill black polygon 0,0 0,{radius} {radius},0 fill white circle {radius},{radius} {radius},0\" \\( +clone -flip \\) -compose Multiply -composite \\( +clone -flop \\) -compose Multiply -composite \\) -alpha off -compose CopyOpacity -composite -compose Over"
    );
    let shadow_filter = " \\( +clone -background black -shadow 75x28+0+22 \\) +swap -background none -layers merge +repage";
    let is_transparent = radius > 0.0 || crop.has_shadow;
    let crop_base = format!(
        "magick '{path}' -crop {width}x{height}+{x}+{y} +repage{}{}",
        if radius > 0.0 {
            corner_filter.as_str()
        } else {
            ""
        },
        if crop.has_shadow { shadow_filter } else { "" },
    );
    let crop_to_stdout = format!("{crop_base} {}", if is_transparent { "png:-" } else { "-" });
    let cleanup = format!("rm '{path}'");
    match action {
        Action::Copy if save_dir.is_empty() => {
            format!("{crop_to_stdout} | wl-copy && {cleanup}")
        }
        Action::Copy => format!(
            "mkdir -p '{dir}' && saveFileName=\"screenshot-$(date '+%Y-%m-%d_%H.%M.%S').png\" && savePath='{dir}'/\"$saveFileName\" && {crop_to_stdout} | tee >(wl-copy) > \"$savePath\" && {cleanup}",
            dir = escape(save_dir),
        ),
        Action::Edit => format!("{crop_to_stdout} | satty -f - && {cleanup}"),
        Action::Record => format!("'{}' record --region '{record_region}'", escape(recorder)),
        Action::RecordWithSound => format!(
            "'{}' record --region '{record_region}' --sound",
            escape(recorder)
        ),
    }
}

pub fn delayed_command(
    command: String,
    seconds: i64,
    screen: &str,
    path: Option<&Path>,
    shows_pointer: bool,
) -> String {
    if seconds <= 0 {
        return command;
    }
    let recapture_suffix = path
        .map(|path| {
            format!(
                " && grim {}-o '{}' '{}'",
                cursor_flag(shows_pointer),
                escape(screen),
                escape(&path.to_string_lossy())
            )
        })
        .unwrap_or_default();
    format!("sleep {seconds}{recapture_suffix} && {command}")
}

fn cursor_flag(shows_pointer: bool) -> &'static str {
    if shows_pointer { "-c " } else { "" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_saves_into_a_folder_whose_name_has_shell_characters() {
        let root = std::env::temp_dir().join(format!("proscenio-save-{}", std::process::id()));
        let folder = root.join("a\"b$HOME`c'd");
        let crop = Crop {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
            has_shadow: false,
            radius: 0.0,
        };
        let script = command(
            &crop,
            Path::new("/nonexistent"),
            Action::Copy,
            &folder.to_string_lossy(),
            "",
            "",
        );
        let setup = script.split(" && magick ").next().unwrap();
        let output = std::process::Command::new("bash")
            .arg("-c")
            .arg(format!("{setup} && printf %s \"$savePath\""))
            .output()
            .unwrap();
        let folder_exists = folder.is_dir();
        let _ = std::fs::remove_dir_all(&root);
        let save_path = String::from_utf8_lossy(&output.stdout);
        assert!(
            folder_exists && save_path.starts_with(&format!("{}/screenshot-", folder.display())),
            "{save_path:?} {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
