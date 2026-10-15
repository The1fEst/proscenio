use gtk4::cairo;
use gtk4::gdk;
use gtk4::gdk::RGBA;
use gtk4::gio;
use gtk4::graphene;
use gtk4::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;

const FALLBACK: &str = "application-x-executable";

thread_local! {
    static CACHE: RefCell<HashMap<Key, Option<cairo::ImageSurface>>> =
        RefCell::new(HashMap::new());
    static NAMES: RefCell<HashMap<String, String>> = RefCell::new(HashMap::new());
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct Key {
    class: String,
    size: i32,
    scale: i32,
    tint: [u8; 3],
    amount: u16,
}

pub fn surface(
    widget: &impl IsA<gtk4::Widget>,
    class: &str,
    size: i32,
    tint: RGBA,
    amount: f64,
) -> Option<cairo::ImageSurface> {
    let scale = widget.as_ref().scale_factor();
    let key = Key {
        class: class.to_owned(),
        size,
        scale,
        tint: [
            (tint.red() * 255.0) as u8,
            (tint.green() * 255.0) as u8,
            (tint.blue() * 255.0) as u8,
        ],
        amount: (amount * 1000.0) as u16,
    };
    if let Some(cached_surface) = CACHE.with(|cache| cache.borrow().get(&key).cloned()) {
        return cached_surface;
    }

    let icon_surface = render(widget, class, size, scale).map(|mut surface| {
        colorize(&mut surface, tint, amount);
        surface
    });
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|other_key, _| other_key.tint == key.tint && other_key.amount == key.amount);
        cache.insert(key, icon_surface.clone());
    });
    icon_surface
}

pub fn render_muted(
    widget: &impl IsA<gtk4::Widget>,
    paintable: &impl IsA<gdk::Paintable>,
    size: i32,
    tint: RGBA,
) -> Option<gdk::Texture> {
    let device_size = size * widget.as_ref().scale_factor();
    let mut surface = rasterize(widget, paintable, device_size)?;
    desaturate(&mut surface, tint);
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?.to_vec();
    Some(
        gdk::MemoryTexture::new(
            device_size,
            device_size,
            gdk::MemoryFormat::B8g8r8a8Premultiplied,
            &gtk4::glib::Bytes::from_owned(data),
            stride,
        )
        .upcast(),
    )
}

fn render(
    widget: &impl IsA<gtk4::Widget>,
    class: &str,
    size: i32,
    scale: i32,
) -> Option<cairo::ImageSurface> {
    let display = gdk::Display::default()?;
    let theme = gtk4::IconTheme::for_display(&display);
    let paintable = load_icon(&theme, &guess(&theme, class), FALLBACK, size, scale);
    let surface = rasterize(widget, &paintable, size * scale)?;
    surface.set_device_scale(scale as f64, scale as f64);
    Some(surface)
}

pub fn load_icon(
    theme: &gtk4::IconTheme,
    name: &str,
    fallback: &str,
    size: i32,
    scale: i32,
) -> gdk::Paintable {
    let icon = if name.starts_with('/') && std::path::Path::new(name).is_file() {
        gtk4::IconPaintable::for_file(&gio::File::for_path(name), size, scale)
    } else {
        let icon_name = if theme.has_icon(name) { name } else { fallback };
        theme.lookup_icon(
            icon_name,
            &[],
            size,
            scale,
            gtk4::TextDirection::None,
            gtk4::IconLookupFlags::PRELOAD,
        )
    };
    recolor(&icon, size * scale).unwrap_or_else(|| icon.upcast())
}

fn recolor(icon: &gtk4::IconPaintable, size: i32) -> Option<gdk::Paintable> {
    let path = icon.file()?.path()?;
    if path.extension()? != "svg" {
        return None;
    }
    let source = std::fs::read_to_string(&path).ok()?;
    let marker = source.find("id=\"current-color-scheme\"")?;
    let open = marker + source[marker..].find('>')? + 1;
    let close = open + source[open..].find("</style>")?;
    let svg = format!("{}{}{}", &source[..open], color_scheme(), &source[close..]);
    let stream =
        gtk4::gio::MemoryInputStream::from_bytes(&gtk4::glib::Bytes::from_owned(svg.into_bytes()));
    let pixbuf = gtk4::gdk_pixbuf::Pixbuf::from_stream_at_scale(
        &stream,
        size,
        size,
        true,
        gio::Cancellable::NONE,
    )
    .ok()?;
    #[allow(deprecated)]
    Some(gdk::Texture::for_pixbuf(&pixbuf).upcast())
}

fn color_scheme() -> String {
    let pick = |group: &str, key: &str, default: &str| {
        crate::ui::widgets::display::text::kdeglobals(group, key)
            .map(|value| kde_color(&value))
            .unwrap_or_else(|| default.to_owned())
    };
    let text = pick("Colors:Window", "ForegroundNormal", "#232629");
    let background = pick("Colors:Window", "BackgroundNormal", "#eff0f1");
    let highlight = pick("Colors:Selection", "BackgroundNormal", "#3daee9");
    let highlighted_text = pick("Colors:Selection", "ForegroundNormal", "#fcfcfc");
    let positive = pick("Colors:Window", "ForegroundPositive", "#27ae60");
    let neutral = pick("Colors:Window", "ForegroundNeutral", "#f67400");
    let negative = pick("Colors:Window", "ForegroundNegative", "#da4453");
    let active = pick("Colors:Window", "ForegroundActive", "#3daee9");
    format!(
        ".ColorScheme-Text {{ color:{text}; }}\
         .ColorScheme-Background {{ color:{background}; }}\
         .ColorScheme-Highlight {{ color:{highlight}; }}\
         .ColorScheme-HighlightedText {{ color:{highlighted_text}; }}\
         .ColorScheme-PositiveText {{ color:{positive}; }}\
         .ColorScheme-NeutralText {{ color:{neutral}; }}\
         .ColorScheme-NegativeText {{ color:{negative}; }}\
         .ColorScheme-ActiveText {{ color:{active}; }}\
         .ColorScheme-Complement {{ color:{background}; }}\
         .ColorScheme-Contrast {{ color:{background}; }}\
         .ColorScheme-Accent {{ color:{highlight}; }}"
    )
}

fn kde_color(value: &str) -> String {
    if value.starts_with('#') {
        return value.to_owned();
    }
    let parts: Vec<u8> = value
        .split(',')
        .filter_map(|part| part.trim().parse().ok())
        .collect();
    match parts.as_slice() {
        [red, green, blue, ..] => format!("#{red:02x}{green:02x}{blue:02x}"),
        _ => value.to_owned(),
    }
}

fn rasterize(
    widget: &impl IsA<gtk4::Widget>,
    paintable: &impl IsA<gdk::Paintable>,
    size: i32,
) -> Option<cairo::ImageSurface> {
    let widget = widget.as_ref();
    let snapshot = gtk4::Snapshot::new();
    paintable.snapshot(&snapshot, size as f64, size as f64);
    let node = snapshot.to_node()?;
    let renderer = widget.native()?.renderer()?;
    let texture = renderer.render_texture(
        &node,
        Some(&graphene::Rect::new(0.0, 0.0, size as f32, size as f32)),
    );

    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, size, size).ok()?;
    let stride = surface.stride() as usize;
    {
        let mut data = surface.data().ok()?;
        texture.download(&mut data, stride);
    }
    surface.mark_dirty();
    Some(surface)
}

fn desaturate(surface: &mut cairo::ImageSurface, tint: RGBA) {
    const GRAY: f64 = 0.8;
    const WASH: f64 = 0.1;

    let stride = surface.stride() as usize;
    let height = surface.height() as usize;
    let width = surface.width() as usize;
    let Ok(mut data) = surface.data() else {
        return;
    };
    for row in 0..height {
        for column in 0..width {
            let at = row * stride + column * 4;
            let alpha = data[at + 3] as f64 / 255.0;
            if alpha <= 0.0 {
                continue;
            }
            let straight_channels = [
                data[at] as f64 / 255.0 / alpha,
                data[at + 1] as f64 / 255.0 / alpha,
                data[at + 2] as f64 / 255.0 / alpha,
            ];
            let gray = 0.114 * straight_channels[0]
                + 0.587 * straight_channels[1]
                + 0.299 * straight_channels[2];
            let wash = [tint.blue() as f64, tint.green() as f64, tint.red() as f64];
            for (offset, channel) in straight_channels.iter().enumerate() {
                let muted_channel = channel + (gray - channel) * GRAY;
                let value = muted_channel + (wash[offset] - muted_channel) * WASH;
                data[at + offset] = (value.clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
            }
        }
    }
}

fn colorize(surface: &mut cairo::ImageSurface, tint: RGBA, amount: f64) {
    if amount <= 0.0 {
        return;
    }
    let (hue, saturation) = {
        let (hue, saturation, _) =
            to_hsl(tint.red() as f64, tint.green() as f64, tint.blue() as f64);
        (hue, saturation)
    };

    let stride = surface.stride() as usize;
    let height = surface.height() as usize;
    let width = surface.width() as usize;
    let Ok(mut data) = surface.data() else {
        return;
    };
    for row in 0..height {
        for column in 0..width {
            let at = row * stride + column * 4;
            let alpha = data[at + 3] as f64 / 255.0;
            if alpha <= 0.0 {
                continue;
            }
            let blue = data[at] as f64 / 255.0 / alpha;
            let green = data[at + 1] as f64 / 255.0 / alpha;
            let red = data[at + 2] as f64 / 255.0 / alpha;

            let (_, _, lightness) = to_hsl(red, green, blue);
            let (tinted_red, tinted_green, tinted_blue) = to_rgb(hue, saturation, lightness);

            let mix = |original: f64, target: f64| original + (target - original) * amount;
            let mixed_channels = [
                mix(blue, tinted_blue),
                mix(green, tinted_green),
                mix(red, tinted_red),
            ];
            for (offset, channel) in mixed_channels.iter().enumerate() {
                data[at + offset] = (channel.clamp(0.0, 1.0) * alpha * 255.0).round() as u8;
            }
        }
    }
}

fn to_hsl(red: f64, green: f64, blue: f64) -> (f64, f64, f64) {
    let high = red.max(green).max(blue);
    let low = red.min(green).min(blue);
    let lightness = (high + low) / 2.0;
    if (high - low).abs() < f64::EPSILON {
        return (0.0, 0.0, lightness);
    }
    let span = high - low;
    let saturation = if lightness > 0.5 {
        span / (2.0 - high - low)
    } else {
        span / (high + low)
    };
    let hue = if high == red {
        (green - blue) / span + if green < blue { 6.0 } else { 0.0 }
    } else if high == green {
        (blue - red) / span + 2.0
    } else {
        (red - green) / span + 4.0
    };
    (hue / 6.0, saturation, lightness)
}

fn to_rgb(hue: f64, saturation: f64, lightness: f64) -> (f64, f64, f64) {
    if saturation <= 0.0 {
        return (lightness, lightness, lightness);
    }
    let q = if lightness < 0.5 {
        lightness * (1.0 + saturation)
    } else {
        lightness + saturation - lightness * saturation
    };
    let p = 2.0 * lightness - q;
    (
        channel(p, q, hue + 1.0 / 3.0),
        channel(p, q, hue),
        channel(p, q, hue - 1.0 / 3.0),
    )
}

fn channel(p: f64, q: f64, mut part: f64) -> f64 {
    if part < 0.0 {
        part += 1.0;
    }
    if part > 1.0 {
        part -= 1.0;
    }
    if part < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * part;
    }
    if part < 0.5 {
        return q;
    }
    if part < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - part) * 6.0;
    }
    p
}

pub fn guess(theme: &gtk4::IconTheme, class: &str) -> String {
    if class.is_empty() {
        return FALLBACK.to_owned();
    }
    if let Some(name) = NAMES.with(|names| names.borrow().get(class).cloned()) {
        return name;
    }
    let name = search(theme, class);
    NAMES.with(|names| names.borrow_mut().insert(class.to_owned(), name.clone()));
    name
}

fn search(theme: &gtk4::IconTheme, class: &str) -> String {
    if let Some(icon) = lookup_entry_icon(class) {
        return icon;
    }
    if let Some(icon) = substitute(class) {
        return icon;
    }

    let last_segment = class.rsplit('.').next().unwrap_or(class).to_owned();
    let candidates = [
        class.to_owned(),
        class.to_lowercase(),
        last_segment.clone(),
        last_segment.to_lowercase(),
        class.to_lowercase().replace(char::is_whitespace, "-"),
        class.to_lowercase().replace('_', "-"),
    ];
    for candidate in candidates {
        if theme.has_icon(&candidate) {
            return candidate;
        }
    }

    FALLBACK.to_owned()
}

fn lookup_entry_icon(class: &str) -> Option<String> {
    let desktop_id = format!("{}.desktop", class.to_lowercase());
    for info in gio::AppInfo::all() {
        let id = info.id().map(|id| id.to_lowercase()).unwrap_or_default();
        if id != desktop_id && !info.name().eq_ignore_ascii_case(class) {
            continue;
        }
        if let Some(name) = resolve_icon(&info) {
            return Some(name);
        }
    }
    None
}

fn resolve_icon(info: &gio::AppInfo) -> Option<String> {
    let icon = info.icon()?;
    if let Some(themed_icon) = icon.downcast_ref::<gio::ThemedIcon>() {
        return themed_icon.names().first().map(|name| name.to_string());
    }
    if let Some(file_icon) = icon.downcast_ref::<gio::FileIcon>() {
        return file_icon
            .file()
            .path()
            .map(|path| path.to_string_lossy().into_owned());
    }
    None
}

fn substitute(class: &str) -> Option<String> {
    let alias = match class {
        "code-url-handler" | "Code" => Some("visual-studio-code"),
        "gnome-tweaks" => Some("org.gnome.tweaks"),
        "pavucontrol-qt" => Some("pavucontrol"),
        "wps" | "wpsoffice" => Some("wps-office2019-kprometheus"),
        "footclient" => Some("foot"),
        _ => None,
    };
    if let Some(alias) = alias {
        return Some(alias.to_owned());
    }
    if let Some(app_id) = class.strip_prefix("steam_app_")
        && app_id.chars().all(|digit| digit.is_ascii_digit())
    {
        return Some(format!("steam_icon_{app_id}"));
    }
    if class.starts_with("Minecraft") {
        return Some("minecraft".to_owned());
    }
    if class.contains("polkit") || class.contains("gcr.prompter") {
        return Some("system-lock-screen".to_owned());
    }
    None
}
