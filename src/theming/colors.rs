use gtk4::gdk_pixbuf::{Pixbuf, PixbufAnimation};
use gtk4::glib;
use gtk4::prelude::*;
use material_colors::color::Argb;
use material_colors::hct::Hct;
use material_colors::utils::math::{difference_degrees, rotate_direction, sanitize_degrees_double};
use serde_json::Value;
use std::path::Path;

use crate::theming::material::{ROLES, Scheme};
use crate::theming::quantize;

pub const USAGE: &str = "usage: proscenio colors generate [--path IMAGE | --color HEX] [options]\n       proscenio colors scheme-for-image [--colorfulness] IMAGE\n       proscenio colors kde-selection\n";

const PRECISION_BITS: u32 = 32 - 8 - 2;
const QUANTIZE_COLORS: usize = 128;
const SCHEME_IMAGE_SIZE: u32 = 128;
const COLORFUL: f64 = 40.0;

const SUCCESS_DARK: [(&str, &str); 4] = [
    ("success", "#B5CCBA"),
    ("onSuccess", "#213528"),
    ("successContainer", "#374B3E"),
    ("onSuccessContainer", "#D1E9D6"),
];

const SUCCESS_LIGHT: [(&str, &str); 4] = [
    ("success", "#4F6354"),
    ("onSuccess", "#FFFFFF"),
    ("successContainer", "#D1E8D5"),
    ("onSuccessContainer", "#0C1F13"),
];

pub fn run(arguments: &[String]) -> glib::ExitCode {
    let result = match arguments.first().map(String::as_str) {
        Some("generate") => generate(&arguments[1..], None),
        Some("scheme-for-image") => scheme_for_image(&arguments[1..]),
        Some("kde-selection") => kde_selection(),
        _ => Err(USAGE.to_owned()),
    };
    match result {
        Ok(output) => {
            print!("{output}");
            glib::ExitCode::SUCCESS
        }
        Err(error) => {
            eprint!("{}", with_newline(error));
            glib::ExitCode::FAILURE
        }
    }
}

struct Options {
    path: Option<String>,
    size: u32,
    color: Option<String>,
    dark: bool,
    scheme: String,
    smart: bool,
    transparent: bool,
    termscheme: Option<String>,
    harmony: f64,
    harmonize_threshold: f64,
    term_fg_boost: f64,
    blend_bg_fg: bool,
    cache: Option<String>,
}

fn parse(arguments: &[String]) -> Result<Options, String> {
    let mut options = Options {
        path: None,
        size: 128,
        color: None,
        dark: true,
        scheme: "vibrant".to_owned(),
        smart: false,
        transparent: false,
        termscheme: None,
        harmony: 0.8,
        harmonize_threshold: 100.0,
        term_fg_boost: 0.35,
        blend_bg_fg: false,
        cache: None,
    };
    let mut rest = arguments.iter();
    while let Some(argument) = rest.next() {
        let (name, inline) = match argument.split_once('=') {
            Some((name, value)) => (name, Some(value.to_owned())),
            None => (argument.as_str(), None),
        };
        let mut value = || {
            inline
                .clone()
                .or_else(|| rest.next().cloned())
                .ok_or_else(|| format!("argument {name}: expected one argument"))
        };
        let number = |text: String| {
            text.parse::<f64>()
                .map_err(|_| format!("argument {name}: invalid float value: '{text}'"))
        };
        match name {
            "--path" => options.path = Some(value()?),
            "--size" => {
                let text = value()?;
                options.size = text
                    .parse()
                    .map_err(|_| format!("argument --size: invalid int value: '{text}'"))?;
            }
            "--color" => options.color = Some(value()?),
            "--mode" => {
                options.dark = match value()?.as_str() {
                    "dark" => true,
                    "light" => false,
                    other => return Err(format!("argument --mode: invalid choice: '{other}'")),
                }
            }
            "--scheme" => options.scheme = value()?,
            "--smart" => options.smart = true,
            "--transparency" => {
                options.transparent = match value()?.as_str() {
                    "transparent" => true,
                    "opaque" => false,
                    other => {
                        return Err(format!(
                            "argument --transparency: invalid choice: '{other}'"
                        ));
                    }
                }
            }
            "--termscheme" => {
                let path = value()?;
                let text =
                    std::fs::read_to_string(&path).map_err(|error| format!("{path}: {error}"))?;
                options.termscheme = Some(text);
            }
            "--harmony" => options.harmony = number(value()?)?,
            "--harmonize_threshold" => options.harmonize_threshold = number(value()?)?,
            "--term_fg_boost" => options.term_fg_boost = number(value()?)?,
            "--blend_bg_fg" => options.blend_bg_fg = true,
            "--cache" => options.cache = Some(value()?),
            other => return Err(format!("unrecognized arguments: {other}")),
        }
    }
    Ok(options)
}

pub fn generate(arguments: &[String], terminal_scheme: Option<&str>) -> Result<String, String> {
    let mut options = parse(arguments)?;
    let source = if let Some(path) = &options.path {
        let argb = image_source(Path::new(path), options.size)?;
        if let Some(cache) = &options.cache {
            std::fs::write(cache, hex(argb)).map_err(|error| format!("{cache}: {error}"))?;
        }
        if options.smart && Hct::new(argb).get_chroma() < 20.0 {
            options.scheme = "neutral".to_owned();
        }
        argb
    } else if let Some(color) = &options.color {
        parse_hex(color).ok_or_else(|| format!("invalid colour: {color}"))?
    } else {
        return Err("either --path or --color is required".to_owned());
    };

    let scheme = Scheme::new(&options.scheme, Hct::new(source), options.dark);
    let mut output = format!(
        "$darkmode: {};\n$transparent: {};\n",
        python_bool(options.dark),
        python_bool(options.transparent)
    );
    let mut material = Vec::new();
    for (name, role) in ROLES {
        let value = hex(scheme.argb(role));
        output.push_str(&format!("${name}: {value};\n"));
        material.push((name, value));
    }
    for (name, value) in if options.dark {
        SUCCESS_DARK
    } else {
        SUCCESS_LIGHT
    } {
        output.push_str(&format!("${name}: {value};\n"));
    }

    if let Some(text) = options.termscheme.as_deref().or(terminal_scheme) {
        let json: Value =
            serde_json::from_str(text).map_err(|error| format!("terminal scheme: {error}"))?;
        let mode = if options.dark { "dark" } else { "light" };
        let colours = json
            .get(mode)
            .and_then(Value::as_object)
            .ok_or_else(|| format!("terminal scheme: no {mode} scheme"))?;
        let mut names: Vec<&String> = colours.keys().collect();
        names.sort_by_key(|name| (terminal_index(name), (*name).clone()));
        let lookup = |wanted: &str| {
            material
                .iter()
                .find(|(name, _)| *name == wanted)
                .and_then(|(_, value)| parse_hex(value))
                .unwrap_or_default()
        };
        let primary = lookup("primary_paletteKeyColor");
        for name in names {
            let Some(value) = colours[name].as_str() else {
                continue;
            };
            if options.scheme == "monochrome" {
                output.push_str(&format!("${name}: {value};\n"));
                continue;
            }
            let source = parse_hex(value).unwrap_or_default();
            let harmonized = if options.blend_bg_fg && name == "term0" {
                boost_chroma_tone(lookup("surfaceContainerLow"), 1.2, 0.95)
            } else if options.blend_bg_fg && name == "term15" {
                boost_chroma_tone(lookup("onSurface"), 3.0, 1.0)
            } else {
                let shifted = harmonize(
                    source,
                    primary,
                    options.harmonize_threshold,
                    options.harmony,
                );
                let direction = if options.dark { 1.0 } else { -1.0 };
                boost_chroma_tone(shifted, 1.0, 1.0 + options.term_fg_boost * direction)
            };
            output.push_str(&format!("${name}: {};\n", hex(harmonized)));
        }
    }
    Ok(output)
}

fn harmonize(design: Argb, source: Argb, threshold: f64, harmony: f64) -> Argb {
    let from = Hct::new(design);
    let to = Hct::new(source);
    let rotation = (difference_degrees(from.get_hue(), to.get_hue()) * harmony).min(threshold);
    let hue = sanitize_degrees_double(
        from.get_hue() + rotation * rotate_direction(from.get_hue(), to.get_hue()),
    );
    Hct::from(hue, from.get_chroma(), from.get_tone()).into()
}

fn boost_chroma_tone(argb: Argb, chroma: f64, tone: f64) -> Argb {
    let hct = Hct::new(argb);
    Hct::from(
        hct.get_hue(),
        hct.get_chroma() * chroma,
        hct.get_tone() * tone,
    )
    .into()
}

fn terminal_index(name: &str) -> u32 {
    name.strip_prefix("term")
        .and_then(|number| number.parse().ok())
        .unwrap_or(u32::MAX)
}

fn python_bool(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn hex(argb: Argb) -> String {
    format!("#{:02X}{:02X}{:02X}", argb.red, argb.green, argb.blue)
}

fn parse_hex(text: &str) -> Option<Argb> {
    let digits = text.strip_prefix('#').unwrap_or(text);
    if digits.len() < 6 {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(digits.get(range)?, 16).ok();
    Some(Argb::new(
        255,
        channel(0..2)?,
        channel(2..4)?,
        channel(4..6)?,
    ))
}

struct Image {
    width: usize,
    height: usize,
    channels: usize,
    pixels: Vec<u8>,
}

fn load(path: &Path) -> Result<Image, String> {
    let mut head = [0u8; 26];
    if let Ok(mut file) = std::fs::File::open(path) {
        use std::io::Read;
        let _ = file.read(&mut head);
    }
    if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return load_jpeg(path);
    }
    if head.starts_with(b"\x89PNG\r\n\x1a\n") && head[24] == 16 {
        return load_deep_png(path, matches!(head[25], 4 | 6));
    }
    let describe = |error: glib::Error| format!("{}: {error}", path.display());
    let pixbuf = if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gif"))
    {
        let animation = PixbufAnimation::from_file(path).map_err(describe)?;
        let start = std::time::SystemTime::UNIX_EPOCH;
        let frames = animation.iter(Some(start));
        let first = frames.pixbuf();
        let delay = frames.delay_time();
        match delay {
            Some(delay) if frames.advance(start + delay) => frames.pixbuf(),
            _ => first,
        }
    } else {
        Pixbuf::from_file(path).map_err(describe)?
    };
    let (width, height) = (pixbuf.width() as usize, pixbuf.height() as usize);
    let channels = pixbuf.n_channels() as usize;
    let stride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let mut pixels = Vec::with_capacity(width * height * channels);
    for row in 0..height {
        pixels.extend_from_slice(&bytes[row * stride..row * stride + width * channels]);
    }
    Ok(Image {
        width,
        height,
        channels,
        pixels,
    })
}

fn decode_with(path: &Path, command: &mut std::process::Command) -> Result<Vec<u8>, String> {
    let output = command
        .output()
        .map_err(|error| format!("{}: {error}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{}: {}",
            path.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

fn load_jpeg(path: &Path) -> Result<Image, String> {
    let ppm = decode_with(
        path,
        std::process::Command::new("djpeg").arg("-ppm").arg(path),
    )?;
    let mut fields = Vec::new();
    let mut at = 0;
    while fields.len() < 4 && at < ppm.len() {
        while at < ppm.len() && ppm[at].is_ascii_whitespace() {
            at += 1;
        }
        let start = at;
        while at < ppm.len() && !ppm[at].is_ascii_whitespace() {
            at += 1;
        }
        fields.push(String::from_utf8_lossy(&ppm[start..at]).into_owned());
    }
    let number = |index: usize| {
        fields
            .get(index)
            .and_then(|field| field.parse::<usize>().ok())
    };
    let (Some(width), Some(height)) = (number(1), number(2)) else {
        return Err(format!("{}: unreadable JPEG", path.display()));
    };
    let (channels, start) = match fields.first().map(String::as_str) {
        Some("P6") => (3, at + 1),
        Some("P5") => (1, at + 1),
        _ => return Err(format!("{}: unreadable JPEG", path.display())),
    };
    let data = &ppm[start..];
    let pixels: Vec<u8> = if channels == 1 {
        data.iter()
            .take(width * height)
            .flat_map(|&value| [value; 3])
            .collect()
    } else {
        data[..width * height * 3].to_vec()
    };
    Ok(Image {
        width,
        height,
        channels: 3,
        pixels,
    })
}

fn load_deep_png(path: &Path, alpha: bool) -> Result<Image, String> {
    let describe = |error: glib::Error| format!("{}: {error}", path.display());
    let pixbuf = Pixbuf::from_file(path).map_err(describe)?;
    let (width, height) = (pixbuf.width() as usize, pixbuf.height() as usize);
    let channels = if alpha { 4 } else { 3 };
    let format = if alpha { "rgba:-" } else { "rgb:-" };
    let raw = decode_with(
        path,
        std::process::Command::new("magick")
            .arg(path)
            .args(["-depth", "16", "-endian", "MSB", format]),
    )?;
    let pixels: Vec<u8> = raw.chunks_exact(2).map(|sample| sample[0]).collect();
    if pixels.len() != width * height * channels {
        return Err(format!("{}: unreadable PNG", path.display()));
    }
    Ok(Image {
        width,
        height,
        channels,
        pixels,
    })
}

fn image_source(path: &Path, size: u32) -> Result<Argb, String> {
    let image = load(path)?;
    let area = (image.width * image.height) as f64;
    let bitmap = (size as f64).powi(2);
    let scale = if area > bitmap {
        (bitmap / area).sqrt()
    } else {
        1.0
    };
    let width = ((image.width as f64 * scale).round() as usize).max(1);
    let height = ((image.height as f64 * scale).round() as usize).max(1);
    let image = if width < image.width || height < image.height {
        resize(&image, width, height, Filter::Bicubic)
    } else {
        image
    };
    let pixels: Vec<u32> = image
        .pixels
        .chunks_exact(image.channels)
        .map(|pixel| ((pixel[0] as u32) << 16) | ((pixel[1] as u32) << 8) | pixel[2] as u32)
        .collect();
    let colours = quantize::celebi(&pixels, QUANTIZE_COLORS);
    quantize::score(&colours)
        .first()
        .map(|&argb| Argb::from_u32(argb))
        .ok_or_else(|| "no colours in the image".to_owned())
}

pub fn scheme_for_image(arguments: &[String]) -> Result<String, String> {
    let colorfulness_only = arguments
        .iter()
        .any(|argument| argument == "--colorfulness");
    let Some(path) = arguments
        .iter()
        .find(|argument| *argument != "--colorfulness")
    else {
        return Err("scheme-tonal-spot".to_owned());
    };
    let Ok(image) = load(Path::new(path)) else {
        return Err("scheme-tonal-spot".to_owned());
    };
    let image = rgb(image);
    let longest = image.width.max(image.height);
    let image = if longest > SCHEME_IMAGE_SIZE as usize {
        let scale = SCHEME_IMAGE_SIZE as f64 / longest as f64;
        let width = ((image.width as f64 * scale) as usize).max(1);
        let height = ((image.height as f64 * scale) as usize).max(1);
        resize(&image, width, height, Filter::Lanczos)
    } else {
        image
    };
    let colorfulness = colorfulness(&image);
    if colorfulness_only {
        return Ok(format!("{}\n", python_float(colorfulness)));
    }
    let scheme = if colorfulness < COLORFUL {
        "scheme-neutral"
    } else {
        "scheme-tonal-spot"
    };
    Ok(format!("{scheme}\n"))
}

fn rgb(image: Image) -> Image {
    if image.channels == 3 {
        return image;
    }
    let pixels = image
        .pixels
        .chunks_exact(image.channels)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    Image {
        channels: 3,
        pixels,
        ..image
    }
}

fn colorfulness(image: &Image) -> f64 {
    let count = (image.width * image.height) as f64;
    let (mut rg_sum, mut rg_squares, mut yb_sum, mut yb_squares) = (0.0, 0.0, 0.0, 0.0);
    for pixel in image.pixels.chunks_exact(image.channels) {
        let (red, green, blue) = (pixel[0] as i32, pixel[1] as i32, pixel[2] as i32);
        let rg = (red - green).abs() as f64;
        let yb = ((red + green) / 2 - blue).abs() as f64;
        rg_sum += rg;
        rg_squares += rg * rg;
        yb_sum += yb;
        yb_squares += yb * yb;
    }
    let deviation = |sum: f64, squares: f64| {
        let mean = sum / count;
        (squares / count - mean * mean).max(0.0).sqrt()
    };
    let (rg_mean, yb_mean) = (rg_sum / count, yb_sum / count);
    let (rg_deviation, yb_deviation) =
        (deviation(rg_sum, rg_squares), deviation(yb_sum, yb_squares));
    (rg_deviation.powi(2) + yb_deviation.powi(2)).sqrt()
        + 0.3 * (rg_mean.powi(2) + yb_mean.powi(2)).sqrt()
}

fn python_float(value: f64) -> String {
    let text = format!("{value}");
    if text.contains('.') || text.contains('e') {
        text
    } else {
        format!("{text}.0")
    }
}

#[derive(Clone, Copy)]
enum Filter {
    Bicubic,
    Lanczos,
}

impl Filter {
    fn support(self) -> f64 {
        match self {
            Filter::Bicubic => 2.0,
            Filter::Lanczos => 3.0,
        }
    }

    fn weight(self, x: f64) -> f64 {
        match self {
            Filter::Bicubic => {
                let a = -0.5;
                let x = x.abs();
                if x < 1.0 {
                    ((a + 2.0) * x - (a + 3.0)) * x * x + 1.0
                } else if x < 2.0 {
                    (((x - 5.0) * x + 8.0) * x - 4.0) * a
                } else {
                    0.0
                }
            }
            Filter::Lanczos => {
                if (-3.0..3.0).contains(&x) {
                    sinc(x) * sinc(x / 3.0)
                } else {
                    0.0
                }
            }
        }
    }
}

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let x = x * std::f64::consts::PI;
    x.sin() / x
}

struct Coefficients {
    size: usize,
    bounds: Vec<(usize, usize)>,
    weights: Vec<i32>,
}

fn coefficients(input: usize, output: usize, filter: Filter) -> Coefficients {
    let scale = input as f64 / output as f64;
    let filter_scale = scale.max(1.0);
    let support = filter.support() * filter_scale;
    let size = support.ceil() as usize * 2 + 1;
    let mut bounds = Vec::with_capacity(output);
    let mut weights = vec![0; output * size];
    for out in 0..output {
        let centre = (out as f64 + 0.5) * scale;
        let inverse = 1.0 / filter_scale;
        let min = ((centre - support + 0.5) as i64).max(0) as usize;
        let max = ((centre + support + 0.5) as i64).min(input as i64) as usize - min;
        let mut row = vec![0.0; max];
        let mut total = 0.0;
        for (index, slot) in row.iter_mut().enumerate() {
            let weight = filter.weight((index as f64 + min as f64 - centre + 0.5) * inverse);
            *slot = weight;
            total += weight;
        }
        for (index, weight) in row.into_iter().enumerate() {
            let weight = if total != 0.0 { weight / total } else { weight };
            let fixed = weight * (1u64 << PRECISION_BITS) as f64;
            weights[out * size + index] = if weight < 0.0 {
                (-0.5 + fixed) as i32
            } else {
                (0.5 + fixed) as i32
            };
        }
        bounds.push((min, max));
    }
    Coefficients {
        size,
        bounds,
        weights,
    }
}

fn clip(value: i32) -> u8 {
    (value >> PRECISION_BITS).clamp(0, 255) as u8
}

fn resize(image: &Image, width: usize, height: usize, filter: Filter) -> Image {
    let alpha = image.channels == 4;
    let source = if alpha {
        premultiply(image)
    } else {
        image.pixels.clone()
    };
    let channels = image.channels;
    let horizontal = coefficients(image.width, width, filter);
    let vertical = coefficients(image.height, height, filter);
    let first = vertical.bounds[0].0;
    let last = vertical.bounds[height - 1].0 + vertical.bounds[height - 1].1;
    let (row_offset, pixels) = if width != image.width {
        let rows = last - first;
        let mut out = vec![0u8; width * rows * channels];
        for row in 0..rows {
            let line = &source[(row + first) * image.width * channels..];
            for column in 0..width {
                let (min, count) = horizontal.bounds[column];
                let weights = &horizontal.weights[column * horizontal.size..];
                for channel in 0..channels {
                    let mut sum = 1i32 << (PRECISION_BITS - 1);
                    for index in 0..count {
                        sum += line[(min + index) * channels + channel] as i32 * weights[index];
                    }
                    out[(row * width + column) * channels + channel] = clip(sum);
                }
            }
        }
        (first, out)
    } else {
        (0, source)
    };
    let pixels = if height != image.height {
        let mut out = vec![0u8; width * height * channels];
        for row in 0..height {
            let (min, count) = vertical.bounds[row];
            let min = min - row_offset;
            let weights = &vertical.weights[row * vertical.size..];
            for column in 0..width {
                for channel in 0..channels {
                    let mut sum = 1i32 << (PRECISION_BITS - 1);
                    for index in 0..count {
                        sum += pixels[((min + index) * width + column) * channels + channel] as i32
                            * weights[index];
                    }
                    out[(row * width + column) * channels + channel] = clip(sum);
                }
            }
        }
        out
    } else {
        pixels
    };
    let mut resized = Image {
        width,
        height,
        channels,
        pixels,
    };
    if alpha {
        unpremultiply(&mut resized);
    }
    resized
}

fn premultiply(image: &Image) -> Vec<u8> {
    let mut pixels = image.pixels.clone();
    for pixel in pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        for channel in &mut pixel[..3] {
            let value = *channel as u32 * alpha + 128;
            *channel = (((value >> 8) + value) >> 8) as u8;
        }
    }
    pixels
}

fn unpremultiply(image: &mut Image) {
    for pixel in image.pixels.chunks_exact_mut(4) {
        let alpha = pixel[3] as u32;
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((255 * *channel as u32) / alpha).min(255) as u8;
        }
    }
}

pub fn kde_selection() -> Result<String, String> {
    let colours = crate::core::paths::generated().join("colors.json");
    let kdeglobals = glib::user_config_dir().join("kdeglobals");
    let schemes = glib::user_data_dir().join("color-schemes");
    let json: Value = std::fs::read_to_string(&colours)
        .map_err(|error| error.to_string())
        .and_then(|text| serde_json::from_str(&text).map_err(|error| error.to_string()))
        .map_err(|error| format!("no generated colors to read: {error}"))?;
    let (Some(background), Some(foreground)) = (
        json.get("primary_container").and_then(Value::as_str),
        json.get("on_primary_container").and_then(Value::as_str),
    ) else {
        return Err("generated colors carry no primary container".to_owned());
    };
    let changed = patch_selection(&kdeglobals, background, foreground);
    let scheme = std::fs::read_to_string(&kdeglobals).ok().and_then(|text| {
        text.lines()
            .find_map(|line| line.strip_prefix("ColorScheme="))
            .map(|name| schemes.join(format!("{}.colors", name.trim())))
    });
    if let Some(scheme) = scheme {
        patch_selection(&scheme, background, foreground);
    }
    if changed {
        let _ = std::process::Command::new("dbus-send")
            .args([
                "--session",
                "--type=signal",
                "/KGlobalSettings",
                "org.kde.KGlobalSettings.notifyChange",
                "int32:0",
                "int32:0",
            ])
            .output();
    }
    Ok(String::new())
}

fn patch_selection(path: &Path, background: &str, foreground: &str) -> bool {
    const SECTION: &str = "[Colors:Selection]";
    const BACKGROUND_KEYS: [&str; 4] = [
        "BackgroundNormal",
        "BackgroundAlternate",
        "DecorationFocus",
        "DecorationHover",
    ];
    const FOREGROUND_KEYS: [&str; 3] =
        ["ForegroundNormal", "ForegroundActive", "ForegroundInactive"];
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes);
    let mut inside = false;
    let mut changed = false;
    let mut body = Vec::new();
    for line in text.lines() {
        let mut line = line.to_owned();
        if line.starts_with('[') {
            inside = line.trim() == SECTION;
        } else if inside && let Some((key, _)) = line.split_once('=') {
            let key = key.trim().to_owned();
            let wanted = if BACKGROUND_KEYS.contains(&key.as_str()) {
                Some(background)
            } else if FOREGROUND_KEYS.contains(&key.as_str()) {
                Some(foreground)
            } else {
                None
            };
            if let Some(wanted) = wanted {
                let replaced = format!("{key}={wanted}");
                if line != replaced {
                    line = replaced;
                    changed = true;
                }
            }
        }
        body.push(line);
    }
    if changed {
        let _ = std::fs::write(path, body.join("\n") + "\n");
    }
    changed
}

fn with_newline(mut text: String) -> String {
    if !text.ends_with('\n') {
        text.push('\n');
    }
    text
}
