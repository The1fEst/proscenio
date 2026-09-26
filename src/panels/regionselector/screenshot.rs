use std::path::{Path, PathBuf};

use crate::services::cliphist::escape;

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
    pub shadow: bool,
    pub radius: f64,
}

pub fn temp_dir() -> PathBuf {
    crate::core::paths::runtime().join("screenshot")
}

pub fn temp_path(screen: &str) -> PathBuf {
    temp_dir().join(format!("image-{screen}"))
}

pub fn temp_command(screen: &str, path: &Path, pointer: bool) -> String {
    format!(
        "mkdir -p '{}' && grim {}-o '{}' '{}'",
        escape(&temp_dir().to_string_lossy()),
        cursor_flag(pointer),
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
    let transparent = radius > 0.0 || crop.shadow;
    let crop_base = format!(
        "magick '{path}' -crop {width}x{height}+{x}+{y} +repage{}{}",
        if radius > 0.0 {
            corner_filter.as_str()
        } else {
            ""
        },
        if crop.shadow { shadow_filter } else { "" },
    );
    let crop_to_stdout = format!("{crop_base} {}", if transparent { "png:-" } else { "-" });
    let cleanup = format!("rm '{path}'");
    match action {
        Action::Copy if save_dir.is_empty() => {
            format!("{crop_to_stdout} | wl-copy && {cleanup}")
        }
        Action::Copy => format!(
            "mkdir -p '{dir}' && saveFileName=\"screenshot-$(date '+%Y-%m-%d_%H.%M.%S').png\" && savePath=\"{save_dir}/$saveFileName\" && {crop_to_stdout} | tee >(wl-copy) > \"$savePath\" && {cleanup}",
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

pub fn delayed(
    command: String,
    seconds: i64,
    screen: &str,
    path: Option<&Path>,
    pointer: bool,
) -> String {
    if seconds <= 0 {
        return command;
    }
    let again = path
        .map(|path| {
            format!(
                " && grim {}-o '{}' '{}'",
                cursor_flag(pointer),
                escape(screen),
                escape(&path.to_string_lossy())
            )
        })
        .unwrap_or_default();
    format!("sleep {seconds}{again} && {command}")
}

fn cursor_flag(pointer: bool) -> &'static str {
    if pointer { "-c " } else { "" }
}
