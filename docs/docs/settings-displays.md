---
title: Display settings
sidebar_label: Displays
description: The Displays page and its Color and Night light subpages, which arrange monitors and set their mode, color, HDR and luminance through Hyprland monitor rules, and schedule night light through hyprsunset.
---

# Display settings

The Displays page arranges the monitors and sets each one's mode, rotation and refresh behavior. Its
**Color** subpage sets a display's color profile, HDR and luminance, and its **Night light** subpage
schedules a warmer screen in the evening.

## How changes are written

Everything a display's controls change goes into that display's `hl.monitor` block in
`~/.config/hypr/settings/displays.lua`, after which Hyprland reloads:

```lua
hl.monitor({
	output = "DP-1",
	mode = "2560x1440@144.00",
	position = "0x0",
	scale = 1.25,
	transform = 0,
	bitdepth = 10,
	cm = "hdr",
})
```

Each change writes what Hyprland is running for that display (mode, position, scale, rotation, bit
depth and color profile) together with the changed keys, and keeps the block's other keys, so a
display that had no rule gets a complete one. The main display is a line in the same file,
`hl.env("WAYLANDDRV_PRIMARY_MONITOR", "DP-1")`, and the options under **All displays** are
`hl.config` lines there too. The page asks Hyprland for all monitors, turned-off ones included, and
asks again when Hyprland reports a display added or removed. The block writer is
`src/platform/monitorrules.rs`.

## Displays

**Arrangement.** With more than one display, a field at the top shows every enabled display as a
plate, scaled to fit, with its model or name. Tapping a plate chooses that display. Dragging one
moves it: it snaps to the edges of its neighbors, and a plate dropped onto another is pushed out to
the nearest free side. The positions are then written with the main display at 0 × 0.

**Choosing a display.** The displays also appear as a row of buttons, "*model* (*name*)", disabled
ones included. At the end of the row, **Rescan displays** asks every monitor again what it can do
(`hl.dsp.force_renderer_reload()`); modes a display reports only once it is fully awake show up after
it. The controls below apply to the chosen display.

| Control | Choices | Writes |
|---|---|---|
| Use as | Main display, Extended display, Mirror for *another display*, Off | see below |
| Resolution | see below | `mode` and `scale` |
| Show all resolutions | — | nothing; lists every mode the panel reports while the page is open |
| Refresh rate | the rates the display offers at its current size | `mode` |
| Rotation | Standard, 90°, 180°, 270°, Flipped, Flipped 90°, Flipped 180°, Flipped 270° | `transform`, 0 to 7 |
| Variable refresh rate | Follow the global setting, Off, On, Fullscreen only, Fullscreen games and video | `vrr`: −1, 0, 1, 2, 3 |
| Color | — | opens the Color subpage for this display |

**Use as** shows only while there is another display.

- **Main display** names this display in `WAYLANDDRV_PRIMARY_MONITOR` and moves every display so that
  this one sits at 0 × 0. Without a choice, the main display is the first by connector: DP, then
  HDMI, DVI, eDP, LVDS and VGA, lower numbers first.
- **Extended display** clears `mirror`. A display that was mirroring is placed to the right of the
  others.
- **Mirror for** writes `mirror` with the other display's name.
- Choosing Extended display or Mirror for the main display hands the main role to another display
  first.
- **Off** writes `disabled = true` alone, without the running values, and hands the main display to
  another enabled one if this was it. It is offered while another display stays on, or when this one is already off.
  Picking anything for a display that is off writes `disabled = false` first.

**Resolution** lists the display's native mode divided by 1, 1.25, 4⁄3, 1.5, 1.6 and 2, wherever
that gives whole pixels, as logical sizes: picking one keeps the native mode and sets `scale` to the
divisor. The native size is marked "(Default)". The panel's mode at the size the display is running
is listed as well, and with **Show all resolutions** every mode the panel reports; a size already in
the list is not repeated. Picking one of these panel modes sets it at scale 1 and its highest refresh
rate.

**Reserved area** keeps a strip along each edge free of windows: Top, Right, Bottom and Left, 0 to
2000 pixels, written together as `reserved_area = { top = …, right = …, bottom = …, left = … }`.

**All displays** holds Hyprland options that apply everywhere:

| Control | Choices | Option |
|---|---|---|
| Auto HDR | Off, HDR, HDR (display profile); switches to HDR while a fullscreen window shows HDR content | `render:cm_auto_hdr`: 0, 1 (default), 2 |
| Variable refresh rate | Off, On, Fullscreen only, Fullscreen games and video; what a display set to follow the global setting does | `misc:vrr`: 0 to 3 |
| Allow tearing | lets a game draw a frame before the display is ready for it, trading a torn line for latency | `general:allow_tearing` |
| Keep X11 apps sharp on scaled displays | draws X11 apps unscaled instead of stretched; they look sharp, and small unless they scale themselves (`GDK_SCALE`, `QT_SCALE_FACTOR`) | `xwayland:force_zero_scaling` |

The page ends with a link row to Night light.

## Color

The Color subpage is titled "Color · *model* (*name*)" for the display it was opened for. Opened from
search, it shows the first display.

| Control | Choices | Writes |
|---|---|---|
| Color profile | Automatic, sRGB, DCI P3, Display P3, Adobe RGB, Wide color, the display's own profile (named after its model), HDR, HDR with the display's own profile | `cm`: `auto`, `srgb`, `dcip3`, `dp3`, `adobe`, `wide`, `edid`, `hdr`, `hdredid`; an HDR profile also sets `bitdepth = 10` |
| Bit depth | 8-bit, 10-bit | `bitdepth` |
| Force wide color | Automatic, On, Off | `supports_wide_color`: 0, 1, −1 |
| Force HDR | Automatic, On, Off | `supports_hdr`: 0, 1, −1 |
| SDR transfer function | Default, Automatic, sRGB, Gamma 2.2, Gamma 2.2 forced | `sdr_eotf`: `default`, `auto`, `srgb`, `gamma22`, `gamma22force` |
| ICC profile | None, or an `.icc` or `.icm` file | `icc` |

:::warning[Force HDR]

Forcing HDR on a display that does not report it can leave the screen black.

:::

The ICC profiles offered are the files found under `~/.local/share/icc`, `~/.color/icc`,
`/usr/local/share/color/icc` and `/usr/share/color/icc`, subfolders included.

**Luminance**:

| Control | Range | Writes |
|---|---|---|
| SDR brightness | 0 to 10, in steps of 0.05; how bright content that is not HDR is drawn while the display is in HDR | `sdrbrightness` |
| SDR saturation | 0 to 10, in steps of 0.05 | `sdrsaturation` |
| SDR minimum luminance | 0 to 1000 | `sdr_min_luminance` |
| SDR maximum luminance | 0 to 10000 | `sdr_max_luminance` |
| Display minimum luminance | −1 to 1000 | `min_luminance` |
| Display maximum luminance | −1 to 10000 | `max_luminance` |
| Display maximum average luminance | −1 to 10000 | `max_avg_luminance` |

The three display figures describe the panel itself; −1 leaves each one to what the display reports.

## Night light

Night light warms the screen through `hyprsunset`; without it a notice says that night light does
nothing. Its settings are keys in `~/.config/proscenio/config.toml`.

| Control | Key | Default |
|---|---|---|
| Automatic schedule | `light.night.automatic` | on |
| From (HH:mm) | `light.night.from` | `19:00` |
| To (HH:mm) | `light.night.to` | `06:30` |
| Color temperature (K) | `light.night.colorTemperature`, 1000 to 6500 in steps of 100 | 5000 |

The From and To fields are inactive while the schedule is off. With the schedule on, night light
turns on at From and off at To. Turning it on runs `hyprsunset -t <temperature>`, or
`hyprctl hyprsunset temperature <temperature>` when `hyprsunset` is already running; turning it off
sets the temperature back to 6000 K. Switching night light by hand, from its quick toggle or the
sidebar's night light dialog, holds until the next From or To. A temperature changed while night
light is on applies at once.
