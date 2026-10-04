---
title: The background
sidebar_label: Background
description: The wallpaper window on each monitor, how the wallpaper is sized and slides with the workspaces, its blur on the lock screen, and the clock and weather widgets drawn over it.
---

# The background

Each monitor gets one window on the bottom layer, under the application windows, that draws the
wallpaper and, over it, the desktop widgets: a clock and a weather card. The wallpaper is a little larger than the
screen and slides as you change workspaces, and the widgets slide with it a little further.

Most of what follows is set on the **Appearance › Background** page of the settings app (see
[Appearance settings](settings-appearance.md)); the keys named here are in
`~/.config/proscenio/config.toml`.

## The window

- There is one background per monitor the shell runs on, which is every monitor unless
  `bar.screenList` names some of them (see [The bar](bar.md)).
- It sits on the bottom layer, reserves no space and takes no input, except over a widget that can
  be dragged.
- `proscenio --no-background` starts the shell without it (see [Command line](command-line.md)).
- A change of `background.wallpaperPath` loads the new wallpaper at once.

**Fullscreen windows.** With `background.hideWhenFullscreen` on (the default, *Hide when a window
is fullscreen*), the background hides while a fullscreen window is on the workspace the monitor
shows. Only real fullscreen counts; a maximized window does not. While the screen is locked the
background always shows.

**Video wallpapers.** A `wallpaperPath` ending in `.mp4`, `.webm`, `.mkv`, `.avi` or `.mov` is
played by `mpvpaper`, one instance per monitor, started when the wallpaper is picked (see
[Wallpaper selector](wallpaper-selector.md)). The background then draws no picture and stays
transparent over the video; the widgets still show.

## Sizing

The wallpaper is scaled to cover the screen, so that neither axis leaves a gap, and then enlarged
by `background.parallax.workspaceZoom` (default `1.07`, shown as *Preferred wallpaper zoom* in
percent). That extra size is the distance the wallpaper can slide: for an image of the screen's
shape, at 1.07 a 2560-pixel-wide screen gets about 180 pixels of travel, whatever the image's
resolution. A wider image travels further. A zoom below 1 leaves the image smaller than the screen,
and it then sits centered and does not move.

The image size is read from the file's header, and the picture is decoded off the main thread
straight to its final size. The window is exactly the size of the scaled wallpaper and is placed
with layer-shell margins, so a slide moves the surface instead of redrawing it.

Each time a wallpaper loads, at start-up or after a change, it appears at a random point of its
travel and glides to its place over one second.

## Parallax

The wallpaper moves along one axis. Its position along that axis is a fraction from 0 (one end) to
1 (the other); the other axis stays centered.

| Setting | Key under `background.parallax` | Default | Effect |
|---|---|---|---|
| Depends on workspace | `enableWorkspace` | on | The fraction follows the active workspace. Off, the wallpaper stays centered. |
| Vertical | `vertical` | off | Slide up and down instead of sideways. |
| Vertical for tall wallpapers | `autoVertical` | off | Slide vertically when the image is taller than it is wide. |
| Depends on sidebars | `enableSidebar` | on | Nudge the wallpaper sideways while the [sidebar](sidebar.md) is open. |
| Preferred wallpaper zoom | `workspaceZoom` | `1.07` | How much larger than the screen the wallpaper is, and so how far it can slide. |
| Widget movement | `widgetsFactor` | `1.2` | How far the widgets slide compared to the wallpaper. |

**The workspace fraction.** Workspaces are counted in groups of `bar.workspaces.shown` (default
10, the number of workspaces the bar shows). Let `last` be the higher of the active workspace and
the highest workspace that holds a window on this monitor; special workspaces do not count. The
wallpaper then spans `ceil(last / shown) × shown` workspaces, and workspace `n` sits at
`(n − 1) / (total − 1)`. With windows only on workspaces 1 to 4, workspace 1 is at the left end
and workspace 10 at the right; once a window exists on workspace 11 or beyond, the span becomes 20
workspaces.

**The sidebar nudge.** While the sidebar is open, `workspaceZoom / shown / 2` is added to the
horizontal fraction: 0.0535 of the travel with the defaults.

The wallpaper slides to a new position over 600 ms with an ease-out cubic curve. It re-places
itself on workspace and monitor focus changes, on windows opening, closing or moving, on a
Hyprland config reload, and when the sidebar opens or closes. Changing a parallax setting, the
fullscreen setting or `bar.workspaces.shown` rebuilds the background windows, so the change shows
at once.

## On the lock screen

When the session locks, the background zooms in by `lock.blur.extraZoom` (default `1.1`) over
400 ms and shows a blurred copy of the wallpaper under a 30% wash of the shell's background color.
Once the zoom settles, the window moves to the overlay layer. The lock screen's own surfaces are
transparent, so this blurred wallpaper, with the clock, is what shows behind the password field.
On unlock the window drops back to the bottom layer and the sharp wallpaper zooms back out.

| Setting | Key | Default |
|---|---|---|
| Enable blur | `lock.blur.enable` | on |
| Extra wallpaper zoom (%) | `lock.blur.extraZoom` | `1.1` |
| Blur radius | `lock.blur.radius` | `100` |

These are on **Privacy & Security › Screen Lock**, under *Style: Blurred* (see
[Power settings](settings-power.md)). With the blur off, the wallpaper stays as it is while locked.
See [The lock screen](lock.md) for the rest of the lock.

The blurred copy is made once per wallpaper and radius, off the main thread, not on every frame:
the image is decoded at a reduced size (at most eight times smaller, depending on the radius),
blurred with three box passes whose combined spread matches a Gaussian, and stretched over the
wallpaper while locked.

## Desktop widgets

The clock and the weather card float over the wallpaper. Each has an **Enable** switch and a
placement:

| Placement | `placementStrategy` | Behavior |
|---|---|---|
| Draggable | `free` | The widget sits at its `x` and `y`. Drag it to move it; it grows to 105% while held, and on release its position is written to `config.toml`. |
| Random | `random` | The widget sits at a random spot at least 200 pixels from every edge, picked again for each new wallpaper. |

The widgets slide with the wallpaper, by `widgetsFactor / workspaceZoom` times the wallpaper's own
offset from center (about 1.12 times as far with the defaults). While the screen is locked they
stop following the wallpaper.

The window takes input only over a widget that can be dragged: one that is enabled, set to
*Draggable*, and not on the lock screen. Everywhere else, clicks go through to the desktop.

### The clock

Keys are under `background.widgets.clock`.

| Setting | Key | Default |
|---|---|---|
| Enable | `enable` | on |
| Placement | `placementStrategy`, `x`, `y` | `random`, 100, 100 |
| Show only when locked | `showOnlyWhenLocked` | off |
| Clock style | `style` | `cookie` |
| Clock style (locked) | `styleLocked` | `cookie` |
| Quote | `quote.enable`, `quote.text` | off, empty |

There are two styles:

- **Cookie**, a 230-pixel dial in the shape of a scalloped cookie, in the primary container color,
  with hands and a date in the styles below. The quote, when set, sits in a bubble under it.
- **Digital**, the time in large type, with the date and the quote under it.

On the lock screen the clock moves to the center of the screen over 500 ms when
`lock.centerClock` is on (the default), and shows a "Locked" badge under it when
`lock.showLockedText` is on (also the default). Both are on the *Screen Lock* page.

The clock follows `time.format` and `time.dateFormat` (see [System settings](settings-system.md)).
It updates once a minute, or every second while the screen is locked, while `time.secondPrecision`
is on, or when the time format shows seconds.

**Cookie options**, under `background.widgets.clock.cookie`:

| Setting | Key | Choices | Default |
|---|---|---|---|
| Sides | `sides` | 0 to 40 | 14 |
| Dial style | `dialNumberStyle` | `none`, `dots`, `full`, `numbers` | `full` |
| Hour hand | `hourHandStyle` | `hide`, `classic`, `hollow`, `fill` | `fill` |
| Minute hand | `minuteHandStyle` | `hide`, `classic`, `thin`, `medium`, `bold` | `medium` |
| Second hand | `secondHandStyle` | `hide`, `classic`, `line`, `dot` | `dot` |
| Date style | `dateStyle` | `hide`, `bubble`, `border`, `rect` | `bubble` |
| Hour marks | `hourMarks` | on or off; drawn only with the `dots` or `full` dial | off |
| Digits in the middle | `timeIndicators` | on or off; not drawn with the `numbers` dial | on |
| Constantly rotate | `constantlyRotate` | on or off | off |
| Use old sine wave cookie implementation | `useSineCookie` | on or off | off |

The second hand shows only while `time.secondPrecision` is on. *Constantly rotate* turns the
cookie one full turn every 30 seconds; it redraws the dial on every frame, so it costs processor
time the whole time the clock is on screen.

**Digital options**, under `background.widgets.clock.digital`:

| Setting | Key | Default |
|---|---|---|
| Vertical | `vertical` | off |
| Animate time change | `animateChange` | on |
| Show date | `showDate` | on |
| Use adaptive alignment | `adaptiveAlignment` | on |
| Font family | `font.family` | Google Sans Flex |
| Font weight | `font.weight` | 350 |
| Font size | `font.size` | 90 |
| Font width | `font.width` | 100 |
| Font roundness | `font.roundness` | 0 |

Vertical puts the hours above the minutes. Adaptive alignment aligns the date and quote to the
left, center or right depending on which third of the screen the clock is in; it does not apply to
the vertical layout or to the centered clock on the lock screen. Width and roundness are
variable-font axes, so only fonts that have them, such as Google Sans Flex, respond to them.

### The weather card

A 200-pixel pill in the primary container color, with the temperature at the top right and the
weather symbol at the bottom left. Its keys are under `background.widgets.weather`: `enable`
(default off), `placementStrategy` (default `free`), and `x` and `y` (default 400 and 100). It fades
out while the screen is locked.

The card shows what the shell's weather service fetches, and the service runs only while
`bar.weather.enable` is on; without it the card shows `--°`. The location and units are set there
too (see [The bar](bar.md)).

## Text color

The digital clock, its date and quote, and the clock's status badges use one text color:

- on the lock screen with the blur on, the shell's ordinary text color;
- otherwise, the hue of the primary color at 80% lightness when the primary color is dark, or at
  12% lightness when it is light.

On the cookie style, the badge and the quote bubble use the colors of their own background
instead.

## Work safety

With **Privacy & Security › Work safety › Hide sussy/anime wallpapers** on
(`workSafety.enable.wallpaper`), the background replaces the wallpaper with a flat color, three
quarters the shell's background color and one quarter primary, when both of these hold:

- the wallpaper's path contains one of `workSafety.triggerCondition.fileKeywords`;
- the name of the current network contains one of `workSafety.triggerCondition.networkNameKeywords`.

The path and the network name are compared in lowercase. While this is in effect, the clock shows a
"Wallpaper safety enforced" badge. Video wallpapers are never replaced. The keyword lists are edited
on the same page (see [System settings](settings-system.md)).
