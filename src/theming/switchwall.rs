use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::glib;
use serde_json::Value;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::core::i18n::{tr, trf};
use crate::core::{assets, config, gsettings, paths, process};
use crate::platform::hypr;
use crate::platform::notify::{self, Notification};
use crate::services::updates;
use crate::theming::{colors, kde};

const SCHEMES: [&str; 8] = [
    "scheme-content",
    "scheme-expressive",
    "scheme-fidelity",
    "scheme-fruit-salad",
    "scheme-monochrome",
    "scheme-neutral",
    "scheme-rainbow",
    "scheme-tonal-spot",
];
const FALLBACK_SCHEME: &str = "scheme-tonal-spot";
const VIDEOS: [&str; 5] = ["mp4", "webm", "mkv", "avi", "mov"];
const VIDEO_OPTIONS: &str = "no-audio loop hwdec=auto scale=bilinear interpolation=no video-sync=display-resample panscan=1.0 video-scale-x=1.0 video-scale-y=1.0 video-align-x=0.5 video-align-y=0.5 load-scripts=no";
const EDITORS: [&str; 7] = [
    "Code",
    "VSCodium",
    "Code - OSS",
    "Code - Insiders",
    "Cursor",
    "Antigravity",
    "Windsurf",
];
const CODE_KEY: &str = "\"material-code.primaryColor\"";
const TERMINAL_ALPHA: &str = "100";
const O_NONBLOCK: i32 = 0o4000;
const APP_NAME: &str = "Wallpaper switcher";
const THEMING: &str = "/appearance/wallpaperTheming";

pub fn detach(arguments: &[&str]) {
    let mut command = vec!["switchwall"];
    command.extend_from_slice(arguments);
    process::launch_subcommand(&command);
}

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let mut jobs = Vec::new();
    let code = start(arguments, &mut jobs);
    for job in jobs {
        let _ = job.join();
    }
    code
}

fn start(arguments: &[String], jobs: &mut Vec<JoinHandle<()>>) -> glib::ExitCode {
    let mut image = None;
    let mut mode = None;
    let mut scheme = None;
    let mut noswitch = false;
    let mut rest = arguments.iter().peekable();
    while let Some(argument) = rest.next() {
        match argument.as_str() {
            "--mode" => mode = rest.next().cloned(),
            "--type" => scheme = rest.next().cloned(),
            "--color" => match rest.peek().copied().map(String::as_str) {
                Some(value) if is_hex(value) => {
                    set_accent(value);
                    rest.next();
                }
                Some("clear") => {
                    set_accent("");
                    rest.next();
                }
                _ => {
                    let picked = process::output(&["hyprpicker", "--no-fancy"]).unwrap_or_default();
                    set_accent(picked.split_whitespace().next().unwrap_or_default());
                }
            },
            "--image" => image = rest.next().cloned(),
            "--noswitch" => {
                noswitch = true;
                image = config_string("/background/wallpaperPath");
            }
            _ => {
                if image.is_none() {
                    image = Some(argument.clone());
                }
            }
        }
    }
    let mut image = image.filter(|path| !path.is_empty());

    let mut color = config_string("/appearance/palette/accentColor").filter(|value| is_hex(value));
    let mut scheme = scheme
        .or_else(|| config_string("/appearance/palette/type"))
        .unwrap_or_else(|| "auto".to_owned());
    if scheme != "auto" && !SCHEMES.contains(&scheme.as_str()) {
        eprintln!("[switchwall] Warning: Invalid type '{scheme}', defaulting to 'auto'");
        scheme = "auto".to_owned();
    }

    if image.is_none() && color.is_none() && !noswitch {
        let folder = pictures().to_string_lossy().into_owned();
        image = process::output(&[
            "kdialog",
            "--getopenfilename",
            &folder,
            "--title",
            &tr("Choose wallpaper"),
        ])
        .filter(|path| !path.is_empty());
    }

    if image.is_some() && !noswitch {
        set_accent("");
        color = None;
    }

    if scheme == "auto" {
        scheme = match image.as_deref().filter(|path| Path::new(path).is_file()) {
            Some(path) => match colors::scheme_for_image(&[path.to_owned()]) {
                Ok(detected) if SCHEMES.contains(&detected.trim()) => detected.trim().to_owned(),
                _ => {
                    eprintln!(
                        "[switchwall] Warning: Could not auto-detect a valid scheme, defaulting to '{FALLBACK_SCHEME}'"
                    );
                    FALLBACK_SCHEME.to_owned()
                }
            },
            None => {
                eprintln!(
                    "[switchwall] Warning: No image to auto-detect scheme from, defaulting to '{FALLBACK_SCHEME}'"
                );
                FALLBACK_SCHEME.to_owned()
            }
        };
    }

    switch(image, mode, scheme, color, !noswitch, jobs)
}

fn switch(
    image: Option<String>,
    mode: Option<String>,
    scheme: String,
    color: Option<String>,
    switched: bool,
    jobs: &mut Vec<JoinHandle<()>>,
) -> glib::ExitCode {
    let mut matugen = strings(&["matugen", "--source-color-index", "0"]);
    let mut generate;
    if let Some(color) = color {
        matugen.extend(strings(&["color", "hex", &color]));
        generate = strings(&["--color", &color]);
    } else {
        let Some(image) = image else {
            println!("Aborted");
            return glib::ExitCode::SUCCESS;
        };
        if switched {
            jobs.push(thread::spawn({
                let image = image.clone();
                move || offer_upscale(&image)
            }));
        }
        process::run(&["pkill", "-9", "-x", "mpvpaper"]);

        let source = if is_video(&image) {
            let missing: Vec<&str> = ["mpvpaper", "ffmpeg"]
                .into_iter()
                .filter(|program| !process::exists(program))
                .collect();
            if !missing.is_empty() {
                offer_install(&missing);
                return glib::ExitCode::SUCCESS;
            }
            config::store_value("/background/wallpaperPath", Value::from(image.as_str()));
            for monitor in monitors() {
                if let Some(name) = monitor.get("name").and_then(Value::as_str) {
                    process::detach(&["mpvpaper", "-o", VIDEO_OPTIONS, name, &image]);
                    thread::sleep(Duration::from_millis(100));
                }
            }
            let _ = std::fs::create_dir_all(thumbnails());
            let name = Path::new(&image).file_name().unwrap_or_default();
            let mut thumbnail = thumbnails().join(name).into_os_string();
            thumbnail.push(".jpg");
            let thumbnail = thumbnail.to_string_lossy().into_owned();
            process::output(&["ffmpeg", "-y", "-i", &image, "-vframes", "1", &thumbnail]);
            config::store_value("/background/thumbnailPath", Value::from(thumbnail.as_str()));
            if !Path::new(&thumbnail).is_file() {
                println!("Cannot create image to colorgen");
                write_restore(None);
                return glib::ExitCode::FAILURE;
            }
            write_restore(Some(&image));
            thumbnail
        } else {
            config::store_value("/background/wallpaperPath", Value::from(image.as_str()));
            write_restore(None);
            image
        };
        matugen.extend(strings(&["image", &source]));
        generate = strings(&["--path", &source]);
    }

    let mode = mode.filter(|mode| !mode.is_empty()).unwrap_or_else(|| {
        if gsettings::prefers_dark() {
            "dark"
        } else {
            "light"
        }
        .to_owned()
    });
    matugen.extend(strings(&["--mode", &mode]));
    let force_dark = config::value(&format!("{THEMING}/terminalGenerationProps/forceDarkMode"));
    let terminal_mode = if force_dark == Some(Value::Bool(true)) {
        "dark"
    } else {
        &mode
    };
    generate.extend(strings(&[
        "--mode",
        &mode,
        "--terminal-mode",
        terminal_mode,
    ]));
    matugen.extend(strings(&["--type", &scheme]));
    generate.extend(strings(&[
        "--scheme",
        &scheme,
        "--blend_bg_fg",
        "--cache",
        &paths::generated().join("color.txt").to_string_lossy(),
    ]));

    match mode.as_str() {
        "dark" => gsettings::set_dark(true),
        "light" => gsettings::set_dark(false),
        _ => {}
    }

    if config::value(&format!("{THEMING}/enableAppsAndShell")) == Some(Value::Bool(false)) {
        println!("App and shell theming disabled, skipping matugen and color generation");
        return glib::ExitCode::SUCCESS;
    }

    for (key, flag) in [
        ("harmony", "--harmony"),
        ("harmonizeThreshold", "--harmonize_threshold"),
        ("termFgBoost", "--term_fg_boost"),
    ] {
        let value = match config::value(&format!("{THEMING}/terminalGenerationProps/{key}")) {
            None | Some(Value::Null) => continue,
            Some(Value::String(text)) => text,
            Some(other) => other.to_string(),
        };
        if !value.is_empty() {
            generate.extend([flag.to_owned(), value]);
        }
    }

    process::run(&matugen);
    let scss = match colors::generate(&generate, Some(assets::TERMINAL_SCHEME)) {
        Ok(scss) => scss,
        Err(error) => {
            eprintln!("{error}");
            return glib::ExitCode::FAILURE;
        }
    };
    let _ = std::fs::create_dir_all(paths::generated());
    if let Err(error) = std::fs::write(paths::generated().join("material_colors.scss"), &scss) {
        eprintln!("material_colors.scss: {error}");
        return glib::ExitCode::FAILURE;
    }

    if config::value(&format!("{THEMING}/enableTerminal")) != Some(Value::Bool(false)) {
        jobs.push(thread::spawn(move || apply_terminal(&scss)));
    }
    jobs.push(thread::spawn(move || apply_kde(&scheme)));
    jobs.push(thread::spawn(apply_code));
    glib::ExitCode::SUCCESS
}

fn offer_upscale(image: &str) {
    let monitors = monitors();
    let largest = |key: &str| {
        monitors
            .iter()
            .filter_map(|monitor| monitor.get(key)?.as_i64())
            .max()
    };
    let (Some(width), Some(height)) = (largest("width"), largest("height")) else {
        return;
    };
    if !Path::new(image).is_file() {
        return;
    }
    let (image_width, image_height) = if is_video(image) {
        (width, height)
    } else {
        let Some((_, image_width, image_height)) = Pixbuf::file_info(image) else {
            return;
        };
        (i64::from(image_width), i64::from(image_height))
    };
    if image_width >= width && image_height >= height {
        return;
    }
    let body = trf(
        "Image resolution (%1x%2) is lower than screen resolution (%3x%4)",
        &[
            &image_width.to_string(),
            &image_height.to_string(),
            &width.to_string(),
            &height.to_string(),
        ],
    );
    if !ask(
        &tr("Upscale?"),
        &body,
        "",
        ("open_upscayl", &tr("Open Upscayl")),
    ) {
        return;
    }
    if !process::exists("upscayl") {
        let install = ask(
            &tr("Install Upscayl?"),
            "paru -S upscayl-bin",
            "im.error",
            ("install_upscayl", &tr("Install Upscayl (Arch)")),
        );
        if !install {
            return;
        }
        if let Some(command) = updates::install_command(&["upscayl-bin"]) {
            process::run(&command);
        }
        if !process::exists("upscayl") {
            return;
        }
    }
    process::detach(&["upscayl"]);
}

fn offer_install(missing: &[&str]) {
    let names = missing.join(" ");
    println!("Missing deps: {names}");
    println!("Arch: sudo pacman -S {names}");
    let install = ask(
        &tr("Can't switch to video wallpaper"),
        &trf("Missing dependencies: %1", &[&names]),
        "im.error",
        ("install_arch", &tr("Install (Arch)")),
    );
    if !install {
        return;
    }
    if let Some(command) = updates::install_command(missing) {
        process::run(&command);
    }
    if process::exists("mpvpaper") && process::exists("ffmpeg") {
        notify::send_blocking(&Notification {
            app: APP_NAME,
            summary: &tr(APP_NAME),
            body: &tr("Alright, try again!"),
            ..Default::default()
        });
    }
}

fn ask(summary: &str, body: &str, category: &str, action: (&str, &str)) -> bool {
    let chosen = notify::send_blocking(&Notification {
        app: APP_NAME,
        summary,
        body,
        category,
        actions: &[action],
        ..Default::default()
    });
    chosen.as_deref() == Some(action.0)
}

fn write_restore(video: Option<&str>) {
    let path = glib::user_config_dir().join("hypr/custom/scripts/__restore_video_wallpaper.sh");
    let body = match video {
        Some(video) => {
            let time = glib::DateTime::now_local()
                .ok()
                .and_then(|now| now.format("%c").ok())
                .unwrap_or_default();
            format!(
                "#!/bin/bash\n# Generated by switchwall.sh - Don't modify it by yourself.\n# Time: {time}\n\npkill -f -9 mpvpaper\n\nfor monitor in $(hyprctl monitors -j | jq -r '.[] | .name'); do\n    mpvpaper -o \"{VIDEO_OPTIONS}\" \"$monitor\" \"{video}\" &\n    sleep 0.1\ndone\n"
            )
        }
        None => "#!/bin/bash\n# The content of this script will be generated by switchwall.sh - Don't modify it by yourself.\n".to_owned(),
    };
    let staging = path.with_extension("sh.tmp");
    if std::fs::write(&staging, body).is_err() || std::fs::rename(&staging, &path).is_err() {
        return;
    }
    if video.is_some() {
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
}

fn apply_terminal(scss: &str) {
    let colours: Vec<(&str, &str)> = scss
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name, value.trim().trim_end_matches(';')))
        })
        .collect();
    let output = paths::generated().join("terminal");
    let _ = std::fs::create_dir_all(&output);

    let _ = std::fs::write(
        output.join("kitty-theme.conf"),
        fill(assets::KITTY_THEME, &colours),
    );
    process::run(&["pkill", "-USR1", "-x", "kitty"]);

    let sequences = terminal_sequences(&colours);
    let _ = std::fs::write(output.join("sequences.txt"), &sequences);
    let Ok(entries) = std::fs::read_dir("/dev/pts") else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        if let Ok(mut terminal) = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(O_NONBLOCK)
            .open(entry.path())
        {
            let _ = terminal.write_all(sequences.as_bytes());
        }
    }
}

fn terminal_sequences(colours: &[(&str, &str)]) -> String {
    fill(assets::TERMINAL_SEQUENCES, colours)
        .lines()
        .map(|line| line.replace("\\e", "\x1b"))
        .collect::<String>()
        .replace("$alpha", TERMINAL_ALPHA)
}

fn fill(template: &str, colours: &[(&str, &str)]) -> String {
    let mut text = template.to_owned();
    for (name, value) in colours {
        text = text.replace(
            &format!("{name} #"),
            value.strip_prefix('#').unwrap_or(value),
        );
    }
    text
}

fn apply_kde(scheme: &str) {
    if config::value(&format!("{THEMING}/enableQtApps")) == Some(Value::Bool(false)) {
        return;
    }
    if let Some(source) = std::fs::read_to_string(paths::generated().join("color.txt"))
        .ok()
        .and_then(|color| colors::parse_hex(color.trim()))
        && let Err(error) = kde::apply(source, scheme, gsettings::prefers_dark())
    {
        eprintln!("{error}");
    }
    if let Err(error) = colors::kde_selection() {
        eprintln!("{error}");
    }
}

fn apply_code() {
    let Ok(color) = std::fs::read_to_string(paths::generated().join("color.txt")) else {
        return;
    };
    let color = color.trim_end_matches('\n');
    for editor in EDITORS {
        let path = glib::user_config_dir()
            .join(editor)
            .join("User/settings.json");
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let updated = set_code_color(&text, color);
        if updated != text {
            let _ = std::fs::write(&path, updated);
        }
    }
}

fn set_code_color(text: &str, color: &str) -> String {
    if let Some(at) = text.find(CODE_KEY) {
        let after_key = at + CODE_KEY.len();
        let colon = after_key + whitespace(&text[after_key..]);
        if !text[colon..].starts_with(':') {
            return text.to_owned();
        }
        let quote = colon + 1 + whitespace(&text[colon + 1..]);
        if !text[quote..].starts_with('"') {
            return text.to_owned();
        }
        let Some(length) = text[quote + 1..].find('"') else {
            return text.to_owned();
        };
        let end = quote + 1 + length;
        return format!("{}{color}{}", &text[..quote + 1], &text[end..]);
    }
    let Some(close) = text.rfind('}') else {
        return text.to_owned();
    };
    let before = text[..close].trim_end();
    let separator = if before.ends_with('{') || before.ends_with(',') {
        ""
    } else {
        ","
    };
    format!(
        "{before}{separator}\n  {CODE_KEY}: \"{color}\"\n{}",
        &text[close..]
    )
}

fn whitespace(text: &str) -> usize {
    text.len() - text.trim_start().len()
}

fn set_accent(color: &str) {
    config::store_value("/appearance/palette/accentColor", Value::from(color));
}

fn config_string(pointer: &str) -> Option<String> {
    config::value(pointer)?.as_str().map(str::to_owned)
}

fn is_hex(text: &str) -> bool {
    let digits = text.strip_prefix('#').unwrap_or(text);
    digits.len() == 6 && digits.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_video(path: &str) -> bool {
    let extension = path
        .rsplit_once('.')
        .map_or(path, |(_, extension)| extension);
    VIDEOS.contains(&extension)
}

fn monitors() -> Vec<Value> {
    hypr::json("monitors")
        .and_then(|monitors| monitors.as_array().cloned())
        .unwrap_or_default()
}

fn pictures() -> PathBuf {
    let pictures =
        glib::user_special_dir(glib::UserDirectory::Pictures).unwrap_or_else(glib::home_dir);
    [
        pictures.join("Wallpapers/showcase"),
        pictures.join("Wallpapers"),
    ]
    .into_iter()
    .find(|folder| folder.is_dir())
    .unwrap_or(pictures)
}

fn thumbnails() -> PathBuf {
    glib::user_config_dir().join("hypr/custom/scripts/mpvpaper_thumbnails")
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_terminal_sequence_is_terminated() {
        let sequences = terminal_sequences(&[("$term0", "#1B1A1E")]);
        assert!(sequences.starts_with("\x1b]4;0;#1B1A1E\x1b\\"));
        assert!(!sequences.contains('\n'));
        let commands: Vec<&str> = sequences.split("\x1b]").skip(1).collect();
        assert_eq!(commands.len(), 45);
        for command in commands {
            assert!(command.ends_with("\x1b\\"), "unterminated: {command:?}");
        }
    }

    #[test]
    fn code_color_replaces_or_joins_the_settings() {
        let cases = [
            (
                "{\n  \"a\": 1,\n  \"material-code.primaryColor\" : \"#000000\"\n}\n",
                "{\n  \"a\": 1,\n  \"material-code.primaryColor\" : \"#ABCDEF\"\n}\n",
            ),
            (
                "{\n  \"a\": 1\n}\n",
                "{\n  \"a\": 1,\n  \"material-code.primaryColor\": \"#ABCDEF\"\n}\n",
            ),
            (
                "{\n  \"a\": 1,\n}\n",
                "{\n  \"a\": 1,\n  \"material-code.primaryColor\": \"#ABCDEF\"\n}\n",
            ),
            (
                "{}\n",
                "{\n  \"material-code.primaryColor\": \"#ABCDEF\"\n}\n",
            ),
        ];
        for (settings, expected) in cases {
            assert_eq!(set_code_color(settings, "#ABCDEF"), expected);
        }
    }
}
