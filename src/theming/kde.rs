use std::path::Path;
use std::process::{Command, Stdio};

use gtk4::glib;
use material_colors::color::Argb;
use material_colors::hct::Hct;

use crate::platform::appearance::with_ini_value;
use crate::theming::material::{Role, Scheme, fix_if_disliked};

const TEXT_STATES: [u32; 5] = [0x2980b9, 0x9b59b6, 0xda4453, 0xf67400, 0x27ae60];
const LINK: usize = 0;
const VISITED: usize = 1;
const NEGATIVE: usize = 2;
const NEUTRAL: usize = 3;
const POSITIVE: usize = 4;
const TEXT_SCHEME: &str = "scheme-vibrant";

const BACKGROUNDS: [Role; 25] = [
    Role::Background,
    Role::Surface,
    Role::SurfaceDim,
    Role::SurfaceBright,
    Role::SurfaceContainerLowest,
    Role::SurfaceContainerLow,
    Role::SurfaceContainer,
    Role::SurfaceContainerHigh,
    Role::SurfaceContainerHighest,
    Role::SurfaceVariant,
    Role::SurfaceTint,
    Role::Primary,
    Role::PrimaryContainer,
    Role::Secondary,
    Role::SecondaryContainer,
    Role::Tertiary,
    Role::TertiaryContainer,
    Role::Error,
    Role::ErrorContainer,
    Role::PrimaryFixed,
    Role::PrimaryFixedDim,
    Role::SecondaryFixed,
    Role::SecondaryFixedDim,
    Role::TertiaryFixed,
    Role::TertiaryFixedDim,
];

struct Mode {
    dark: bool,
    name: &'static str,
    title: &'static str,
    view_background: Role,
    view_hover: Role,
    active_blend: &'static str,
    inactive_blend: &'static str,
    inactive_foreground: Role,
}

const LIGHT: Mode = Mode {
    dark: false,
    name: "MaterialYouLight",
    title: "Material You Light",
    view_background: Role::SurfaceBright,
    view_hover: Role::SecondaryFixed,
    active_blend: "227,229,231",
    inactive_blend: "239,240,241",
    inactive_foreground: Role::OnSurfaceVariant,
};

const DARK: Mode = Mode {
    dark: true,
    name: "MaterialYouDark",
    title: "Material You dark",
    view_background: Role::SurfaceDim,
    view_hover: Role::InversePrimary,
    active_blend: "252,252,252",
    inactive_blend: "161,169,177",
    inactive_foreground: Role::OnSecondaryContainer,
};

enum Active {
    Role(Role),
    Link,
}

struct Group {
    name: &'static str,
    background_alternate: Role,
    background_normal: Role,
    decoration_hover: Role,
    foreground_active: Active,
    foreground_inactive: Role,
    foreground_normal: Role,
    on_selection: bool,
}

pub fn apply(source: Argb, scheme: &str, dark: bool) -> Result<(), String> {
    let folder = glib::user_data_dir().join("color-schemes");
    std::fs::create_dir_all(&folder).map_err(|error| format!("{}: {error}", folder.display()))?;
    let [light, dark_text] = schemes(source, scheme);
    for (mode, text) in [(&LIGHT, &light), (&DARK, &dark_text)] {
        for suffix in ["2", ""] {
            let path = folder.join(format!("{}{suffix}.colors", mode.name));
            std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
        }
    }
    let (mode, text) = if dark {
        (&DARK, &dark_text)
    } else {
        (&LIGHT, &light)
    };
    let _ = apply_scheme(&folder.join(format!("{}2.colors", mode.name)));
    apply_scheme(&folder.join(format!("{}.colors", mode.name)))?;
    if let Some(hash) = glib::compute_checksum_for_data(glib::ChecksumType::Sha1, text.as_bytes()) {
        let kdeglobals = glib::user_config_dir().join("kdeglobals");
        if let Ok(current) = std::fs::read_to_string(&kdeglobals) {
            let updated = with_ini_value(&current, "ColorSchemeHash", &hash, "[General]");
            std::fs::write(&kdeglobals, updated)
                .map_err(|error| format!("{}: {error}", kdeglobals.display()))?;
        }
    }
    Ok(())
}

fn apply_scheme(path: &Path) -> Result<(), String> {
    let status = Command::new("plasma-apply-colorscheme")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("plasma-apply-colorscheme: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "plasma-apply-colorscheme {} failed: {status}",
            path.display()
        ))
    }
}

fn schemes(source: Argb, scheme: &str) -> [String; 2] {
    let light = Scheme::new(scheme, Hct::new(source), false);
    let dark = Scheme::new(scheme, Hct::new(source), true);
    [
        scheme_text(&LIGHT, &light, &text_states(false)),
        scheme_text(&DARK, &dark, &text_states(true)),
    ]
}

fn color(scheme: &Scheme, role: Role) -> Argb {
    let mut hct = Hct::new(scheme.argb(role));
    if BACKGROUNDS.contains(&role) {
        hct.set_tone(hct.get_tone().trunc());
    }
    hct.set_chroma(hct.get_chroma());
    hct.into()
}

fn text_states(dark: bool) -> [(String, String); 5] {
    TEXT_STATES.map(|value| {
        let likable: Argb = fix_if_disliked(Hct::new(Argb::from_u32(0xff00_0000 | value))).into();
        let scheme = Scheme::new(TEXT_SCHEME, Hct::new(likable), dark);
        (
            hex(color(&scheme, Role::Primary)),
            hex(color(&scheme, Role::OnPrimaryFixedVariant)),
        )
    })
}

fn hex(argb: Argb) -> String {
    format!("#{:02x}{:02x}{:02x}", argb.red, argb.green, argb.blue)
}

fn scheme_text(mode: &Mode, scheme: &Scheme, states: &[(String, String); 5]) -> String {
    let c = |role| hex(color(scheme, role));
    let groups = [
        Group {
            name: "Colors:Button",
            background_alternate: Role::SurfaceVariant,
            background_normal: Role::SurfaceContainerHigh,
            decoration_hover: Role::Primary,
            foreground_active: Active::Role(Role::OnSurface),
            foreground_inactive: Role::Outline,
            foreground_normal: Role::OnSurface,
            on_selection: false,
        },
        Group {
            name: "Colors:Complementary",
            background_alternate: Role::Surface,
            background_normal: Role::SurfaceContainer,
            decoration_hover: Role::Primary,
            foreground_active: Active::Role(Role::InverseSurface),
            foreground_inactive: Role::Outline,
            foreground_normal: Role::OnSurfaceVariant,
            on_selection: false,
        },
        Group {
            name: "Colors:Header",
            background_alternate: Role::SurfaceContainer,
            background_normal: Role::SurfaceContainer,
            decoration_hover: Role::Primary,
            foreground_active: Active::Role(Role::InverseSurface),
            foreground_inactive: Role::Outline,
            foreground_normal: Role::OnSurfaceVariant,
            on_selection: false,
        },
        Group {
            name: "Colors:Header][Inactive",
            background_alternate: Role::SurfaceContainer,
            background_normal: Role::SurfaceContainer,
            decoration_hover: Role::Primary,
            foreground_active: Active::Role(Role::InverseSurface),
            foreground_inactive: Role::Outline,
            foreground_normal: Role::OnSurfaceVariant,
            on_selection: false,
        },
        Group {
            name: "Colors:Selection",
            background_alternate: Role::Primary,
            background_normal: Role::Primary,
            decoration_hover: Role::Secondary,
            foreground_active: Active::Role(Role::OnPrimary),
            foreground_inactive: Role::OnPrimary,
            foreground_normal: Role::OnPrimary,
            on_selection: true,
        },
        Group {
            name: "Colors:Tooltip",
            background_alternate: Role::SurfaceVariant,
            background_normal: Role::SurfaceContainer,
            decoration_hover: Role::Primary,
            foreground_active: Active::Role(Role::OnSurface),
            foreground_inactive: Role::Outline,
            foreground_normal: Role::OnSurface,
            on_selection: false,
        },
        Group {
            name: "Colors:View",
            background_alternate: Role::SurfaceContainer,
            background_normal: mode.view_background,
            decoration_hover: mode.view_hover,
            foreground_active: Active::Role(Role::InverseSurface),
            foreground_inactive: Role::Outline,
            foreground_normal: Role::OnSurface,
            on_selection: false,
        },
        Group {
            name: "Colors:Window",
            background_alternate: Role::SurfaceVariant,
            background_normal: Role::SurfaceContainer,
            decoration_hover: Role::Primary,
            foreground_active: Active::Link,
            foreground_inactive: Role::Outline,
            foreground_normal: Role::OnSurfaceVariant,
            on_selection: false,
        },
    ];

    let mut text = format!(
        "[ColorEffects:Disabled]\nColor={}\nColorAmount=0.5\nColorEffect=3\nContrastAmount=0\nContrastEffect=0\nIntensityAmount=0\nIntensityEffect=0\n\n\
         [ColorEffects:Inactive]\nChangeSelectionColor=true\nColor={}\nColorAmount=0.025\nColorEffect=0\nContrastAmount=0.1\nContrastEffect=0\nEnable={}\nIntensityAmount=0\nIntensityEffect=0\n\n",
        c(Role::SurfaceContainer),
        c(Role::SurfaceContainerLowest),
        mode.dark,
    );
    for group in groups {
        let state = |index: usize| {
            let (primary, on_selection) = &states[index];
            if group.on_selection {
                on_selection
            } else {
                primary
            }
        };
        let active = match group.foreground_active {
            Active::Role(role) => c(role),
            Active::Link => states[LINK].0.clone(),
        };
        text.push_str(&format!(
            "[{}]\nBackgroundAlternate={}\nBackgroundNormal={}\nDecorationFocus={}\nDecorationHover={}\nForegroundActive={active}\nForegroundInactive={}\nForegroundLink={}\nForegroundNegative={}\nForegroundNeutral={}\nForegroundNormal={}\nForegroundPositive={}\nForegroundVisited={}\n\n",
            group.name,
            c(group.background_alternate),
            c(group.background_normal),
            c(Role::Primary),
            c(group.decoration_hover),
            c(group.foreground_inactive),
            state(LINK),
            state(NEGATIVE),
            state(NEUTRAL),
            c(group.foreground_normal),
            state(POSITIVE),
            state(VISITED),
        ));
    }
    text.push_str(&format!(
        "[General]\nColorScheme={}\nName={}\nshadeSortColumn=true\n\n[KDE]\ncontrast=4\n\n\
         [WM]\nactiveBackground=#ff{}\nactiveBlend={}\nactiveForeground={}\ninactiveBackground=#ff{}\ninactiveBlend={}\ninactiveForeground={}\n",
        mode.name,
        mode.title,
        &c(Role::SurfaceContainerHighest)[1..],
        mode.active_blend,
        c(Role::OnSurface),
        &c(Role::SecondaryContainer)[1..],
        mode.inactive_blend,
        c(mode.inactive_foreground),
    ));
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schemes_match_kde_material_you_colors_for_one_seed() {
        let [light, dark] = schemes(Argb::from_u32(0xff4c3d50), "scheme-tonal-spot");
        let expected = [
            (
                &dark,
                "[Colors:Button]\nBackgroundAlternate=#4a424b\nBackgroundNormal=#2b262c\nDecorationFocus=#e2b4ef\nDecorationHover=#e2b4ef\nForegroundActive=#e9e0e7\nForegroundInactive=#988e97\nForegroundLink=#8fc9fc\nForegroundNegative=#ffb3b4\nForegroundNeutral=#fcb38a\nForegroundNormal=#e9e0e7\nForegroundPositive=#00e479\nForegroundVisited=#ebb2ff\n",
            ),
            (
                &dark,
                "[Colors:Selection]\nBackgroundAlternate=#e2b4ef\nBackgroundNormal=#e2b4ef\nDecorationFocus=#e2b4ef\nDecorationHover=#d5c0d7\nForegroundActive=#452253\nForegroundInactive=#452253\nForegroundLink=#004b73\nForegroundNegative=#920023\nForegroundNeutral=#753400\nForegroundNormal=#452253\nForegroundPositive=#005228\nForegroundVisited=#74009f\n",
            ),
            (
                &dark,
                "[WM]\nactiveBackground=#ff363137\nactiveBlend=252,252,252\nactiveForeground=#e9e0e7\ninactiveBackground=#ff534457\ninactiveBlend=161,169,177\ninactiveForeground=#f1dcf3\n",
            ),
            (
                &dark,
                "[Colors:View]\nBackgroundAlternate=#231e23\nBackgroundNormal=#161217\nDecorationFocus=#e2b4ef\n",
            ),
            (
                &light,
                "[General]\nColorScheme=MaterialYouLight\nName=Material You Light\n",
            ),
        ];
        for (text, block) in expected {
            assert!(text.contains(block), "missing\n{block}\nin\n{text}");
        }
    }
}
