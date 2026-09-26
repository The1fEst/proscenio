use gtk4::gdk::RGBA;
use gtk4::gdk_pixbuf::Pixbuf;
use gtk4::gio;
use gtk4::prelude::*;
use serde_json::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use crate::core::config::Config;
use crate::core::watch;

pub type SharedTheme = Rc<RefCell<Theme>>;

pub mod rounding {
    pub const UNSHARPEN: i32 = 2;
    pub const UNSHARPENMORE: i32 = 6;
    pub const VERYSMALL: i32 = 8;
    pub const SMALL: i32 = 12;
    pub const NORMAL: i32 = 17;
    pub const LARGE: i32 = 23;
    pub const FULL: i32 = 9999;
    pub const SCREEN_ROUNDING: i32 = LARGE;
    pub const WINDOW_ROUNDING: i32 = 18;
}

pub mod pixel_size {
    pub const SMALLEST: i32 = 10;
    pub const SMALLER: i32 = 12;
    pub const SMALLIE: i32 = 13;
    pub const SMALL: i32 = 15;
    pub const NORMAL: i32 = 16;
    pub const LARGE: i32 = 17;
    pub const LARGER: i32 = 19;
    pub const HUGE: i32 = 22;
    pub const HUGEASS: i32 = 23;
    pub const TITLE: i32 = HUGE;
}

macro_rules! palette {
    ($($key:ident = $default:literal,)*) => {
        #[derive(Clone)]
        pub struct M3 {
            pub darkmode: bool,
            $(pub $key: RGBA,)*
        }

        impl M3 {
            fn read(json: &Value) -> Self {
                let mut m3 = M3 {
                    darkmode: true,
                    $($key: json
                        .get(stringify!($key))
                        .and_then(Value::as_str)
                        .and_then(|text| RGBA::parse(text).ok())
                        .unwrap_or_else(|| rgba($default)),)*
                };
                m3.darkmode = hsl_lightness(m3.background) < 0.5;
                m3
            }

            fn variables(&self) -> String {
                let mut out = String::new();
                $(out.push_str(&format!("  --{}: {};\n", m3_name(stringify!($key)), css_rgba(self.$key)));)*
                out
            }

            fn foregrounds() -> String {
                let mut out = String::new();
                $(out.push_str(&format!(".fg-{0} {{ color: var(--{0}); }}\n", m3_name(stringify!($key))));)*
                out
            }
        }
    };
}

palette! {
    background = "#141313",
    on_background = "#e6e1e1",
    surface = "#141313",
    surface_dim = "#141313",
    surface_bright = "#3a3939",
    surface_container_lowest = "#0f0e0e",
    surface_container_low = "#1c1b1c",
    surface_container = "#201f20",
    surface_container_high = "#2b2a2a",
    surface_container_highest = "#363435",
    on_surface = "#e6e1e1",
    surface_variant = "#49464a",
    on_surface_variant = "#cbc5ca",
    inverse_surface = "#e6e1e1",
    inverse_on_surface = "#313030",
    outline = "#948f94",
    outline_variant = "#49464a",
    shadow = "#000000",
    scrim = "#000000",
    surface_tint = "#cbc4cb",
    primary = "#cbc4cb",
    on_primary = "#322f34",
    primary_container = "#2d2a2f",
    on_primary_container = "#bcb6bc",
    inverse_primary = "#615d63",
    secondary = "#cac5c8",
    on_secondary = "#323032",
    secondary_container = "#4d4b4d",
    on_secondary_container = "#ece6e9",
    tertiary = "#d1c3c6",
    on_tertiary = "#372e30",
    tertiary_container = "#31292b",
    on_tertiary_container = "#c1b4b7",
    error = "#ffb4ab",
    on_error = "#690005",
    error_container = "#93000a",
    on_error_container = "#ffdad6",
    primary_fixed = "#e7e0e7",
    primary_fixed_dim = "#cbc4cb",
    on_primary_fixed = "#1d1b1f",
    on_primary_fixed_variant = "#49454b",
    secondary_fixed = "#e6e1e4",
    secondary_fixed_dim = "#cac5c8",
    on_secondary_fixed = "#1d1b1d",
    on_secondary_fixed_variant = "#484648",
    tertiary_fixed = "#eddfe1",
    tertiary_fixed_dim = "#d1c3c6",
    on_tertiary_fixed = "#211a1c",
    on_tertiary_fixed_variant = "#4e4447",
    success = "#B5CCBA",
    on_success = "#213528",
    success_container = "#374B3E",
    on_success_container = "#D1E9D6",
}

macro_rules! colors {
    ($($field:ident: $css:literal,)*) => {
        #[derive(Clone)]
        pub struct Colors {
            $(pub $field: RGBA,)*
        }

        impl Colors {
            fn variables(&self) -> String {
                let mut out = String::new();
                $(out.push_str(&format!("  --{}: {};\n", $css, css_rgba(self.$field)));)*
                out
            }

            fn foregrounds() -> String {
                let mut out = String::new();
                $(out.push_str(&format!(".fg-{0} {{ color: var(--{0}); }}\n", $css));)*
                out
            }
        }
    };
}

colors! {
    col_subtext: "colSubtext",
    col_layer0_base: "colLayer0Base",
    col_layer0: "colLayer0",
    col_on_layer0: "colOnLayer0",
    col_layer0_hover: "colLayer0Hover",
    col_layer0_active: "colLayer0Active",
    col_layer0_border: "colLayer0Border",
    col_layer1_base: "colLayer1Base",
    col_layer1: "colLayer1",
    col_on_layer1: "colOnLayer1",
    col_on_layer1_inactive: "colOnLayer1Inactive",
    col_layer1_hover: "colLayer1Hover",
    col_layer1_active: "colLayer1Active",
    col_layer2_base: "colLayer2Base",
    col_layer2: "colLayer2",
    col_layer2_hover: "colLayer2Hover",
    col_layer2_active: "colLayer2Active",
    col_layer2_disabled: "colLayer2Disabled",
    col_on_layer2: "colOnLayer2",
    col_on_layer2_disabled: "colOnLayer2Disabled",
    col_layer3_base: "colLayer3Base",
    col_layer3: "colLayer3",
    col_layer3_hover: "colLayer3Hover",
    col_layer3_active: "colLayer3Active",
    col_on_layer3: "colOnLayer3",
    col_layer4_base: "colLayer4Base",
    col_layer4: "colLayer4",
    col_layer4_hover: "colLayer4Hover",
    col_layer4_active: "colLayer4Active",
    col_on_layer4: "colOnLayer4",
    col_primary: "colPrimary",
    col_on_primary: "colOnPrimary",
    col_primary_hover: "colPrimaryHover",
    col_primary_active: "colPrimaryActive",
    col_primary_container: "colPrimaryContainer",
    col_primary_container_hover: "colPrimaryContainerHover",
    col_primary_container_active: "colPrimaryContainerActive",
    col_on_primary_container: "colOnPrimaryContainer",
    col_secondary: "colSecondary",
    col_secondary_hover: "colSecondaryHover",
    col_secondary_active: "colSecondaryActive",
    col_on_secondary: "colOnSecondary",
    col_secondary_container: "colSecondaryContainer",
    col_secondary_container_hover: "colSecondaryContainerHover",
    col_secondary_container_active: "colSecondaryContainerActive",
    col_on_secondary_container: "colOnSecondaryContainer",
    col_tertiary: "colTertiary",
    col_tertiary_hover: "colTertiaryHover",
    col_tertiary_active: "colTertiaryActive",
    col_tertiary_container: "colTertiaryContainer",
    col_tertiary_container_hover: "colTertiaryContainerHover",
    col_tertiary_container_active: "colTertiaryContainerActive",
    col_on_tertiary: "colOnTertiary",
    col_on_tertiary_container: "colOnTertiaryContainer",
    col_background_surface_container: "colBackgroundSurfaceContainer",
    col_surface_container_low: "colSurfaceContainerLow",
    col_surface_container: "colSurfaceContainer",
    col_surface_container_high: "colSurfaceContainerHigh",
    col_surface_container_highest: "colSurfaceContainerHighest",
    col_surface_container_highest_hover: "colSurfaceContainerHighestHover",
    col_surface_container_highest_active: "colSurfaceContainerHighestActive",
    col_on_surface: "colOnSurface",
    col_on_surface_variant: "colOnSurfaceVariant",
    col_tooltip: "colTooltip",
    col_on_tooltip: "colOnTooltip",
    col_scrim: "colScrim",
    col_shadow: "colShadow",
    col_outline: "colOutline",
    col_outline_variant: "colOutlineVariant",
    col_error: "colError",
    col_error_hover: "colErrorHover",
    col_error_active: "colErrorActive",
    col_on_error: "colOnError",
    col_error_container: "colErrorContainer",
    col_error_container_hover: "colErrorContainerHover",
    col_error_container_active: "colErrorContainerActive",
    col_on_error_container: "colOnErrorContainer",
}

impl Colors {
    fn compute(m3: &M3, config: &Config, background_transparency: f32) -> Self {
        let content = if config.transparency_automatic {
            0.9
        } else {
            config.content_transparency
        };
        let col_layer0_base = mix(
            m3.background,
            m3.primary,
            if config.extra_background_tint {
                0.99
            } else {
                1.0
            },
        );
        let col_layer0 = transparentize(col_layer0_base, background_transparency);
        let col_on_layer0 = m3.on_background;
        let col_layer1_base = m3.surface_container_low;
        let col_layer1 = solve_overlay_color(col_layer0_base, col_layer1_base, 1.0 - content);
        let col_on_layer1 = m3.on_surface_variant;
        let col_layer1_hover = transparentize(mix(col_layer1, col_on_layer1, 0.92), content);
        let col_layer1_active = transparentize(mix(col_layer1, col_on_layer1, 0.85), content);
        let col_on_layer2 = m3.on_surface;
        let col_layer2_base = m3.surface_container;
        let col_on_layer3 = m3.on_surface;
        let col_layer3_base = m3.surface_container_high;
        let col_on_layer4 = m3.on_surface;
        let col_layer4_base = m3.surface_container_highest;
        let col_primary = m3.primary;
        let col_primary_container = m3.primary_container;
        let col_on_primary_container = m3.on_primary_container;
        Colors {
            col_subtext: m3.outline,
            col_layer0_base,
            col_layer0,
            col_on_layer0,
            col_layer0_hover: transparentize(mix(col_layer0, col_on_layer0, 0.9), 1.0),
            col_layer0_active: transparentize(mix(col_layer0, col_on_layer0, 0.8), 1.0),
            col_layer0_border: mix(m3.outline_variant, col_layer0, 0.4),
            col_layer1_base,
            col_layer1,
            col_on_layer1,
            col_on_layer1_inactive: mix(col_on_layer1, col_layer1, 0.45),
            col_layer1_hover,
            col_layer1_active,
            col_layer2_base,
            col_layer2: solve_overlay_color(col_layer1_base, col_layer2_base, 1.0 - content),
            col_layer2_hover: solve_overlay_color(
                col_layer1_base,
                mix(col_layer2_base, col_on_layer2, 0.90),
                1.0 - content,
            ),
            col_layer2_active: solve_overlay_color(
                col_layer1_base,
                mix(col_layer2_base, col_on_layer2, 0.80),
                1.0 - content,
            ),
            col_layer2_disabled: solve_overlay_color(
                col_layer1_base,
                mix(col_layer2_base, m3.background, 0.8),
                1.0 - content,
            ),
            col_on_layer2,
            col_on_layer2_disabled: mix(col_on_layer2, m3.background, 0.4),
            col_layer3_base,
            col_layer3: solve_overlay_color(col_layer2_base, col_layer3_base, 1.0 - content),
            col_layer3_hover: solve_overlay_color(
                col_layer2_base,
                mix(col_layer3_base, col_on_layer3, 0.90),
                1.0 - content,
            ),
            col_layer3_active: solve_overlay_color(
                col_layer2_base,
                mix(col_layer3_base, col_on_layer3, 0.80),
                1.0 - content,
            ),
            col_on_layer3,
            col_layer4_base,
            col_layer4: solve_overlay_color(col_layer3_base, col_layer4_base, 1.0 - content),
            col_layer4_hover: solve_overlay_color(
                col_layer3_base,
                mix(col_layer4_base, col_on_layer4, 0.90),
                1.0 - content,
            ),
            col_layer4_active: solve_overlay_color(
                col_layer3_base,
                mix(col_layer4_base, col_on_layer4, 0.80),
                1.0 - content,
            ),
            col_on_layer4,
            col_primary,
            col_on_primary: m3.on_primary,
            col_primary_hover: mix(col_primary, col_layer1_hover, 0.87),
            col_primary_active: mix(col_primary, col_layer1_active, 0.7),
            col_primary_container,
            col_primary_container_hover: mix(col_primary_container, col_on_primary_container, 0.9),
            col_primary_container_active: mix(col_primary_container, col_on_primary_container, 0.8),
            col_on_primary_container,
            col_secondary: m3.secondary,
            col_secondary_hover: mix(m3.secondary, col_layer1_hover, 0.85),
            col_secondary_active: mix(m3.secondary, col_layer1_active, 0.4),
            col_on_secondary: m3.on_secondary,
            col_secondary_container: m3.secondary_container,
            col_secondary_container_hover: mix(
                m3.secondary_container,
                m3.on_secondary_container,
                0.90,
            ),
            col_secondary_container_active: mix(
                m3.secondary_container,
                m3.on_secondary_container,
                0.54,
            ),
            col_on_secondary_container: m3.on_secondary_container,
            col_tertiary: m3.tertiary,
            col_tertiary_hover: mix(m3.tertiary, col_layer1_hover, 0.85),
            col_tertiary_active: mix(m3.tertiary, col_layer1_active, 0.4),
            col_tertiary_container: m3.tertiary_container,
            col_tertiary_container_hover: mix(
                m3.tertiary_container,
                m3.on_tertiary_container,
                0.90,
            ),
            col_tertiary_container_active: mix(m3.tertiary_container, col_layer1_active, 0.54),
            col_on_tertiary: m3.on_tertiary,
            col_on_tertiary_container: m3.on_tertiary_container,
            col_background_surface_container: transparentize(
                m3.surface_container,
                background_transparency,
            ),
            col_surface_container_low: solve_overlay_color(
                m3.background,
                m3.surface_container_low,
                1.0 - content,
            ),
            col_surface_container: solve_overlay_color(
                m3.surface_container_low,
                m3.surface_container,
                1.0 - content,
            ),
            col_surface_container_high: solve_overlay_color(
                m3.surface_container,
                m3.surface_container_high,
                1.0 - content,
            ),
            col_surface_container_highest: solve_overlay_color(
                m3.surface_container_high,
                m3.surface_container_highest,
                1.0 - content,
            ),
            col_surface_container_highest_hover: mix(
                m3.surface_container_highest,
                m3.on_surface,
                0.95,
            ),
            col_surface_container_highest_active: mix(
                m3.surface_container_highest,
                m3.on_surface,
                0.85,
            ),
            col_on_surface: m3.on_surface,
            col_on_surface_variant: m3.on_surface_variant,
            col_tooltip: m3.inverse_surface,
            col_on_tooltip: m3.inverse_on_surface,
            col_scrim: transparentize(m3.scrim, 0.5),
            col_shadow: transparentize(m3.shadow, 0.7),
            col_outline: m3.outline,
            col_outline_variant: m3.outline_variant,
            col_error: m3.error,
            col_error_hover: mix(m3.error, col_layer1_hover, 0.85),
            col_error_active: mix(m3.error, col_layer1_active, 0.7),
            col_on_error: m3.on_error,
            col_error_container: m3.error_container,
            col_error_container_hover: mix(m3.error_container, m3.on_error_container, 0.90),
            col_error_container_active: mix(m3.error_container, m3.on_error_container, 0.70),
            col_on_error_container: m3.on_error_container,
        }
    }
}

#[derive(Clone)]
pub struct Theme {
    pub m3: M3,
    pub colors: Colors,
    font_main: String,
    font_title: String,
    font_monospace: String,
    font_reading: String,
    font_expressive: String,
    font_nerd: String,
}

impl Theme {
    pub fn load(config: &Config) -> Self {
        let m3 = M3::read(&read_palette().unwrap_or(Value::Null));
        let background_transparency =
            match (config.transparency_enable, config.transparency_automatic) {
                (false, _) => 0.0,
                (true, false) => config.background_transparency,
                (true, true) => auto_background_transparency(config, m3.darkmode),
            };
        Theme {
            colors: Colors::compute(&m3, config, background_transparency),
            m3,
            font_main: config.font_main.clone(),
            font_title: config.font_title.clone(),
            font_monospace: config.font_monospace.clone(),
            font_reading: config.font_reading.clone(),
            font_expressive: config.font_expressive.clone(),
            font_nerd: config.font_nerd.clone(),
        }
    }

    /// Qt's Material style draws text field outlines and hints in this, not in the palette.
    pub fn qt_hint(&self) -> RGBA {
        if self.m3.darkmode {
            RGBA::new(1.0, 1.0, 1.0, 0.3)
        } else {
            RGBA::new(0.0, 0.0, 0.0, 0.38)
        }
    }

    pub fn css(&self) -> String {
        format!(
            "window, tooltip, popover {{\n{}{}  --qtHint: {};\n  --fontMain: \"{}\";\n  --fontTitle: \"{}\";\n  --fontMonospace: \"{}\";\n  --fontReading: \"{}\";\n  --fontExpressive: \"{}\";\n  --fontNerd: \"{}\";\n}}\n{}{}{}",
            self.colors.variables(),
            self.m3.variables(),
            css_rgba(self.qt_hint()),
            self.font_main,
            self.font_title,
            self.font_monospace,
            self.font_reading,
            self.font_expressive,
            self.font_nerd,
            include_str!("style.css"),
            Colors::foregrounds(),
            M3::foregrounds(),
        )
    }
}

fn auto_background_transparency(config: &Config, dark: bool) -> f32 {
    let video = [".mp4", ".webm", ".mkv", ".avi", ".mov"]
        .iter()
        .any(|suffix| config.wallpaper.ends_with(suffix));
    let source = if video {
        &config.thumbnail
    } else {
        &config.wallpaper
    };
    let vibrancy = Pixbuf::from_file_at_scale(source, 10, 10, true)
        .ok()
        .and_then(average)
        .map(|colour| (hsl_saturation(colour) + hsl_lightness(colour)) / 2.0)
        .unwrap_or(f32::NAN);
    let y = 0.5768 * (vibrancy * vibrancy) - 0.759 * vibrancy + 0.2896;
    y.clamp(0.0, 0.22) - if dark { 0.0 } else { 0.12 }
}

pub fn average(image: Pixbuf) -> Option<RGBA> {
    let bytes = image.read_pixel_bytes();
    let channels = image.n_channels() as usize;
    let stride = image.rowstride() as usize;
    let (mut sum, mut count) = ([0.0f64; 3], 0.0);
    for row in 0..image.height() as usize {
        for column in 0..image.width() as usize {
            let at = row * stride + column * channels;
            for (channel, total) in sum.iter_mut().enumerate() {
                *total += *bytes.get(at + channel)? as f64;
            }
            count += 1.0;
        }
    }
    if count == 0.0 {
        return None;
    }
    Some(RGBA::new(
        (sum[0] / count / 255.0) as f32,
        (sum[1] / count / 255.0) as f32,
        (sum[2] / count / 255.0) as f32,
        1.0,
    ))
}

pub struct Following {
    _palette: Option<gio::FileMonitor>,
    _config: Vec<watch::Watch>,
}

pub fn watch(
    theme: &SharedTheme,
    provider: &gtk4::CssProvider,
    app: &gtk4::Application,
) -> Following {
    let reload: Rc<dyn Fn()> = Rc::new({
        let theme = theme.clone();
        let provider = provider.clone();
        let app = app.clone();
        move || {
            let fresh = Theme::load(&crate::core::config::current());
            provider.load_from_string(&fresh.css());
            theme.replace(fresh);
            for window in app.windows() {
                window.queue_draw();
            }
        }
    });
    let palette = gio::File::for_path(palette_path())
        .monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE)
        .ok();
    if let Some(monitor) = &palette {
        let reload = reload.clone();
        monitor.connect_changed(move |_, _, _, event| {
            if matches!(
                event,
                gio::FileMonitorEvent::ChangesDoneHint
                    | gio::FileMonitorEvent::Created
                    | gio::FileMonitorEvent::MovedIn
            ) {
                reload();
            }
        });
    }
    let config = [
        "/appearance/transparency",
        "/appearance/extraBackgroundTint",
        "/appearance/fonts",
    ]
    .into_iter()
    .map(|pointer| {
        let reload = reload.clone();
        watch::config(pointer, move || reload())
    })
    .collect();
    Following {
        _palette: palette,
        _config: config,
    }
}

fn palette_path() -> PathBuf {
    crate::core::paths::generated().join("colors.json")
}

fn read_palette() -> Option<Value> {
    let text = std::fs::read_to_string(palette_path()).ok()?;
    serde_json::from_str(&text).ok()
}

fn m3_name(key: &str) -> String {
    let mut name = String::from("m3");
    let mut upper = false;
    for character in key.chars() {
        if character == '_' {
            upper = true;
            continue;
        }
        if upper {
            name.extend(character.to_uppercase());
        } else {
            name.push(character);
        }
        upper = false;
    }
    name
}

fn rgba(hex: &str) -> RGBA {
    RGBA::parse(hex).expect("built-in fallback colour")
}

fn hsl_lightness(colour: RGBA) -> f32 {
    let high = colour.red().max(colour.green()).max(colour.blue());
    let low = colour.red().min(colour.green()).min(colour.blue());
    (high + low) / 2.0
}

fn hsl_saturation(colour: RGBA) -> f32 {
    let high = colour.red().max(colour.green()).max(colour.blue());
    let low = colour.red().min(colour.green()).min(colour.blue());
    let lightness = (high + low) / 2.0;
    if high == low {
        return 0.0;
    }
    (high - low) / (1.0 - (2.0 * lightness - 1.0).abs())
}

fn hsl_hue(colour: RGBA) -> f32 {
    let (red, green, blue) = (colour.red(), colour.green(), colour.blue());
    let high = red.max(green).max(blue);
    let low = red.min(green).min(blue);
    if high == low {
        return -1.0;
    }
    let delta = high - low;
    let hue = if high == red {
        ((green - blue) / delta).rem_euclid(6.0)
    } else if high == green {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    };
    hue / 6.0
}

fn from_hsl(hue: f32, saturation: f32, lightness: f32, alpha: f32) -> RGBA {
    if hue < 0.0 || saturation == 0.0 {
        return RGBA::new(lightness, lightness, lightness, alpha);
    }
    let chroma = (1.0 - (2.0 * lightness - 1.0).abs()) * saturation;
    let sector = hue * 6.0;
    let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match sector as i32 {
        0 => (chroma, second, 0.0),
        1 => (second, chroma, 0.0),
        2 => (0.0, chroma, second),
        3 => (0.0, second, chroma),
        4 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let base = lightness - chroma / 2.0;
    RGBA::new(red + base, green + base, blue + base, alpha)
}

fn adapt_to_accent(colour: RGBA, accent: RGBA) -> RGBA {
    from_hsl(
        hsl_hue(accent),
        hsl_saturation(accent),
        hsl_lightness(colour),
        colour.alpha(),
    )
}

#[derive(Clone, Copy)]
pub struct Adapted {
    pub col_layer0: RGBA,
    pub col_layer1: RGBA,
    pub col_on_layer0: RGBA,
    pub col_on_layer1: RGBA,
    pub col_subtext: RGBA,
    pub col_primary: RGBA,
    pub col_primary_hover: RGBA,
    pub col_primary_active: RGBA,
    pub col_secondary: RGBA,
    pub col_secondary_container: RGBA,
    pub col_secondary_container_hover: RGBA,
    pub col_secondary_container_active: RGBA,
    pub col_on_primary: RGBA,
    pub col_on_secondary_container: RGBA,
}

impl Theme {
    pub fn adapted(&self, colour: RGBA) -> Adapted {
        let (colors, m3) = (&self.colors, &self.m3);
        let dark = hsl_lightness(colour) < 0.5;
        Adapted {
            col_layer0: mix(
                colors.col_layer0,
                colour,
                if dark && m3.darkmode { 0.6 } else { 0.5 },
            ),
            col_layer1: mix(colors.col_layer1, colour, 0.5),
            col_on_layer0: mix(colors.col_on_layer0, colour, 0.5),
            col_on_layer1: mix(colors.col_on_layer1, colour, 0.5),
            col_subtext: mix(colors.col_on_layer1, colour, 0.5),
            col_primary: mix(adapt_to_accent(colors.col_primary, colour), colour, 0.5),
            col_primary_hover: mix(
                adapt_to_accent(colors.col_primary_hover, colour),
                colour,
                0.3,
            ),
            col_primary_active: mix(
                adapt_to_accent(colors.col_primary_active, colour),
                colour,
                0.3,
            ),
            col_secondary: mix(adapt_to_accent(colors.col_secondary, colour), colour, 0.5),
            col_secondary_container: mix(m3.secondary_container, colour, 0.15),
            col_secondary_container_hover: mix(colors.col_secondary_container_hover, colour, 0.3),
            col_secondary_container_active: mix(colors.col_secondary_container_active, colour, 0.5),
            col_on_primary: mix(adapt_to_accent(m3.on_primary, colour), colour, 0.5),
            col_on_secondary_container: mix(m3.on_secondary_container, colour, 0.5),
        }
    }
}

pub fn mix(a: RGBA, b: RGBA, part: f32) -> RGBA {
    RGBA::new(
        a.red() * part + b.red() * (1.0 - part),
        a.green() * part + b.green() * (1.0 - part),
        a.blue() * part + b.blue() * (1.0 - part),
        a.alpha() * part + b.alpha() * (1.0 - part),
    )
}

pub fn is_dark(colour: RGBA) -> bool {
    hsl_lightness(colour) < 0.5
}

pub fn with_lightness(colour: RGBA, lightness: f32) -> RGBA {
    from_hsl(
        hsl_hue(colour),
        hsl_saturation(colour),
        lightness,
        colour.alpha(),
    )
}

pub fn transparentize(color: RGBA, amount: f32) -> RGBA {
    RGBA::new(
        color.red(),
        color.green(),
        color.blue(),
        color.alpha() * (1.0 - amount),
    )
}

fn solve_overlay_color(base: RGBA, target: RGBA, opacity: f32) -> RGBA {
    let inverse = 1.0 - opacity;
    let solve = |t: f32, b: f32| ((t - b * inverse) / opacity).clamp(0.0, 1.0);
    RGBA::new(
        solve(target.red(), base.red()),
        solve(target.green(), base.green()),
        solve(target.blue(), base.blue()),
        opacity,
    )
}

fn css_rgba(color: RGBA) -> String {
    format!(
        "rgba({:.3},{:.3},{:.3},{:.4})",
        color.red() * 255.0,
        color.green() * 255.0,
        color.blue() * 255.0,
        color.alpha()
    )
}
