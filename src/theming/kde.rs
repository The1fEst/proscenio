use std::collections::HashMap;

use gtk4::gio;
use gtk4::glib;
use gtk4::prelude::*;
use material_colors::color::Argb;
use material_colors::hct::Hct;

use crate::theming::material::{Role, Scheme, fix_if_disliked};

const COLOR_SETS: [&str; 7] = [
    "Colors:View",
    "Colors:Window",
    "Colors:Button",
    "Colors:Selection",
    "Colors:Tooltip",
    "Colors:Complementary",
    "Colors:Header",
];
const COLOR_KEYS: [&str; 12] = [
    "BackgroundNormal",
    "BackgroundAlternate",
    "ForegroundNormal",
    "ForegroundInactive",
    "ForegroundActive",
    "ForegroundLink",
    "ForegroundVisited",
    "ForegroundNegative",
    "ForegroundNeutral",
    "ForegroundPositive",
    "DecorationFocus",
    "DecorationHover",
];
const WM_COLORS: [&str; 6] = [
    "activeBackground",
    "activeForeground",
    "inactiveBackground",
    "inactiveForeground",
    "activeBlend",
    "inactiveBlend",
];
const EFFECT_GROUPS: [&str; 2] = ["ColorEffects:Inactive", "ColorEffects:Disabled"];
const EFFECTS: [&str; 9] = [
    "Enable",
    "ChangeSelectionColor",
    "IntensityEffect",
    "IntensityAmount",
    "ColorEffect",
    "ColorAmount",
    "Color",
    "ContrastEffect",
    "ContrastAmount",
];
const DEFAULT_FRAME_CONTRAST: &str = "0.2";
const DEFAULT_CONTRAST: &str = "7";
const PALETTE_CHANGED: i32 = 0;

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
        let path = folder.join(format!("{}.colors", mode.name));
        std::fs::write(&path, text).map_err(|error| format!("{}: {error}", path.display()))?;
    }
    let (mode, text) = if dark {
        (&DARK, &dark_text)
    } else {
        (&LIGHT, &light)
    };
    let kdeglobals = glib::user_config_dir().join("kdeglobals");
    let current = std::fs::read_to_string(&kdeglobals).unwrap_or_default();
    std::fs::write(&kdeglobals, applied(&current, text, mode.name))
        .map_err(|error| format!("{}: {error}", kdeglobals.display()))?;
    notify_palette();
    Ok(())
}

fn applied(kdeglobals: &str, scheme_text: &str, name: &str) -> String {
    let scheme = Ini::parse(scheme_text);
    let mut output = Ini::parse(kdeglobals);
    if let Some(hash) =
        glib::compute_checksum_for_data(glib::ChecksumType::Sha1, scheme_text.as_bytes())
    {
        output.set("General", "ColorSchemeHash", &hash);
    }
    for set in COLOR_SETS {
        output.delete(set);
        if set == "Colors:Header" && !scheme.has(set) {
            continue;
        }
        for key in COLOR_KEYS {
            if let Some(value) = scheme.entry(set, key) {
                output.set(set, key, value);
            }
        }
        let inactive = format!("{set}][Inactive");
        if scheme.has(&inactive) {
            let from = if set == "Colors:Header" {
                "Colors:Window"
            } else {
                inactive.as_str()
            };
            for key in COLOR_KEYS {
                if let Some(value) = scheme.entry(from, key) {
                    output.set(&inactive, key, value);
                }
            }
        }
    }
    for key in WM_COLORS {
        if let Some(value) = scheme.entry("WM", key) {
            output.set("WM", key, &kconfig_color(value));
        }
    }
    output.set(
        "KDE",
        "frameContrast",
        scheme
            .entry("KDE", "frameContrast")
            .unwrap_or(DEFAULT_FRAME_CONTRAST),
    );
    output.set(
        "KDE",
        "contrast",
        scheme.entry("KDE", "contrast").unwrap_or(DEFAULT_CONTRAST),
    );
    for group in EFFECT_GROUPS {
        for key in EFFECTS {
            output.set(group, key, scheme.entry(group, key).unwrap_or_default());
        }
    }
    output.set("General", "ColorScheme", name);
    output.to_string()
}

fn kconfig_color(value: &str) -> String {
    let Some(hex) = value.strip_prefix('#') else {
        return value.to_owned();
    };
    let channel = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    let (alpha, rgb) = match hex.len() {
        6 => (Some(255), 0),
        8 => (channel(0), 2),
        _ => (None, 0),
    };
    let (Some(alpha), Some(red), Some(green), Some(blue)) =
        (alpha, channel(rgb), channel(rgb + 2), channel(rgb + 4))
    else {
        return value.to_owned();
    };
    if alpha == 255 {
        format!("{red},{green},{blue}")
    } else {
        format!("{red},{green},{blue},{alpha}")
    }
}

fn notify_palette() {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return;
    };
    let changed = HashMap::from([("KDE".to_owned(), vec![b"frameContrast".to_vec()])]);
    let _ = bus.emit_signal(
        None,
        "/kdeglobals",
        "org.kde.kconfig.notify",
        "ConfigChanged",
        Some(&(changed,).to_variant()),
    );
    let _ = bus.emit_signal(
        None,
        "/KGlobalSettings",
        "org.kde.KGlobalSettings",
        "notifyChange",
        Some(&(PALETTE_CHANGED, 0i32).to_variant()),
    );
    let _ = bus.flush_sync(gio::Cancellable::NONE);
}

struct Ini {
    groups: Vec<(String, Vec<String>)>,
}

impl Ini {
    fn parse(text: &str) -> Ini {
        let mut groups = vec![(String::new(), Vec::new())];
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some(name) = trimmed
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                groups.push((name.to_owned(), Vec::new()));
            } else if let Some((_, lines)) = groups.last_mut() {
                lines.push(line.to_owned());
            }
        }
        Ini { groups }
    }

    fn has(&self, group: &str) -> bool {
        self.groups.iter().any(|(name, _)| name == group)
    }

    fn entry(&self, group: &str, key: &str) -> Option<&str> {
        let (_, lines) = self.groups.iter().find(|(name, _)| name == group)?;
        lines.iter().find_map(|line| {
            let (found, value) = line.split_once('=')?;
            (found.trim() == key).then_some(value)
        })
    }

    fn delete(&mut self, group: &str) {
        let nested = format!("{group}][");
        self.groups
            .retain(|(name, _)| name != group && !name.starts_with(&nested));
    }

    fn set(&mut self, group: &str, key: &str, value: &str) {
        let line = format!("{key}={value}");
        let at = match self.groups.iter().position(|(name, _)| name == group) {
            Some(at) => at,
            None => {
                self.groups.push((group.to_owned(), Vec::new()));
                self.groups.len() - 1
            }
        };
        let lines = &mut self.groups[at].1;
        let existing = lines.iter().position(|existing| {
            existing
                .split_once('=')
                .is_some_and(|(found, _)| found.trim() == key)
        });
        match existing {
            Some(index) => lines[index] = line,
            None => lines.push(line),
        }
    }
}

impl std::fmt::Display for Ini {
    fn fmt(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        let mut first = true;
        for (name, lines) in &self.groups {
            if name.is_empty() && lines.is_empty() {
                continue;
            }
            if !first {
                writeln!(formatter)?;
            }
            first = false;
            if !name.is_empty() {
                writeln!(formatter, "[{name}]")?;
            }
            for line in lines {
                writeln!(formatter, "{line}")?;
            }
        }
        Ok(())
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

    fn entries(ini: &Ini) -> Vec<(String, Vec<String>)> {
        let mut groups: Vec<(String, Vec<String>)> = ini
            .groups
            .iter()
            .filter(|(name, lines)| !name.is_empty() || !lines.is_empty())
            .map(|(name, lines)| {
                let mut lines = lines.clone();
                lines.sort();
                (name.clone(), lines)
            })
            .collect();
        groups.sort();
        groups
    }

    #[test]
    fn a_scheme_lands_in_kdeglobals_as_plasma_apply_colorscheme_writes_it() {
        let scheme = "[ColorEffects:Disabled]\nColor=#1e1d20\nColorAmount=0.5\nColorEffect=3\n\n\
            [ColorEffects:Inactive]\nChangeSelectionColor=true\nEnable=true\n\n\
            [Colors:Button]\nBackgroundNormal=#28272a\nForegroundNormal=#e5e1e5\nExtraKey=#123456\n\n\
            [Colors:Header]\nBackgroundNormal=#1e1d20\nForegroundActive=#e5e1e5\n\n\
            [Colors:Header][Inactive]\nBackgroundNormal=#000000\n\n\
            [Colors:Selection]\nBackgroundNormal=#c2c0eb\nForegroundNormal=#2b2a4c\n\n\
            [Colors:View]\nBackgroundNormal=#131316\nForegroundLink=#8fc9fc\n\n\
            [Colors:View][Inactive]\nBackgroundNormal=#101012\n\n\
            [Colors:Window]\nBackgroundNormal=#201f22\nDecorationFocus=#c2c0eb\n\n\
            [General]\nColorScheme=MaterialYouDark\nName=Material You dark\n\n\
            [KDE]\ncontrast=4\n\n\
            [WM]\nactiveBackground=#ff333235\nactiveBlend=252,252,252\nactiveForeground=#e5e1e5\n\
            inactiveBackground=#80444353\ninactiveBlend=161,169,177\ninactiveForeground=#b5b3c6\n";
        let kdeglobals = "[Colors:Complementary]\nBackgroundNormal=#ffffff\n\n\
            [Colors:Header][Inactive]\nBackgroundNormal=#111111\n\n\
            [Colors:Tooltip]\nBackgroundNormal=#222222\n\n\
            [General]\nColorScheme=BreezeDark\nfixed=Hack,10\n\n\
            [Icons]\nTheme=Papirus-Dark\n\n\
            [KDE]\nwidgetStyle=Darkly\n\n\
            [WM]\nactiveFont=Google Sans,10\n";
        let written = "[ColorEffects:Disabled]\nChangeSelectionColor=\nColor=#1e1d20\nColorAmount=0.5\n\
            ColorEffect=3\nContrastAmount=\nContrastEffect=\nEnable=\nIntensityAmount=\nIntensityEffect=\n\n\
            [ColorEffects:Inactive]\nChangeSelectionColor=true\nColor=\nColorAmount=\nColorEffect=\n\
            ContrastAmount=\nContrastEffect=\nEnable=true\nIntensityAmount=\nIntensityEffect=\n\n\
            [Colors:Button]\nBackgroundNormal=#28272a\nForegroundNormal=#e5e1e5\n\n\
            [Colors:Header]\nBackgroundNormal=#1e1d20\nForegroundActive=#e5e1e5\n\n\
            [Colors:Header][Inactive]\nBackgroundNormal=#201f22\nDecorationFocus=#c2c0eb\n\n\
            [Colors:Selection]\nBackgroundNormal=#c2c0eb\nForegroundNormal=#2b2a4c\n\n\
            [Colors:View]\nBackgroundNormal=#131316\nForegroundLink=#8fc9fc\n\n\
            [Colors:View][Inactive]\nBackgroundNormal=#101012\n\n\
            [Colors:Window]\nBackgroundNormal=#201f22\nDecorationFocus=#c2c0eb\n\n\
            [General]\nColorScheme=MaterialYouDark\nColorSchemeHash=44f1e75e0456d33108abe21d4cb26afa8f4c0fb6\n\
            fixed=Hack,10\n\n\
            [Icons]\nTheme=Papirus-Dark\n\n\
            [KDE]\ncontrast=4\nframeContrast=0.2\nwidgetStyle=Darkly\n\n\
            [WM]\nactiveBackground=51,50,53\nactiveBlend=252,252,252\nactiveFont=Google Sans,10\n\
            activeForeground=229,225,229\ninactiveBackground=68,67,83,128\ninactiveBlend=161,169,177\n\
            inactiveForeground=181,179,198\n";
        let applied = applied(kdeglobals, scheme, "MaterialYouDark");
        assert_eq!(
            entries(&Ini::parse(&applied)),
            entries(&Ini::parse(written)),
            "\n{applied}"
        );
    }
}
