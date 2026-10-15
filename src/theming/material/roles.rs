use super::contrast::{
    curve, darker, find_desired_chroma_by_tone, foreground_tone, lighter, prefers_light_foreground,
    ratio_of_tones,
};
use super::{Palette, Scheme, fix_if_disliked};
use material_colors::hct::Hct;

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
    if scheme.is_dark {
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
    pub(super) fn palette(self, scheme: &Scheme) -> &Palette {
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
        let is_dark = scheme.is_dark;
        let is_monochrome = scheme.is_monochrome();
        let pick = |dark_tone: f64, light_tone: f64| {
            if is_dark { dark_tone } else { light_tone }
        };
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
                if is_dark {
                    6.0
                } else {
                    curve([87.0, 87.0, 80.0, 75.0], level)
                }
            }
            SurfaceBright => {
                if is_dark {
                    curve([24.0, 24.0, 29.0, 34.0], level)
                } else {
                    98.0
                }
            }
            SurfaceContainerLowest => {
                if is_dark {
                    curve([4.0, 4.0, 2.0, 0.0], level)
                } else {
                    100.0
                }
            }
            SurfaceContainerLow => curve(
                if is_dark {
                    [10.0, 10.0, 11.0, 12.0]
                } else {
                    [96.0, 96.0, 96.0, 95.0]
                },
                level,
            ),
            SurfaceContainer => curve(
                if is_dark {
                    [12.0, 12.0, 16.0, 20.0]
                } else {
                    [94.0, 94.0, 92.0, 90.0]
                },
                level,
            ),
            SurfaceContainerHigh => curve(
                if is_dark {
                    [17.0, 17.0, 21.0, 25.0]
                } else {
                    [92.0, 92.0, 88.0, 85.0]
                },
                level,
            ),
            SurfaceContainerHighest => curve(
                if is_dark {
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
                if is_monochrome {
                    pick(100.0, 0.0)
                } else {
                    pick(80.0, 40.0)
                }
            }
            OnPrimary => {
                if is_monochrome {
                    pick(10.0, 90.0)
                } else {
                    pick(20.0, 100.0)
                }
            }
            PrimaryContainer => {
                if scheme.is_fidelity() {
                    scheme.source.get_tone()
                } else if is_monochrome {
                    pick(85.0, 25.0)
                } else {
                    pick(30.0, 90.0)
                }
            }
            OnPrimaryContainer => {
                if scheme.is_fidelity() {
                    foreground_tone(PrimaryContainer.tone(scheme), 4.5)
                } else if is_monochrome {
                    pick(0.0, 100.0)
                } else {
                    pick(90.0, 30.0)
                }
            }
            InversePrimary => pick(40.0, 80.0),
            Secondary => pick(80.0, 40.0),
            OnSecondary => {
                if is_monochrome {
                    10.0
                } else {
                    pick(20.0, 100.0)
                }
            }
            SecondaryContainer => {
                let initial = pick(30.0, 90.0);
                if is_monochrome {
                    pick(30.0, 85.0)
                } else if scheme.is_fidelity() {
                    initial
                } else {
                    find_desired_chroma_by_tone(
                        scheme.secondary.hue,
                        scheme.secondary.chroma,
                        initial,
                        !is_dark,
                    )
                }
            }
            OnSecondaryContainer => {
                if is_monochrome {
                    pick(90.0, 10.0)
                } else if !scheme.is_fidelity() {
                    pick(90.0, 30.0)
                } else {
                    foreground_tone(SecondaryContainer.tone(scheme), 4.5)
                }
            }
            Tertiary => {
                if is_monochrome {
                    pick(90.0, 25.0)
                } else {
                    pick(80.0, 40.0)
                }
            }
            OnTertiary => {
                if is_monochrome {
                    pick(10.0, 90.0)
                } else {
                    pick(20.0, 100.0)
                }
            }
            TertiaryContainer => {
                if is_monochrome {
                    pick(60.0, 49.0)
                } else if !scheme.is_fidelity() {
                    pick(30.0, 90.0)
                } else {
                    let proposed = Hct::new(scheme.tertiary.tone(scheme.source.get_tone()));
                    fix_if_disliked(proposed).get_tone()
                }
            }
            OnTertiaryContainer => {
                if is_monochrome {
                    pick(0.0, 100.0)
                } else if !scheme.is_fidelity() {
                    pick(90.0, 30.0)
                } else {
                    foreground_tone(TertiaryContainer.tone(scheme), 4.5)
                }
            }
            Error => pick(80.0, 40.0),
            OnError => pick(20.0, 100.0),
            ErrorContainer => pick(30.0, 90.0),
            OnErrorContainer => {
                if is_dark {
                    90.0
                } else if is_monochrome {
                    10.0
                } else {
                    30.0
                }
            }
            PrimaryFixed | TertiaryFixed => {
                if is_monochrome {
                    40.0
                } else {
                    90.0
                }
            }
            PrimaryFixedDim | TertiaryFixedDim => {
                if is_monochrome {
                    30.0
                } else {
                    80.0
                }
            }
            OnPrimaryFixed | OnTertiaryFixed => {
                if is_monochrome {
                    100.0
                } else {
                    10.0
                }
            }
            OnPrimaryFixedVariant | OnTertiaryFixedVariant => {
                if is_monochrome {
                    90.0
                } else {
                    30.0
                }
            }
            SecondaryFixed => {
                if is_monochrome {
                    80.0
                } else {
                    90.0
                }
            }
            SecondaryFixedDim => {
                if is_monochrome {
                    70.0
                } else {
                    80.0
                }
            }
            OnSecondaryFixed => 10.0,
            OnSecondaryFixedVariant => {
                if is_monochrome {
                    25.0
                } else {
                    30.0
                }
            }
        }
    }

    pub(super) fn tone_in(self, scheme: &Scheme) -> f64 {
        let decreasing = scheme.contrast < 0.0;
        if let Some(pair) = self.tone_delta_pair() {
            let background = self
                .background(scheme)
                .map(|role| role.tone_in(scheme))
                .unwrap_or(0.0);
            let is_a_nearer = pair.polarity == Polarity::Nearer
                || (pair.polarity == Polarity::Lighter && !scheme.is_dark);
            let (nearer, farther) = if is_a_nearer {
                (pair.a, pair.b)
            } else {
                (pair.b, pair.a)
            };
            let am_nearer = self == nearer;
            let expansion = if scheme.is_dark { 1.0 } else { -1.0 };
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
            let light_tone = lighter(upper, desired);
            let dark_tone = darker(lower, desired);
            let prefers_light =
                prefers_light_foreground(first_tone) || prefers_light_foreground(second_tone);
            return if prefers_light && (light_tone == -1.0 || dark_tone == -1.0) {
                light_tone
            } else {
                dark_tone
            };
        }
        answer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use material_colors::color::Argb;

    #[test]
    fn tertiary_containers_follow_material_color_utilities() {
        let source = Hct::new(Argb::from_u32(0xff6750a4));
        let tones = |name: &str, is_dark_mode: bool| {
            let scheme = Scheme::new(name, source, is_dark_mode);
            [Role::TertiaryContainer, Role::OnTertiaryContainer].map(|role| role.tone_in(&scheme))
        };
        assert_eq!(tones("scheme-tonal-spot", true), [30.0, 90.0]);
        assert_eq!(tones("scheme-tonal-spot", false), [90.0, 30.0]);
        assert_eq!(tones("scheme-monochrome", true), [60.0, 0.0]);
        assert_eq!(tones("scheme-monochrome", false), [49.0, 100.0]);
    }

    #[test]
    fn monochrome_primaries_follow_material_color_utilities() {
        let source = Hct::new(Argb::from_u32(0xff6750a4));
        let tones = |is_dark_mode: bool| {
            let scheme = Scheme::new("scheme-monochrome", source, is_dark_mode);
            [
                Role::Primary,
                Role::OnPrimary,
                Role::PrimaryContainer,
                Role::OnPrimaryContainer,
            ]
            .map(|role| role.tone_in(&scheme))
        };
        assert_eq!(tones(true), [100.0, 10.0, 85.0, 0.0]);
        assert_eq!(tones(false), [0.0, 90.0, 25.0, 100.0]);
    }
}
