use material_colors::color::Argb;
use material_colors::hct::Hct;
use material_colors::temperature::TemperatureCache;
use material_colors::utils::math::sanitize_degrees_double;

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
        let ascending = max_chroma(middle) < max_chroma(middle + STEP);
        if max_chroma(middle) >= chroma - EPSILON {
            if (lower - PIVOT).abs() < (upper - PIVOT).abs() {
                upper = middle;
            } else {
                if lower == middle {
                    return Hct::from(hue, chroma, lower as f64);
                }
                lower = middle;
            }
        } else if ascending {
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
    dark: bool,
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
    pub fn new(name: &str, source: Hct, dark: bool) -> Self {
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
            dark,
            primary,
            secondary,
            tertiary,
            neutral,
            neutral_variant,
            error: palette(25.0, 84.0),
        }
    }

    fn fidelity(&self) -> bool {
        matches!(self.variant, Variant::Fidelity | Variant::Content)
    }

    fn monochrome(&self) -> bool {
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

fn python_round(value: f64) -> f64 {
    value.round_ties_even()
}

pub fn fix_if_disliked(hct: Hct) -> Hct {
    let hue = python_round(hct.get_hue());
    let disliked = (90.0..=111.0).contains(&hue)
        && python_round(hct.get_chroma()) > 16.0
        && python_round(hct.get_tone()) < 65.0;
    if disliked {
        Hct::from(hct.get_hue(), hct.get_chroma(), 70.0)
    } else {
        hct
    }
}

fn lab_f(t: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    if t > e {
        t.powf(1.0 / 3.0)
    } else {
        (kappa * t + 16.0) / 116.0
    }
}

fn lab_invf(ft: f64) -> f64 {
    let e = 216.0 / 24389.0;
    let kappa = 24389.0 / 27.0;
    let cubed = ft * ft * ft;
    if cubed > e {
        cubed
    } else {
        (116.0 * ft - 16.0) / kappa
    }
}

fn y_from_lstar(lstar: f64) -> f64 {
    100.0 * lab_invf((lstar + 16.0) / 116.0)
}

fn lstar_from_y(y: f64) -> f64 {
    lab_f(y / 100.0) * 116.0 - 16.0
}

fn ratio_of_ys(first: f64, second: f64) -> f64 {
    let lighter = if first > second { first } else { second };
    let darker = if lighter == second { first } else { second };
    (lighter + 5.0) / (darker + 5.0)
}

fn ratio_of_tones(first: f64, second: f64) -> f64 {
    ratio_of_ys(
        y_from_lstar(first.clamp(0.0, 100.0)),
        y_from_lstar(second.clamp(0.0, 100.0)),
    )
}

fn lighter(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let dark = y_from_lstar(tone);
    let light = ratio * (dark + 5.0) - 5.0;
    let real = ratio_of_ys(light, dark);
    if real < ratio && (real - ratio).abs() > 0.04 {
        return -1.0;
    }
    let value = lstar_from_y(light) + 0.4;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

fn darker(tone: f64, ratio: f64) -> f64 {
    if !(0.0..=100.0).contains(&tone) {
        return -1.0;
    }
    let light = y_from_lstar(tone);
    let dark = (light + 5.0) / ratio - 5.0;
    let real = ratio_of_ys(light, dark);
    if real < ratio && (real - ratio).abs() > 0.04 {
        return -1.0;
    }
    let value = lstar_from_y(dark) - 0.4;
    if !(0.0..=100.0).contains(&value) {
        return -1.0;
    }
    value
}

fn lighter_unsafe(tone: f64, ratio: f64) -> f64 {
    let safe = lighter(tone, ratio);
    if safe < 0.0 { 100.0 } else { safe }
}

fn darker_unsafe(tone: f64, ratio: f64) -> f64 {
    let safe = darker(tone, ratio);
    if safe < 0.0 { 0.0 } else { safe }
}

fn prefers_light_foreground(tone: f64) -> bool {
    python_round(tone) < 60.0
}

fn foreground_tone(background: f64, ratio: f64) -> f64 {
    let lighter_tone = lighter_unsafe(background, ratio);
    let darker_tone = darker_unsafe(background, ratio);
    let lighter_ratio = ratio_of_tones(lighter_tone, background);
    let darker_ratio = ratio_of_tones(darker_tone, background);
    if prefers_light_foreground(background) {
        let negligible = (lighter_ratio - darker_ratio).abs() < 0.1
            && lighter_ratio < ratio
            && darker_ratio < ratio;
        if lighter_ratio >= ratio || lighter_ratio >= darker_ratio || negligible {
            lighter_tone
        } else {
            darker_tone
        }
    } else if darker_ratio >= ratio || darker_ratio >= lighter_ratio {
        darker_tone
    } else {
        lighter_tone
    }
}

fn curve([low, normal, medium, high]: [f64; 4], level: f64) -> f64 {
    let lerp = |start: f64, stop: f64, amount: f64| (1.0 - amount) * start + amount * stop;
    if level <= -1.0 {
        low
    } else if level < 0.0 {
        lerp(low, normal, level + 1.0)
    } else if level < 0.5 {
        lerp(normal, medium, level / 0.5)
    } else if level < 1.0 {
        lerp(medium, high, (level - 0.5) / 0.5)
    } else {
        high
    }
}

fn find_desired_chroma_by_tone(hue: f64, chroma: f64, tone: f64, decreasing: bool) -> f64 {
    let mut answer = tone;
    let mut closest = Hct::from(hue, chroma, tone);
    if closest.get_chroma() < chroma {
        let mut peak = closest.get_chroma();
        while closest.get_chroma() < chroma {
            answer += if decreasing { -1.0 } else { 1.0 };
            let candidate = Hct::from(hue, chroma, answer);
            if peak > candidate.get_chroma() {
                break;
            }
            if (candidate.get_chroma() - chroma).abs() < 0.4 {
                break;
            }
            if (candidate.get_chroma() - chroma).abs() < (closest.get_chroma() - chroma).abs() {
                closest = candidate;
            }
            peak = peak.max(candidate.get_chroma());
        }
    }
    answer
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Polarity {
    Nearer,
    Lighter,
}

struct Pair {
    a: Role,
    b: Role,
    delta: f64,
    polarity: Polarity,
    stay_together: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    PrimaryKey,
    SecondaryKey,
    TertiaryKey,
    NeutralKey,
    NeutralVariantKey,
    Background,
    OnBackground,
    Surface,
    SurfaceDim,
    SurfaceBright,
    SurfaceContainerLowest,
    SurfaceContainerLow,
    SurfaceContainer,
    SurfaceContainerHigh,
    SurfaceContainerHighest,
    OnSurface,
    SurfaceVariant,
    OnSurfaceVariant,
    InverseSurface,
    InverseOnSurface,
    Outline,
    OutlineVariant,
    Shadow,
    Scrim,
    SurfaceTint,
    Primary,
    OnPrimary,
    PrimaryContainer,
    OnPrimaryContainer,
    InversePrimary,
    Secondary,
    OnSecondary,
    SecondaryContainer,
    OnSecondaryContainer,
    Tertiary,
    OnTertiary,
    TertiaryContainer,
    OnTertiaryContainer,
    Error,
    OnError,
    ErrorContainer,
    OnErrorContainer,
    PrimaryFixed,
    PrimaryFixedDim,
    OnPrimaryFixed,
    OnPrimaryFixedVariant,
    SecondaryFixed,
    SecondaryFixedDim,
    OnSecondaryFixed,
    OnSecondaryFixedVariant,
    TertiaryFixed,
    TertiaryFixedDim,
    OnTertiaryFixed,
    OnTertiaryFixedVariant,
}

pub const ROLES: [(&str, Role); 54] = [
    ("primary_paletteKeyColor", Role::PrimaryKey),
    ("secondary_paletteKeyColor", Role::SecondaryKey),
    ("tertiary_paletteKeyColor", Role::TertiaryKey),
    ("neutral_paletteKeyColor", Role::NeutralKey),
    ("neutral_variant_paletteKeyColor", Role::NeutralVariantKey),
    ("background", Role::Background),
    ("onBackground", Role::OnBackground),
    ("surface", Role::Surface),
    ("surfaceDim", Role::SurfaceDim),
    ("surfaceBright", Role::SurfaceBright),
    ("surfaceContainerLowest", Role::SurfaceContainerLowest),
    ("surfaceContainerLow", Role::SurfaceContainerLow),
    ("surfaceContainer", Role::SurfaceContainer),
    ("surfaceContainerHigh", Role::SurfaceContainerHigh),
    ("surfaceContainerHighest", Role::SurfaceContainerHighest),
    ("onSurface", Role::OnSurface),
    ("surfaceVariant", Role::SurfaceVariant),
    ("onSurfaceVariant", Role::OnSurfaceVariant),
    ("inverseSurface", Role::InverseSurface),
    ("inverseOnSurface", Role::InverseOnSurface),
    ("outline", Role::Outline),
    ("outlineVariant", Role::OutlineVariant),
    ("shadow", Role::Shadow),
    ("scrim", Role::Scrim),
    ("surfaceTint", Role::SurfaceTint),
    ("primary", Role::Primary),
    ("onPrimary", Role::OnPrimary),
    ("primaryContainer", Role::PrimaryContainer),
    ("onPrimaryContainer", Role::OnPrimaryContainer),
    ("inversePrimary", Role::InversePrimary),
    ("secondary", Role::Secondary),
    ("onSecondary", Role::OnSecondary),
    ("secondaryContainer", Role::SecondaryContainer),
    ("onSecondaryContainer", Role::OnSecondaryContainer),
    ("tertiary", Role::Tertiary),
    ("onTertiary", Role::OnTertiary),
    ("tertiaryContainer", Role::TertiaryContainer),
    ("onTertiaryContainer", Role::OnTertiaryContainer),
    ("error", Role::Error),
    ("onError", Role::OnError),
    ("errorContainer", Role::ErrorContainer),
    ("onErrorContainer", Role::OnErrorContainer),
    ("primaryFixed", Role::PrimaryFixed),
    ("primaryFixedDim", Role::PrimaryFixedDim),
    ("onPrimaryFixed", Role::OnPrimaryFixed),
    ("onPrimaryFixedVariant", Role::OnPrimaryFixedVariant),
    ("secondaryFixed", Role::SecondaryFixed),
    ("secondaryFixedDim", Role::SecondaryFixedDim),
    ("onSecondaryFixed", Role::OnSecondaryFixed),
    ("onSecondaryFixedVariant", Role::OnSecondaryFixedVariant),
    ("tertiaryFixed", Role::TertiaryFixed),
    ("tertiaryFixedDim", Role::TertiaryFixedDim),
    ("onTertiaryFixed", Role::OnTertiaryFixed),
    ("onTertiaryFixedVariant", Role::OnTertiaryFixedVariant),
];

fn highest_surface(scheme: &Scheme) -> Role {
    if scheme.dark {
        Role::SurfaceBright
    } else {
        Role::SurfaceDim
    }
}

fn pair(a: Role, b: Role, polarity: Polarity, stay_together: bool) -> Option<Pair> {
    Some(Pair {
        a,
        b,
        delta: 10.0,
        polarity,
        stay_together,
    })
}

impl Role {
    fn palette(self, scheme: &Scheme) -> &Palette {
        use Role::*;
        match self {
            PrimaryKey
            | SurfaceTint
            | Primary
            | OnPrimary
            | PrimaryContainer
            | OnPrimaryContainer
            | InversePrimary
            | PrimaryFixed
            | PrimaryFixedDim
            | OnPrimaryFixed
            | OnPrimaryFixedVariant => &scheme.primary,
            SecondaryKey
            | Secondary
            | OnSecondary
            | SecondaryContainer
            | OnSecondaryContainer
            | SecondaryFixed
            | SecondaryFixedDim
            | OnSecondaryFixed
            | OnSecondaryFixedVariant => &scheme.secondary,
            TertiaryKey
            | Tertiary
            | OnTertiary
            | TertiaryContainer
            | OnTertiaryContainer
            | TertiaryFixed
            | TertiaryFixedDim
            | OnTertiaryFixed
            | OnTertiaryFixedVariant => &scheme.tertiary,
            NeutralVariantKey | SurfaceVariant | OnSurfaceVariant | Outline | OutlineVariant => {
                &scheme.neutral_variant
            }
            Error | OnError | ErrorContainer | OnErrorContainer => &scheme.error,
            _ => &scheme.neutral,
        }
    }

    fn background(self, scheme: &Scheme) -> Option<Role> {
        use Role::*;
        Some(match self {
            OnBackground => Background,
            OnSurface | OnSurfaceVariant | Outline | OutlineVariant | Primary
            | PrimaryContainer | Secondary | SecondaryContainer | Tertiary | TertiaryContainer
            | Error | ErrorContainer | PrimaryFixed | PrimaryFixedDim | SecondaryFixed
            | SecondaryFixedDim | TertiaryFixed | TertiaryFixedDim => highest_surface(scheme),
            InverseOnSurface | InversePrimary => InverseSurface,
            OnPrimary => Primary,
            OnPrimaryContainer => PrimaryContainer,
            OnSecondary => Secondary,
            OnSecondaryContainer => SecondaryContainer,
            OnTertiary => Tertiary,
            OnTertiaryContainer => TertiaryContainer,
            OnError => Error,
            OnErrorContainer => ErrorContainer,
            OnPrimaryFixed | OnPrimaryFixedVariant => PrimaryFixedDim,
            OnSecondaryFixed | OnSecondaryFixedVariant => SecondaryFixedDim,
            OnTertiaryFixed | OnTertiaryFixedVariant => TertiaryFixedDim,
            _ => return None,
        })
    }

    fn second_background(self) -> Option<Role> {
        use Role::*;
        match self {
            OnPrimaryFixed | OnPrimaryFixedVariant => Some(PrimaryFixed),
            OnSecondaryFixed | OnSecondaryFixedVariant => Some(SecondaryFixed),
            OnTertiaryFixed | OnTertiaryFixedVariant => Some(TertiaryFixed),
            _ => None,
        }
    }

    fn contrast_curve(self) -> Option<[f64; 4]> {
        use Role::*;
        Some(match self {
            OnBackground => [3.0, 3.0, 4.5, 7.0],
            OnSurface | InverseOnSurface | OnPrimary | OnSecondary | OnTertiary | OnError
            | OnPrimaryFixed | OnSecondaryFixed | OnTertiaryFixed => [4.5, 7.0, 11.0, 21.0],
            OnSurfaceVariant
            | OnPrimaryContainer
            | OnSecondaryContainer
            | OnTertiaryContainer
            | OnErrorContainer
            | OnPrimaryFixedVariant
            | OnSecondaryFixedVariant
            | OnTertiaryFixedVariant => [3.0, 4.5, 7.0, 11.0],
            Outline => [1.5, 3.0, 4.5, 7.0],
            OutlineVariant | PrimaryContainer | SecondaryContainer | TertiaryContainer
            | ErrorContainer | PrimaryFixed | PrimaryFixedDim | SecondaryFixed
            | SecondaryFixedDim | TertiaryFixed | TertiaryFixedDim => [1.0, 1.0, 3.0, 4.5],
            Primary | InversePrimary | Secondary | Tertiary | Error => [3.0, 4.5, 7.0, 7.0],
            _ => return None,
        })
    }

    fn tone_delta_pair(self) -> Option<Pair> {
        use Role::*;
        match self {
            Primary | PrimaryContainer => pair(PrimaryContainer, Primary, Polarity::Nearer, false),
            Secondary | SecondaryContainer => {
                pair(SecondaryContainer, Secondary, Polarity::Nearer, false)
            }
            Tertiary | TertiaryContainer => {
                pair(TertiaryContainer, Tertiary, Polarity::Nearer, false)
            }
            Error | ErrorContainer => pair(ErrorContainer, Error, Polarity::Nearer, false),
            PrimaryFixed | PrimaryFixedDim => {
                pair(PrimaryFixed, PrimaryFixedDim, Polarity::Lighter, true)
            }
            SecondaryFixed | SecondaryFixedDim => {
                pair(SecondaryFixed, SecondaryFixedDim, Polarity::Lighter, true)
            }
            TertiaryFixed | TertiaryFixedDim => {
                pair(TertiaryFixed, TertiaryFixedDim, Polarity::Lighter, true)
            }
            _ => None,
        }
    }

    fn tone(self, scheme: &Scheme) -> f64 {
        use Role::*;
        let dark = scheme.dark;
        let mono = scheme.monochrome();
        let pick = |dark_tone: f64, light_tone: f64| if dark { dark_tone } else { light_tone };
        let level = scheme.contrast;
        match self {
            PrimaryKey => scheme.primary.key.get_tone(),
            SecondaryKey => scheme.secondary.key.get_tone(),
            TertiaryKey => scheme.tertiary.key.get_tone(),
            NeutralKey => scheme.neutral.key.get_tone(),
            NeutralVariantKey => scheme.neutral_variant.key.get_tone(),
            Background | Surface => pick(6.0, 98.0),
            OnBackground | OnSurface => pick(90.0, 10.0),
            SurfaceDim => {
                if dark {
                    6.0
                } else {
                    curve([87.0, 87.0, 80.0, 75.0], level)
                }
            }
            SurfaceBright => {
                if dark {
                    curve([24.0, 24.0, 29.0, 34.0], level)
                } else {
                    98.0
                }
            }
            SurfaceContainerLowest => {
                if dark {
                    curve([4.0, 4.0, 2.0, 0.0], level)
                } else {
                    100.0
                }
            }
            SurfaceContainerLow => curve(
                if dark {
                    [10.0, 10.0, 11.0, 12.0]
                } else {
                    [96.0, 96.0, 96.0, 95.0]
                },
                level,
            ),
            SurfaceContainer => curve(
                if dark {
                    [12.0, 12.0, 16.0, 20.0]
                } else {
                    [94.0, 94.0, 92.0, 90.0]
                },
                level,
            ),
            SurfaceContainerHigh => curve(
                if dark {
                    [17.0, 17.0, 21.0, 25.0]
                } else {
                    [92.0, 92.0, 88.0, 85.0]
                },
                level,
            ),
            SurfaceContainerHighest => curve(
                if dark {
                    [22.0, 22.0, 26.0, 30.0]
                } else {
                    [90.0, 90.0, 84.0, 80.0]
                },
                level,
            ),
            SurfaceVariant => pick(30.0, 90.0),
            OnSurfaceVariant => pick(80.0, 30.0),
            InverseSurface => pick(90.0, 20.0),
            InverseOnSurface => pick(20.0, 95.0),
            Outline => pick(60.0, 50.0),
            OutlineVariant => pick(30.0, 80.0),
            Shadow | Scrim => 0.0,
            SurfaceTint => pick(80.0, 40.0),
            Primary => {
                if mono {
                    100.0
                } else {
                    pick(80.0, 40.0)
                }
            }
            OnPrimary => {
                if mono {
                    10.0
                } else {
                    pick(20.0, 100.0)
                }
            }
            PrimaryContainer => {
                if scheme.fidelity() {
                    scheme.source.get_tone()
                } else if mono {
                    85.0
                } else {
                    pick(30.0, 90.0)
                }
            }
            OnPrimaryContainer => {
                if scheme.fidelity() {
                    foreground_tone(PrimaryContainer.tone(scheme), 4.5)
                } else if mono {
                    0.0
                } else {
                    pick(90.0, 30.0)
                }
            }
            InversePrimary => pick(40.0, 80.0),
            Secondary => pick(80.0, 40.0),
            OnSecondary => {
                if mono {
                    10.0
                } else {
                    pick(20.0, 100.0)
                }
            }
            SecondaryContainer => {
                let initial = pick(30.0, 90.0);
                if mono {
                    pick(30.0, 85.0)
                } else if scheme.fidelity() {
                    initial
                } else {
                    find_desired_chroma_by_tone(
                        scheme.secondary.hue,
                        scheme.secondary.chroma,
                        initial,
                        !dark,
                    )
                }
            }
            OnSecondaryContainer => {
                if mono {
                    pick(90.0, 10.0)
                } else if !scheme.fidelity() {
                    pick(90.0, 30.0)
                } else {
                    foreground_tone(SecondaryContainer.tone(scheme), 4.5)
                }
            }
            Tertiary => {
                if mono {
                    pick(90.0, 25.0)
                } else {
                    pick(80.0, 40.0)
                }
            }
            OnTertiary => {
                if mono {
                    pick(10.0, 90.0)
                } else {
                    pick(20.0, 100.0)
                }
            }
            TertiaryContainer => {
                if !mono {
                    pick(60.0, 49.0)
                } else if !scheme.fidelity() {
                    pick(30.0, 90.0)
                } else {
                    let proposed = Hct::new(scheme.tertiary.tone(scheme.source.get_tone()));
                    fix_if_disliked(proposed).get_tone()
                }
            }
            OnTertiaryContainer => {
                if !mono {
                    pick(0.0, 100.0)
                } else if !scheme.fidelity() {
                    pick(90.0, 30.0)
                } else {
                    foreground_tone(TertiaryContainer.tone(scheme), 4.5)
                }
            }
            Error => pick(80.0, 40.0),
            OnError => pick(20.0, 100.0),
            ErrorContainer => pick(30.0, 90.0),
            OnErrorContainer => {
                if dark {
                    90.0
                } else if mono {
                    10.0
                } else {
                    30.0
                }
            }
            PrimaryFixed | TertiaryFixed => {
                if mono {
                    40.0
                } else {
                    90.0
                }
            }
            PrimaryFixedDim | TertiaryFixedDim => {
                if mono {
                    30.0
                } else {
                    80.0
                }
            }
            OnPrimaryFixed | OnTertiaryFixed => {
                if mono {
                    100.0
                } else {
                    10.0
                }
            }
            OnPrimaryFixedVariant | OnTertiaryFixedVariant => {
                if mono {
                    90.0
                } else {
                    30.0
                }
            }
            SecondaryFixed => {
                if mono {
                    80.0
                } else {
                    90.0
                }
            }
            SecondaryFixedDim => {
                if mono {
                    70.0
                } else {
                    80.0
                }
            }
            OnSecondaryFixed => 10.0,
            OnSecondaryFixedVariant => {
                if mono {
                    25.0
                } else {
                    30.0
                }
            }
        }
    }

    fn tone_in(self, scheme: &Scheme) -> f64 {
        let decreasing = scheme.contrast < 0.0;
        if let Some(pair) = self.tone_delta_pair() {
            let background = self
                .background(scheme)
                .map(|role| role.tone_in(scheme))
                .unwrap_or(0.0);
            let a_nearer = pair.polarity == Polarity::Nearer
                || (pair.polarity == Polarity::Lighter && !scheme.dark);
            let (nearer, farther) = if a_nearer {
                (pair.a, pair.b)
            } else {
                (pair.b, pair.a)
            };
            let am_nearer = self == nearer;
            let expansion = if scheme.dark { 1.0 } else { -1.0 };
            let near_contrast = curve(nearer.contrast_curve().unwrap_or_default(), scheme.contrast);
            let far_contrast = curve(
                farther.contrast_curve().unwrap_or_default(),
                scheme.contrast,
            );
            let near_initial = nearer.tone(scheme);
            let mut near = if ratio_of_tones(background, near_initial) >= near_contrast {
                near_initial
            } else {
                foreground_tone(background, near_contrast)
            };
            let far_initial = farther.tone(scheme);
            let mut far = if ratio_of_tones(background, far_initial) >= far_contrast {
                far_initial
            } else {
                foreground_tone(background, far_contrast)
            };
            if decreasing {
                near = foreground_tone(background, near_contrast);
                far = foreground_tone(background, far_contrast);
            }
            if (far - near) * expansion < pair.delta {
                far = (far - pair.delta * expansion).clamp(0.0, 100.0);
            }
            let shifted = near + pair.delta * expansion;
            if (50.0..60.0).contains(&near) {
                if expansion > 0.0 {
                    near = 60.0;
                    far = far.max(shifted);
                } else {
                    near = 49.0;
                    far = far.min(shifted);
                }
            } else if (50.0..60.0).contains(&far) {
                if pair.stay_together {
                    if expansion > 0.0 {
                        near = 60.0;
                        far = far.max(shifted);
                    } else {
                        near = 49.0;
                        far = far.min(shifted);
                    }
                } else if expansion > 0.0 {
                    far = 60.0;
                } else {
                    far = 49.0;
                }
            }
            return if am_nearer { near } else { far };
        }

        let mut answer = self.tone(scheme);
        let Some(background_role) = self.background(scheme) else {
            return answer;
        };
        let background = background_role.tone_in(scheme);
        let desired = curve(self.contrast_curve().unwrap_or_default(), scheme.contrast);
        if ratio_of_tones(background, answer) < desired {
            answer = foreground_tone(background, desired);
        }
        if decreasing {
            answer = foreground_tone(background, desired);
        }
        if (50.0..60.0).contains(&answer) {
            answer = if ratio_of_tones(49.0, background) >= desired {
                49.0
            } else {
                60.0
            };
        }
        if let Some(second) = self.second_background() {
            let first_tone = background;
            let second_tone = second.tone_in(scheme);
            let upper = first_tone.max(second_tone);
            let lower = first_tone.min(second_tone);
            if ratio_of_tones(upper, answer) >= desired && ratio_of_tones(lower, answer) >= desired
            {
                return answer;
            }
            let light = lighter(upper, desired);
            let dark = darker(lower, desired);
            let prefers_light =
                prefers_light_foreground(first_tone) || prefers_light_foreground(second_tone);
            return if prefers_light && (light == -1.0 || dark == -1.0) {
                light
            } else {
                dark
            };
        }
        answer
    }
}
