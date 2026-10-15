use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::glib;
use serde_json::Value;
use std::collections::HashSet;
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::core::i18n::{tr, trf};
use crate::core::{assets, config, gsettings, paths, process};
use crate::platform::dbus::notify::{self, Notification};
use crate::platform::hyprland::hypr;
use crate::platform::system::appearance;
use crate::platform::wayland::gbm;
use crate::services::files::cliphist::escape;
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
    let exit_code = start(arguments, &mut jobs);
    for job in jobs {
        let _ = job.join();
    }
    exit_code
}

fn start(arguments: &[String], jobs: &mut Vec<JoinHandle<()>>) -> glib::ExitCode {
    let mut image = None;
    let mut mode = None;
    let mut scheme = None;
    let mut keeps_wallpaper = false;
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
                    let picked_color =
                        process::output(&["hyprpicker", "--no-fancy"]).unwrap_or_default();
                    set_accent(picked_color.split_whitespace().next().unwrap_or_default());
                }
            },
            "--image" => image = rest.next().cloned(),
            "--noswitch" => {
                keeps_wallpaper = true;
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

    if image.is_none() && color.is_none() && !keeps_wallpaper {
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

    if image.is_some() && !keeps_wallpaper {
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

    switch(image, mode, scheme, color, !keeps_wallpaper, jobs)
}

fn switch(
    image: Option<String>,
    mode: Option<String>,
    scheme: String,
    color: Option<String>,
    should_offer_upscale: bool,
    jobs: &mut Vec<JoinHandle<()>>,
) -> glib::ExitCode {
    let mut matugen_command = strings(&["matugen", "--source-color-index", "0"]);
    let mut generate_arguments;
    if let Some(color) = color {
        matugen_command.extend(strings(&["color", "hex", &color]));
        generate_arguments = strings(&["--color", &color]);
    } else {
        let Some(image) = image else {
            println!("Aborted");
            return glib::ExitCode::SUCCESS;
        };
        if should_offer_upscale {
            jobs.push(thread::spawn({
                let image = image.clone();
                move || offer_upscale(&image)
            }));
        }
        process::run(&["pkill", "-9", "-x", "mpvpaper"]);

        let source = if is_video(&image) {
            let missing_programs: Vec<&str> = ["mpvpaper", "ffmpeg"]
                .into_iter()
                .filter(|program| !process::exists(program))
                .collect();
            if !missing_programs.is_empty() {
                offer_install(&missing_programs);
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
        matugen_command.extend(strings(&["image", &source]));
        generate_arguments = strings(&["--path", &source]);
    }

    let mode = mode.filter(|mode| !mode.is_empty()).unwrap_or_else(|| {
        if gsettings::prefers_dark() {
            "dark"
        } else {
            "light"
        }
        .to_owned()
    });
    matugen_command.extend(strings(&["--mode", &mode]));
    let force_dark_setting =
        config::value(&format!("{THEMING}/terminalGenerationProps/forceDarkMode"));
    let terminal_mode = if force_dark_setting == Some(Value::Bool(true)) {
        "dark"
    } else {
        &mode
    };
    generate_arguments.extend(strings(&[
        "--mode",
        &mode,
        "--terminal-mode",
        terminal_mode,
    ]));
    matugen_command.extend(strings(&["--type", &scheme]));
    generate_arguments.extend(strings(&[
        "--scheme",
        &scheme,
        "--blend_bg_fg",
        "--cache",
        &paths::generated_dir().join("color.txt").to_string_lossy(),
    ]));

    let dark_mode = match mode.as_str() {
        "dark" => Some(true),
        "light" => Some(false),
        _ => None,
    };
    if let Some(is_dark) = dark_mode {
        gsettings::set_dark(is_dark);
        appearance::follow_mode(is_dark);
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
            generate_arguments.extend([flag.to_owned(), value]);
        }
    }

    process::run(&matugen_command);
    let scss = match colors::generate(&generate_arguments, Some(assets::TERMINAL_SCHEME)) {
        Ok(scss) => scss,
        Err(error) => {
            eprintln!("{error}");
            return glib::ExitCode::FAILURE;
        }
    };
    let _ = std::fs::create_dir_all(paths::generated_dir());
    if let Err(error) = std::fs::write(paths::generated_dir().join("material_colors.scss"), &scss) {
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
    let find_largest = |key: &str| {
        monitors
            .iter()
            .filter_map(|monitor| monitor.get(key)?.as_i64())
            .max()
    };
    let (Some(width), Some(height)) = (find_largest("width"), find_largest("height")) else {
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
        let should_install = ask(
            &tr("Install Upscayl?"),
            "paru -S upscayl-bin",
            "im.error",
            ("install_upscayl", &tr("Install Upscayl (Arch)")),
        );
        if !should_install {
            return;
        }
        if let Some(command) = updates::manager::install_command(&["upscayl-bin"]) {
            process::run(&command);
        }
        if !process::exists("upscayl") {
            return;
        }
    }
    process::detach(&["upscayl"]);
}

fn offer_install(missing_programs: &[&str]) {
    let names = missing_programs.join(" ");
    println!("Missing deps: {names}");
    println!("Arch: sudo pacman -S {names}");
    let should_install = ask(
        &tr("Can't switch to video wallpaper"),
        &trf("Missing dependencies: %1", &[&names]),
        "im.error",
        ("install_arch", &tr("Install (Arch)")),
    );
    if !should_install {
        return;
    }
    if let Some(command) = updates::manager::install_command(missing_programs) {
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
    let response = notify::send_blocking(&Notification {
        app: APP_NAME,
        summary,
        body,
        category,
        actions: &[action],
        ..Default::default()
    });
    response.as_deref() == Some(action.0)
}

fn write_restore(video: Option<&str>) {
    let path = glib::user_config_dir().join("hypr/custom/scripts/__restore_video_wallpaper.sh");
    let body = match video {
        Some(video) => {
            let time = glib::DateTime::now_local()
                .ok()
                .and_then(|now| now.format("%c").ok())
                .unwrap_or_default();
            restore_script(video, &time)
        }
        None => "#!/bin/bash\n# The content of this script will be generated by switchwall.sh - Don't modify it by yourself.\n".to_owned(),
    };
    install_script(&path, &body);
}

fn install_script(path: &Path, body: &str) {
    let staging_path = path.with_extension("sh.tmp");
    if std::fs::write(&staging_path, body).is_err() {
        return;
    }
    let _ = std::fs::set_permissions(&staging_path, std::fs::Permissions::from_mode(0o755));
    let _ = std::fs::rename(&staging_path, path);
}

fn restore_script(video: &str, time: &str) -> String {
    let video = escape(video);
    format!(
        "#!/bin/bash\n# Generated by switchwall.sh - Don't modify it by yourself.\n# Time: {time}\n\npkill -f -9 mpvpaper\n\nfor monitor in $(hyprctl monitors -j | jq -r '.[] | .name'); do\n    mpvpaper -o \"{VIDEO_OPTIONS}\" \"$monitor\" '{video}' &\n    sleep 0.1\ndone\n"
    )
}

fn apply_terminal(scss: &str) {
    let terminal_colors: Vec<(&str, &str)> = scss
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            Some((name, value.trim().trim_end_matches(';')))
        })
        .collect();
    let output_dir = paths::generated_dir().join("terminal");
    let _ = std::fs::create_dir_all(&output_dir);

    let _ = std::fs::write(
        output_dir.join("kitty-theme.conf"),
        fill(assets::KITTY_THEME, &terminal_colors),
    );
    process::run(&["pkill", "-USR1", "-x", "kitty"]);

    let sequences = terminal_sequences(&terminal_colors);
    let _ = std::fs::write(output_dir.join("sequences.txt"), &sequences);
    let Ok(entries) = std::fs::read_dir("/dev/pts") else {
        return;
    };
    let shell_terminals = read_controlling_terminals();
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name
            .to_string_lossy()
            .bytes()
            .all(|byte| byte.is_ascii_digit())
        {
            continue;
        }
        let is_shell_terminal = entry.metadata().is_ok_and(|metadata| {
            shell_terminals.contains(&to_terminal_number(gbm::numbers(metadata.rdev())))
        });
        if !is_shell_terminal {
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

fn terminal_sequences(terminal_colors: &[(&str, &str)]) -> String {
    fill(assets::TERMINAL_SEQUENCES, terminal_colors)
        .lines()
        .map(|line| line.replace("\\e", "\x1b"))
        .collect::<String>()
        .replace("$alpha", TERMINAL_ALPHA)
}

fn read_controlling_terminals() -> HashSet<u64> {
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return HashSet::new();
    };
    entries
        .flatten()
        .filter_map(|entry| std::fs::read_to_string(entry.path().join("stat")).ok())
        .filter_map(|stat| parse_controlling_terminal(&stat))
        .collect()
}

fn parse_controlling_terminal(stat: &str) -> Option<u64> {
    let fields = stat.get(stat.rfind(')')? + 1..)?;
    fields
        .split_whitespace()
        .nth(4)?
        .parse()
        .ok()
        .filter(|&number| number != 0)
}

fn to_terminal_number((major, minor): (u64, u64)) -> u64 {
    (minor & 0xff) | (major << 8) | ((minor & !0xff) << 12)
}

fn fill(template: &str, terminal_colors: &[(&str, &str)]) -> String {
    let mut text = template.to_owned();
    for (name, value) in terminal_colors {
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
    if let Some(source) = std::fs::read_to_string(paths::generated_dir().join("color.txt"))
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
    let Ok(color) = std::fs::read_to_string(paths::generated_dir().join("color.txt")) else {
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
        let new_text = set_code_color(&text, color);
        if new_text != text {
            let _ = std::fs::write(&path, new_text);
        }
    }
}

fn set_code_color(text: &str, color: &str) -> String {
    if let Some(key_index) = text.find(CODE_KEY) {
        let after_key = key_index + CODE_KEY.len();
        let colon_index = after_key + count_leading_whitespace(&text[after_key..]);
        if !text[colon_index..].starts_with(':') {
            return text.to_owned();
        }
        let quote_index = colon_index + 1 + count_leading_whitespace(&text[colon_index + 1..]);
        if !text[quote_index..].starts_with('"') {
            return text.to_owned();
        }
        let Some(length) = text[quote_index + 1..].find('"') else {
            return text.to_owned();
        };
        let end_index = quote_index + 1 + length;
        return format!("{}{color}{}", &text[..quote_index + 1], &text[end_index..]);
    }
    let Some(close_index) = text.rfind('}') else {
        return text.to_owned();
    };
    let before = text[..close_index].trim_end();
    let separator = if before.ends_with('{') || before.ends_with(',') {
        ""
    } else {
        ","
    };
    format!(
        "{before}{separator}\n  {CODE_KEY}: \"{color}\"\n{}",
        &text[close_index..]
    )
}

fn count_leading_whitespace(text: &str) -> usize {
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
    fn the_restore_script_passes_the_video_path_literally() {
        let script = restore_script("/videos/a \"b\" $(touch x) `c` it's.mp4", "now");
        let line = script
            .lines()
            .find(|line| line.trim_start().starts_with("mpvpaper"))
            .unwrap();
        assert_eq!(
            line.trim(),
            format!(
                "mpvpaper -o \"{VIDEO_OPTIONS}\" \"$monitor\" '/videos/a \"b\" $(touch x) `c` it'\\''s.mp4' &"
            )
        );
    }

    #[test]
    fn an_installed_script_is_executable() {
        let folder = std::env::temp_dir().join(format!("proscenio-restore-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("restore.sh");
        install_script(&path, "#!/bin/bash\n");
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(mode & 0o111, 0o111);
    }

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

    #[test]
    fn only_a_shell_terminal_is_one_some_process_controls() {
        let shell = "4242 (fish (login)) S 1 4242 4242 34818 4242 4194560 0 0";
        let daemon = "1005 (kded6) S 1 1005 1005 0 -1 4194560 0 0";
        assert_eq!(parse_controlling_terminal(shell), Some(34818));
        assert_eq!(parse_controlling_terminal(daemon), None);
        assert_eq!(to_terminal_number((136, 2)), 34818);
        assert_eq!(to_terminal_number((136, 300)), 1_083_436);
    }
}
