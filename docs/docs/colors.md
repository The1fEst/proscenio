---
title: Colors from the wallpaper
sidebar_label: Colors from the wallpaper
description: How proscenio switchwall sets the wallpaper, picks a color scheme, runs matugen and writes the palette, the terminal colors, the GTK and Qt color schemes and the colors Hyprland's borders take.
---

# Colors from the wallpaper

One command changes the wallpaper, the accent color or light and dark mode, and recolors everything
that follows the wallpaper:

```bash
proscenio switchwall [--mode dark|light] [--type SCHEME] [--color [HEX|clear]] [--image PATH] [--noswitch] [PATH]
```

Every control in the shell that touches these runs it, among them the
[wallpaper selector](wallpaper-selector.md), the Quick and Colors pages of the [settings](settings.md),
the dark mode buttons of the bar and of the [quick toggles](quick-toggles.md), the launcher's `/dark`,
`/light` and `/accentcolor` actions, and `proscenio ipc call theme toggleLightDark`. The shell starts
it in a transient systemd scope of its own (`systemd-run --user --scope`), outside the shell's
process.

## Arguments

| Argument | Effect |
|---|---|
| `PATH`, `--image PATH` | The wallpaper to set: an image, or a video ending in `.mp4`, `.webm`, `.mkv`, `.avi` or `.mov` |
| `--noswitch` | Keeps the current wallpaper, `background.wallpaperPath`, and only recolors |
| `--mode dark` / `--mode light` | The mode to switch to. Without it, the mode is dark when `color-scheme` in `org.gnome.desktop.interface` is `prefer-dark`, and light otherwise |
| `--type SCHEME` | The scheme for this run. Without it, `appearance.palette.type` decides, and without that, `auto` |
| `--color HEX` | Stores `HEX` (six hex digits, with or without `#`) as the accent color |
| `--color clear` | Empties the accent color |
| `--color` | Lets you pick the accent color from the screen with `hyprpicker` |

With no wallpaper, no accent color and no `--noswitch`, a `kdialog` file picker asks for the
wallpaper, starting in `Pictures/Wallpapers/showcase`, else `Pictures/Wallpapers`, else `Pictures`.
Cancelling it prints `Aborted` and changes nothing.

Choosing a wallpaper clears the accent color. While an accent color is set, the colors come from it
instead of the wallpaper.

The command exits with 0, or with 1 when the first frame of a video cannot be extracted or the
palette cannot be written.

## Schemes

| `--type` | Scheme |
|---|---|
| `scheme-content` | Content |
| `scheme-expressive` | Expressive |
| `scheme-fidelity` | Fidelity |
| `scheme-fruit-salad` | Fruit salad |
| `scheme-monochrome` | Monochrome |
| `scheme-neutral` | Neutral |
| `scheme-rainbow` | Rainbow |
| `scheme-tonal-spot` | Tonal spot |
| `auto` | Neutral for a dull image, tonal spot for a colorful one |

`auto` scales the image to 128 pixels on its longest side and measures its colorfulness: below 40 it
picks `scheme-neutral`, otherwise `scheme-tonal-spot`. With no image it can read, a video for
instance, it picks `scheme-tonal-spot`. An unknown name counts as `auto`, with a warning
on stderr. `proscenio colors scheme-for-image` runs the same check on its own; see
[Command line](command-line.md).

The colors come from the wallpaper's most prominent color: the image is scaled down to the area of a
128 × 128 square, reduced to 128 colors, and the best-ranked one seeds the scheme. JPEG images are
decoded with `djpeg` and 16-bit PNG images with `magick`; other formats go through GdkPixbuf.

## What a run does

1. **The wallpaper**, unless the colors come from an accent color. Any running `mpvpaper` stops, the
   path is stored in `background.wallpaperPath`, and a video starts playing; with `--noswitch`, all
   of this happens to the current wallpaper. When a chosen image is smaller than the largest monitor,
   a notification offers to open Upscayl.
2. **The mode.** `color-scheme` in `org.gnome.desktop.interface` becomes `prefer-dark` or
   `prefer-light`, and `gtk-theme` becomes `adw-gtk3-dark` or `adw-gtk3`.
3. If `appearance.wallpaperTheming.enableAppsAndShell` is `false`, the run stops here.
4. **matugen**, with the user's own config and templates.
5. **The palette**, written to `material_colors.scss`.
6. **The rest, side by side**: the terminal colors, the Qt color scheme and the code editors' color.
   The command returns once all three are done.

### Video wallpapers

A video needs `mpvpaper` and `ffmpeg`; when one is missing, a notification says so and offers to
install them. The video plays muted and looped through `mpvpaper` on every monitor. `ffmpeg` saves
its first frame to `~/.config/hypr/custom/scripts/mpvpaper_thumbnails/<file name>.jpg`, which is
stored as `background.thumbnailPath` and supplies the colors.

The run also writes `~/.config/hypr/custom/scripts/__restore_video_wallpaper.sh`, an executable script
that starts the same video on every monitor again (it calls `hyprctl` and `jq`). Setting an image
wallpaper replaces it with a script that does nothing. To bring a video wallpaper back after logging
in, run the script when Hyprland starts:

```lua
hl.on("hyprland.start", function()
    hl.exec_cmd(os.getenv("HOME") .. "/.config/hypr/custom/scripts/__restore_video_wallpaper.sh")
end)
```

## matugen

proscenio calls `matugen` with the image (or the video's first frame) or the accent color, the mode
and the scheme:

```bash
matugen --source-color-index 0 image PATH --mode dark --type scheme-tonal-spot
matugen --source-color-index 0 color hex '#8CAAEE' --mode dark --type scheme-tonal-spot
```

It passes no config, so matugen reads its own (`~/.config/matugen/config.toml` unless matugen is told
otherwise) and renders every template listed there. proscenio ships no templates. Three outputs
matter to proscenio itself:

| Output | What reads it |
|---|---|
| `~/.local/state/proscenio/generated/colors.json` | The shell's own colors, and `colors kde-selection` |
| `~/.local/state/proscenio/generated/color.txt` | The Qt color scheme and the code editors, when the colors come from an accent color |
| A Lua file the Hyprland config loads, defining `border_colors` | The window border settings, described under Hyprland border colors below |

:::warning[The shell's own colors]

The shell draws with the palette in `colors.json` and nothing else. Without a matugen template that
writes that file, the shell keeps its built-in dark gray palette whatever the wallpaper.

:::

A matugen config with the first two templates:

```toml
[templates.proscenio]
input_path = '~/.config/matugen/templates/proscenio/colors.json'
output_path = '~/.local/state/proscenio/generated/colors.json'

[templates.proscenio-source]
input_path = '~/.config/matugen/templates/proscenio/color.txt'
output_path = '~/.local/state/proscenio/generated/color.txt'
```

`color.txt` holds the source color and nothing else:

```text
{{colors.source_color.default.hex}}
```

`colors.json` is one object of Material 3 roles in matugen's snake_case names, each a `#rrggbb` color:

```json
{
  "background": "{{colors.background.default.hex}}",
  "on_background": "{{colors.on_background.default.hex}}",
  "surface_container_low": "{{colors.surface_container_low.default.hex}}",
  "primary": "{{colors.primary.default.hex}}",
  "primary_container": "{{colors.primary_container.default.hex}}",
  "on_primary_container": "{{colors.on_primary_container.default.hex}}"
}
```

The shell reads these keys: `background`, `on_background`, `surface`, `surface_dim`, `surface_bright`,
`surface_container_lowest`, `surface_container_low`, `surface_container`, `surface_container_high`,
`surface_container_highest`, `on_surface`, `surface_variant`, `on_surface_variant`, `inverse_surface`,
`inverse_on_surface`, `outline`, `outline_variant`, `shadow`, `scrim`, `surface_tint`, `primary`,
`on_primary`, `primary_container`, `on_primary_container`, `inverse_primary`, `secondary`,
`on_secondary`, `secondary_container`, `on_secondary_container`, `tertiary`, `on_tertiary`,
`tertiary_container`, `on_tertiary_container`, `error`, `on_error`, `error_container`,
`on_error_container`, `primary_fixed`, `primary_fixed_dim`, `on_primary_fixed`,
`on_primary_fixed_variant`, `secondary_fixed`, `secondary_fixed_dim`, `on_secondary_fixed`,
`on_secondary_fixed_variant`, `tertiary_fixed`, `tertiary_fixed_dim`, `on_tertiary_fixed`,
`on_tertiary_fixed_variant`, `success`, `on_success`, `success_container` and
`on_success_container`. A missing key keeps its built-in color; matugen has no success roles, so
those four stay as built in unless the template writes them. The shell watches the file and
recolors as soon as it changes, and it looks dark or light by the lightness of `background`.

## The generated files

Everything lands in `~/.local/state/proscenio/generated/`:

| File | Written by | Contents |
|---|---|---|
| `colors.json` | a matugen template | The shell's palette |
| `color.txt` | proscenio, for a wallpaper; a matugen template, for an accent color | The source color as `#RRGGBB` |
| `material_colors.scss` | proscenio | `$darkmode`, `$transparent`, the 54 Material roles in camelCase (`$primary`, `$surfaceContainerLow`, …), `$success`, `$onSuccess`, `$successContainer`, `$onSuccessContainer`, and the terminal colors `$term0` to `$term15` |
| `terminal/kitty-theme.conf` | proscenio | A kitty color theme |
| `terminal/sequences.txt` | proscenio | Escape sequences that recolor a running terminal |

proscenio computes the palette in `material_colors.scss` itself, from the same image or accent color
and the same scheme matugen gets, so this file does not depend on matugen. The dark mode quick toggle
and `theme toggleLightDark` read `$darkmode` from it to know which way to switch.

## Terminal colors

With `appearance.wallpaperTheming.enableTerminal` on, the default, each run writes the two files in
`terminal/`, reloads every `kitty` with `SIGUSR1`, and writes the sequences to every terminal in
`/dev/pts`, so open terminals change colors at once. What uses them is up to you: a kitty config can
`include` `kitty-theme.conf`, and a shell's start-up file can print `sequences.txt` so that terminals
opened later match.

The 16 colors start from a built-in base palette, one set for dark mode and one for light, and lean
toward the scheme's primary color:

| Key under `appearance.wallpaperTheming.terminalGenerationProps` | Default | Effect |
|---|---|---|
| `harmony` | 0.8 | The share of the hue distance to the primary color each color moves |
| `harmonizeThreshold` | 100 | The most a hue moves, in degrees |
| `termFgBoost` | 0.35 | How much brighter (dark mode) or darker (light mode) each color gets |
| `forceDarkMode` | `false` | Generates the palette in dark mode whatever the mode |

`term0`, the background, comes from `surfaceContainerLow` and `term15` from `onSurface`.
`forceDarkMode` applies to the roles and terminal colors in `material_colors.scss`; its `$darkmode`
stays the real mode, so the dark mode toggle still switches both ways.

## GTK and Qt

GTK apps follow the mode through `color-scheme` and `gtk-theme`, set in step 2 above.

For Qt, with `appearance.wallpaperTheming.enableQtApps` on, the default, proscenio builds two color
schemes from the source color in `color.txt` and the scheme type:

- `~/.local/share/color-schemes/MaterialYouLight.colors`
- `~/.local/share/color-schemes/MaterialYouDark.colors`

It applies the one for the current mode to `~/.config/kdeglobals`: the `Colors:View`, `Colors:Window`,
`Colors:Button`, `Colors:Selection`, `Colors:Tooltip`, `Colors:Complementary` and `Colors:Header`
groups are replaced with the scheme's, along with the `WM` colors, `frameContrast` and `contrast` in
`[KDE]`, both `ColorEffects` groups, and `ColorScheme` and `ColorSchemeHash` in `[General]`. Then it
emits `org.kde.kconfig.notify.ConfigChanged` on `/kdeglobals` and `org.kde.KGlobalSettings.notifyChange`
on `/KGlobalSettings`, so that running Qt apps whose platform theme reads `kdeglobals` reload their
palette.

Last, it runs the same step as `proscenio colors kde-selection`: the selection colors become the
palette's primary container. `primary_container` and `on_primary_container` from `colors.json` replace
the existing background keys (`BackgroundNormal`, `BackgroundAlternate`, `DecorationFocus`,
`DecorationHover`) and foreground keys (`ForegroundNormal`, `ForegroundActive`, `ForegroundInactive`)
of `[Colors:Selection]`, in `kdeglobals` and in the color scheme file `kdeglobals` names. When
`kdeglobals` changed, it sends `notifyChange` again.

## Code editors

The source color in `color.txt` becomes `material-code.primaryColor` in every one of these editor
settings files that is there:

```text
~/.config/Code/User/settings.json
~/.config/VSCodium/User/settings.json
~/.config/Code - OSS/User/settings.json
~/.config/Code - Insiders/User/settings.json
~/.config/Cursor/User/settings.json
~/.config/Antigravity/User/settings.json
~/.config/Windsurf/User/settings.json
```

The key's value is replaced in place, or the key is added at the end of the object.

## Hyprland border colors

The Windows page under Appearance in the [settings](settings-appearance.md) colors the borders of the
focused window and of the other windows with a color from the palette, at an opacity of its own. It
writes one line per border to `~/.config/hypr/settings/appearance.lua` and reloads Hyprland:

```lua
if border_colors and border_colors.primary then hl.config({ general = { col = { active_border = "rgba(" .. border_colors.primary .. "CC)" } } }) end
```

`border_colors` is a global Lua table the rest of the Hyprland config defines before it loads the
settings files, typically in a file a matugen template writes. Its values are six hex digits with no
`#`, since the line appends the opacity. These are the keys the settings offer:

| Key | Offered as |
|---|---|
| `active` | Default, for the focused window |
| `inactive` | Default, for the other windows |
| `outline` | Outline |
| `outline_variant` | Outline variant |
| `primary` | Primary |
| `secondary` | Secondary |
| `tertiary` | Tertiary |

A matugen template for it:

```lua
border_colors = {
    active          = "{{colors.outline_variant.default.hex_stripped}}",
    inactive        = "{{colors.surface_container_low.default.hex_stripped}}",
    outline         = "{{colors.outline.default.hex_stripped}}",
    outline_variant = "{{colors.outline_variant.default.hex_stripped}}",
    primary         = "{{colors.primary.default.hex_stripped}}",
    secondary       = "{{colors.secondary.default.hex_stripped}}",
    tertiary        = "{{colors.tertiary.default.hex_stripped}}",
}
```

Which roles `active` and `inactive` stand for is the template's choice. The guard on each line
means a missing table or key leaves that border as the rest of the config sets it. Until a border is
changed in the settings, no line is written, and the page shows Default at 47 % for the focused
window and 20 % for the others.

## Settings

| Key in `config.toml` | Default | Meaning |
|---|---|---|
| `background.wallpaperPath` | | The wallpaper; `switchwall` writes it |
| `background.thumbnailPath` | | The first frame of a video wallpaper; `switchwall` writes it |
| `appearance.palette.type` | `auto` | The scheme, one of the `--type` values |
| `appearance.palette.accentColor` | empty | An accent color that replaces the wallpaper as the source of the colors |
| `appearance.wallpaperTheming.enableAppsAndShell` | `true` | Off, a run only sets the wallpaper and the mode |
| `appearance.wallpaperTheming.enableQtApps` | `true` | The Qt color scheme |
| `appearance.wallpaperTheming.enableTerminal` | `true` | The terminal colors |

The terminal keys are in the table under Terminal colors above. The Colors page under
Appearance in the [settings](settings-appearance.md) changes all of them and runs
`proscenio switchwall --noswitch` after a change of scheme or accent color. Where the files live is
listed in [Files and folders](files.md).
