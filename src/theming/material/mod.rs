mod contrast;
pub mod roles;

use material_colors::color::Argb;
use material_colors::hct::Hct;
use material_colors::temperature::TemperatureCache;
use material_colors::utils::math::sanitize_degrees_double;

use self::contrast::python_round;
use self::roles::Role;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    Monochrome,
    Spritz,
    TonalSpot,
    Vibrant,
    Expressive,
    Fidelity,
    Content,
    Rainbow,
    FruitSalad,
}

pub struct Palette {
    hue: f64,
    chroma: f64,
    key: Hct,
}

impl Palette {
    fn from_hue_and_chroma(hue: f64, chroma: f64) -> Self {
        Palette {
            hue,
            chroma,
            key: key_color(hue, chroma),
        }
    }

    fn from_hct(hct: Hct) -> Self {
        Palette {
            hue: hct.get_hue(),
            chroma: hct.get_chroma(),
            key: hct,
        }
    }

    fn tone(&self, tone: f64) -> Argb {
        Hct::from(self.hue, self.chroma, tone).into()
    }
}

fn key_color(hue: f64, chroma: f64) -> Hct {
    const PIVOT: i32 = 50;
    const STEP: i32 = 1;
    const EPSILON: f64 = 0.01;
    let max_chroma = |tone: i32| Hct::from(hue, 200.0, tone as f64).get_chroma();
    let (mut lower, mut upper): (i32, i32) = (0, 100);
    while lower < upper {
        let middle = (lower + upper).div_euclid(2);
        let is_ascending = max_chroma(middle) < max_chroma(middle + STEP);
        if max_chroma(middle) >= chroma - EPSILON {
            if (lower - PIVOT).abs() < (upper - PIVOT).abs() {
                upper = middle;
            } else {
                if lower == middle {
                    return Hct::from(hue, chroma, lower as f64);
                }
                lower = middle;
            }
        } else if is_ascending {
            lower = middle + STEP;
        } else {
            upper = middle;
        }
    }
    Hct::from(hue, chroma, lower as f64)
}

pub struct Scheme {
    source: Hct,
    variant: Variant,
    contrast: f64,
    is_dark: bool,
    primary: Palette,
    secondary: Palette,
    tertiary: Palette,
    neutral: Palette,
    neutral_variant: Palette,
    error: Palette,
}

const VIBRANT_HUES: [f64; 9] = [0.0, 41.0, 61.0, 101.0, 131.0, 181.0, 251.0, 301.0, 360.0];
const VIBRANT_SECONDARY: [f64; 9] = [18.0, 15.0, 10.0, 12.0, 15.0, 18.0, 15.0, 12.0, 12.0];
const VIBRANT_TERTIARY: [f64; 9] = [35.0, 30.0, 20.0, 25.0, 30.0, 35.0, 30.0, 25.0, 25.0];
const EXPRESSIVE_HUES: [f64; 9] = [0.0, 21.0, 51.0, 121.0, 151.0, 191.0, 271.0, 321.0, 360.0];
const EXPRESSIVE_SECONDARY: [f64; 9] = [45.0, 95.0, 45.0, 20.0, 45.0, 90.0, 45.0, 45.0, 45.0];
const EXPRESSIVE_TERTIARY: [f64; 9] = [120.0, 120.0, 20.0, 45.0, 20.0, 15.0, 20.0, 120.0, 120.0];

impl Scheme {
    pub fn new(name: &str, source: Hct, is_dark: bool) -> Self {
        let hue = source.get_hue();
        let chroma = source.get_chroma();
        let palette = Palette::from_hue_and_chroma;
        let (variant, palettes) = match name {
            "scheme-fruit-salad" => (
                Variant::FruitSalad,
                [
                    palette(sanitize_degrees_double(hue - 50.0), 48.0),
                    palette(sanitize_degrees_double(hue - 50.0), 36.0),
                    palette(hue, 36.0),
                    palette(hue, 10.0),
                    palette(hue, 16.0),
                ],
            ),
            "scheme-expressive" => (
                Variant::Expressive,
                [
                    palette(sanitize_degrees_double(hue + 240.0), 40.0),
                    palette(
                        rotated_hue(hue, &EXPRESSIVE_HUES, &EXPRESSIVE_SECONDARY),
                        24.0,
                    ),
                    palette(
                        rotated_hue(hue, &EXPRESSIVE_HUES, &EXPRESSIVE_TERTIARY),
                        32.0,
                    ),
                    palette(hue + 15.0, 8.0),
                    palette(hue + 15.0, 12.0),
                ],
            ),
            "scheme-monochrome" => (
                Variant::Monochrome,
                [
                    palette(hue, 0.0),
                    palette(hue, 0.0),
                    palette(hue, 0.0),
                    palette(hue, 0.0),
                    palette(hue, 0.0),
                ],
            ),
            "scheme-rainbow" => (
                Variant::Rainbow,
                [
                    palette(hue, 48.0),
                    palette(hue, 16.0),
                    palette(sanitize_degrees_double(hue + 60.0), 24.0),
                    palette(hue, 0.0),
                    palette(hue, 0.0),
                ],
            ),
            "scheme-neutral" => (
                Variant::Spritz,
                [
                    palette(hue, 12.0),
                    palette(hue, 8.0),
                    palette(hue, 16.0),
                    palette(hue, 2.0),
                    palette(hue, 2.0),
                ],
            ),
            "scheme-fidelity" | "scheme-content" => {
                let mut temperature = TemperatureCache::new(source);
                let (variant, tertiary) = if name == "scheme-fidelity" {
                    (Variant::Fidelity, temperature.complement())
                } else {
                    (Variant::Content, temperature.analogous(Some(3), Some(6))[2])
                };
                let tertiary: Argb = fix_if_disliked(tertiary).into();
                (
                    variant,
                    [
                        palette(hue, chroma),
                        palette(hue, (chroma - 32.0).max(chroma * 0.5)),
                        Palette::from_hct(Hct::new(tertiary)),
                        palette(hue, chroma / 8.0),
                        palette(hue, chroma / 8.0 + 4.0),
                    ],
                )
            }
            "scheme-vibrant" => (
                Variant::Vibrant,
                [
                    palette(hue, 200.0),
                    palette(rotated_hue(hue, &VIBRANT_HUES, &VIBRANT_SECONDARY), 24.0),
                    palette(rotated_hue(hue, &VIBRANT_HUES, &VIBRANT_TERTIARY), 32.0),
                    palette(hue, 10.0),
                    palette(hue, 12.0),
                ],
            ),
            _ => (
                Variant::TonalSpot,
                [
                    palette(hue, 36.0),
                    palette(hue, 16.0),
                    palette(sanitize_degrees_double(hue + 60.0), 24.0),
                    palette(hue, 6.0),
                    palette(hue, 8.0),
                ],
            ),
        };
        let [primary, secondary, tertiary, neutral, neutral_variant] = palettes;
        Scheme {
            source,
            variant,
            contrast: 0.0,
            is_dark,
            primary,
            secondary,
            tertiary,
            neutral,
            neutral_variant,
            error: palette(25.0, 84.0),
        }
    }

    fn is_fidelity(&self) -> bool {
        matches!(self.variant, Variant::Fidelity | Variant::Content)
    }

    fn is_monochrome(&self) -> bool {
        self.variant == Variant::Monochrome
    }

    pub fn argb(&self, role: Role) -> Argb {
        role.palette(self).tone(role.tone_in(self))
    }
}

fn rotated_hue(hue: f64, hues: &[f64], rotations: &[f64]) -> f64 {
    for index in 0..hues.len() - 1 {
        if hues[index] < hue && hue < hues[index + 1] {
            return sanitize_degrees_double(hue + rotations[index]);
        }
    }
    hue
}

pub fn fix_if_disliked(hct: Hct) -> Hct {
    let hue = python_round(hct.get_hue());
    let is_disliked = (90.0..=111.0).contains(&hue)
        && python_round(hct.get_chroma()) > 16.0
        && python_round(hct.get_tone()) < 65.0;
    if is_disliked {
        Hct::from(hct.get_hue(), hct.get_chroma(), 70.0)
    } else {
        hct
    }
}
