---
title: Appearance settings
sidebar_label: Appearance
description: The Appearance page and its subpages for colors, fonts, windows, the background, the bar and the panels, with the config key, Hyprland option or file each control writes.
---

# Appearance settings

The **Appearance** page sets the look of the desktop: the toolkit themes, the pointer and the shell's
own title bars. Its subpages go further:

| Subpage | What it covers |
|---|---|
| Background | the wallpaper, parallax, and the clock and weather widgets on the desktop |
| Bar | what the bar shows and where it sits, with the Utility buttons and Workspaces subpages |
| Panels | the on-screen keyboard, with the Dock, Sidebars and Cheat sheet subpages |
| Colors | the palette scheme, the accent, transparency and what gets themed |
| Fonts | the families and sizes for apps and panels |
| Windows | corners, blur, opacity, borders, shadows and dimming of Hyprland's windows |

## Where the values go

Most controls are keys of the shell's config, `~/.config/proscenio/config.toml`. The keys below are
dotted TOML paths, so `bar.autoHide.enable` is the `enable` key of the `[bar.autoHide]` table.

The Hyprland options on the Appearance page and the Windows subpage are read from the running
compositor with `getoption` and written one line each to `~/.config/hypr/settings/appearance.lua`,
such as `hl.config({ decoration = { rounding = 12 } })`. Changes made within 50 ms of each other are
written together, then Hyprland reloads. Hyprland applies the file only if its config loads the
`settings` folder; see [Building and installing](installing.md).

The theme, icon, cursor and font controls write the settings and files each toolkit reads, editing
one key and leaving the rest of each file as it was. They read and write off the main thread, one
write at a time, and every write is followed by a fresh read of all of them.

## The Appearance page

**Desktop** holds link rows to the Background, Bar and Panels subpages, and a section after
**Theme** holds link rows to Colors, Fonts and Windows.

### Theme

| Control | Lists | Writes |
|---|---|---|
| GTK theme | folders with a `gtk-*` subfolder in `~/.themes`, `~/.local/share/themes` and `/usr/share/themes` | gsettings `org.gnome.desktop.interface gtk-theme`, and `gtk-theme-name` in `~/.config/gtk-3.0/settings.ini` and `~/.config/gtk-4.0/settings.ini` |
| Qt style | Fusion, Windows and the style plugins in `/usr/lib/qt6/plugins/styles` and `/usr/lib/qt/plugins/styles` | `widgetStyle` in `~/.config/kdeglobals`, the file Qt apps read their style and fonts from |
| Icon theme | icon themes in `~/.icons`, `~/.local/share/icons` and `/usr/share/icons`, except `hicolor` | gsettings `icon-theme`, `gtk-icon-theme-name` in both `settings.ini` files, `Theme` in the `[Icons]` group of `kdeglobals`, and `hl.env` lines for the Qt icon theme in `appearance.lua` |

The icon theme applies to GTK apps, Qt apps and the shell at once.

### Pointer

**Cursor theme** lists the themes with a `cursors` folder in the same three places as the icon
themes. **Cursor size** runs from 8 to 128 in steps of 4. Changing either one sets both everywhere
the pointer is drawn:

- gsettings `cursor-theme` and `cursor-size`;
- `gtk-cursor-theme-name` and `gtk-cursor-theme-size` in both GTK `settings.ini` files;
- `hl.env` lines for `XCURSOR_THEME`, `XCURSOR_SIZE` and `HYPRCURSOR_SIZE` in `appearance.lua`;
- an `hl.exec_cmd("hyprctl setcursor <theme> <size>")` that runs when Hyprland starts, in
  `appearance.lua`;
- `Inherits` in `~/.icons/default/index.theme`, the fallback cursor theme;
- and `hyprctl setcursor` right away, for the running session.

**Use hyprcursor themes** sets the Hyprland option `cursor:enable_hyprcursor`.

### Shell windows

| Control | Key | Default |
|---|---|---|
| Show title bar | `windows.showTitlebar` | on |
| Center title | `windows.centerTitle` | on, disabled while the title bar is off |

The title bar is the one the shell draws on its own windows, such as the settings window.

## Colors

The palette comes from the wallpaper; [Colors](colors.md) describes how it is generated.

### Palette

The palette type is one of Auto, Content, Expressive, Fidelity, Fruit Salad, Monochrome, Neutral,
Rainbow and Tonal Spot, stored in `appearance.palette.type` as `auto` or `scheme-<name>`
(`scheme-tonal-spot`, for one). The field under it is the accent color, a hex color such as
`#8caaee`, stored in `appearance.palette.accentColor`; empty uses the wallpaper's own color. Either
change regenerates the colors from the current wallpaper by running `proscenio switchwall --noswitch`.

### Shell surfaces

| Control | Key | Default |
|---|---|---|
| Extra background tint | `appearance.extraBackgroundTint` | on |
| Transparency | `appearance.transparency.enable` | off |
| Automatic transparency values | `appearance.transparency.automatic` | on |
| Background (%) | `appearance.transparency.backgroundTransparency` | 11 %, stored as 0.11 |
| Content (%) | `appearance.transparency.contentTransparency` | 57 %, stored as 0.57 |

**Extra background tint** tints the shell's surfaces more strongly with the accent color. With
**Automatic transparency values** on, the two amounts are derived from the wallpaper and both boxes
are disabled. Otherwise **Background** is live while transparency is on, and **Content** is always
live, since it also sets how surfaces are layered when transparency is off.

### Color generation

Under **What gets themed**:

| Control | Key | Default |
|---|---|---|
| Shell & utilities | `appearance.wallpaperTheming.enableAppsAndShell` | on |
| Qt apps | `appearance.wallpaperTheming.enableQtApps` | on |
| Terminal | `appearance.wallpaperTheming.enableTerminal` | on |

With **Shell & utilities** off, a wallpaper change runs neither `matugen` nor the shell's own color
generation, so the Qt and terminal switches only count while it is on. A notice names `matugen` when
it is missing.

**Terminal colors** tune the terminal palette and are ignored while terminal theming is off:

| Control | Key | Default | Range |
|---|---|---|---|
| Force dark mode in terminal | `appearance.wallpaperTheming.terminalGenerationProps.forceDarkMode` | off | |
| Harmony (%) | `…terminalGenerationProps.harmony` | 80 %, stored as 0.8 | 0–100 % |
| Harmonize threshold | `…terminalGenerationProps.harmonizeThreshold` | 100 | 0–100 |
| Foreground boost (%) | `…terminalGenerationProps.termFgBoost` | 35 %, stored as 0.35 | 0–100 % |

## Fonts

### Apps & panels

One row per font role, each with a family and a size from 5 to 72. The family boxes list what
`fc-list` reports; a family that is not installed stays in its box, first. Sizes apply to GTK and Qt
apps; the panels scale their own.

| Role | Writes | Also sets the shell's |
|---|---|---|
| General | `font` in the `[General]` group of `kdeglobals`, gsettings `font-name`, and `gtk-font-name` in both GTK `settings.ini` files | `appearance.fonts.main` and `appearance.fonts.reading` |
| Fixed width | `fixed` in `[General]` of `kdeglobals`, and gsettings `monospace-font-name` | `appearance.fonts.monospace` |
| Titles | `activeFont` in the `[WM]` group of `kdeglobals` | `appearance.fonts.title` |
| Small | `smallestReadableFont` in `[General]` of `kdeglobals` | |
| Toolbar | `toolBarFont` in `[General]` of `kdeglobals` | |
| Menu | `menuFont` in `[General]` of `kdeglobals` | |

Only the family's change is copied to the shell's keys; sizes stay with the toolkits.

### Adjust all

Two switches choose what changes, **Family** (on) and **Size** (off), and the family and size boxes
next to them start from General's. **Apply to all fonts** sets every role except Fixed width, so code
stays monospaced, and a new family is also stored in `appearance.fonts.main`, `appearance.fonts.reading`
and `appearance.fonts.title`.

### Panels only

Faces the shell uses that GTK and Qt have no counterpart for, stored in the config alone:

| Control | Key | Default |
|---|---|---|
| Nerd icons | `appearance.fonts.iconNerd` | JetBrains Mono NF |
| Expressive | `appearance.fonts.expressive` | Space Grotesk |

## Windows

Every control here is a Hyprland option or line in `appearance.lua`.

| Subsection | Control | Hyprland option | Range |
|---|---|---|---|
| Corners | Corner rounding | `decoration:rounding` | 0–40 px |
| Corners | Corner shape | `decoration:rounding_power` | 2.0–10.0; 2 is a circle, higher squares the corner off while keeping it smooth |
| Blur | Blur behind windows | `decoration:blur:enabled` | on or off |
| Blur | Radius | `decoration:blur:size` | 1–40 |
| Blur | Passes | `decoration:blur:passes` | 1–10 |
| Blur | X-ray | `decoration:blur:xray` | on or off; a floating window blurs the wallpaper rather than the windows behind it |
| Opacity | Focused window (%) | `decoration:active_opacity` | 10–100 %, stored as 0.1–1.0 |
| Opacity | Other windows (%) | `decoration:inactive_opacity` | 10–100 % |
| Borders | Border width | `general:border_size` | 0–20 px |
| Shadows | Drop shadows under windows | `decoration:shadow:enabled` | on or off |
| Shadows | Size (px) | `decoration:shadow:range` | 0–100 px |
| Shadows | Falloff | `decoration:shadow:render_power` | 1–4 |
| Shadows | Sharp edge | `decoration:shadow:sharp` | on or off |
| Dimming | Dim windows out of focus | `decoration:dim_inactive` | on or off |
| Dimming | Dim by (%) | `decoration:dim_strength` | 0–100 % |
| Dimming | Dim around the special workspace by (%) | `decoration:dim_special` | 0–100 % |

Radius, passes and X-ray are disabled while blur is off; size, falloff and the sharp edge while
shadows are off; and **Dim by** and **Keep fullscreen windows undimmed** while dimming is off.

### Fullscreen windows

Two switches each own one window rule line in `appearance.lua`. A switch is on while its line is
there; turning it on or off adds or removes the line and reloads Hyprland. Hyprland's
`fullscreen = true` match also covers maximized windows.

| Control | Line |
|---|---|
| Keep fullscreen windows opaque | `hl.window_rule({ name = "opaque-fullscreen", match = { fullscreen = true }, opacity = "1 override 1 override" })` |
| Keep fullscreen windows undimmed | `hl.window_rule({ name = "no-dim-fullscreen", match = { fullscreen = true }, no_dim = true })` |

### Border colors

Each window border, **Focused window** and **Other windows**, has a color and an opacity. The color is
one of Default, Outline, Outline variant, Primary, Secondary or Tertiary, and the opacity runs from
0 to 100 % in steps of 5.

The colors follow the wallpaper. Hyprland's generated colors file, written by a `matugen` template,
defines a Lua table `border_colors` of hex colors without alpha: `active` and `inactive`, the
template's own border colors, and the palette roles `outline`, `outline_variant`, `primary`,
`secondary` and `tertiary`. Default picks `active` for the focused border and `inactive` for the
others. Each border owns one line in `appearance.lua`, for example:

```lua
if border_colors and border_colors.secondary then hl.config({ general = { col = { active_border = "rgba(" .. border_colors.secondary .. "CC)" } } }) end
```

A change to either control rewrites the border's line and reloads Hyprland, so the border keeps the
chosen role and alpha through every wallpaper change, and a colors file without that role leaves the
line inert. Until a border has a line, it keeps what the generated colors file sets, and its opacity
box reads 47 % for the focused border and 20 % for the others.

## Background

See [Background](background.md) for the desktop layer these settings shape.

### Wallpaper

The section shows the current wallpaper and a **Choose file** button, which runs
`proscenio switchwall`: it opens a file picker (`kdialog`) in the Pictures folder and sets the
chosen image as the wallpaper. A notice names `kdialog` when it is missing, and the button is then
disabled.

| Control | Key | Default |
|---|---|---|
| Use system file picker | `wallpaperSelector.useSystemFileDialog` | off |
| Hide when a window is fullscreen | `background.hideWhenFullscreen` | on |

With **Use system file picker** on, the [wallpaper selector](wallpaper-selector.md)'s shortcut opens
the file picker instead of the selector. Hiding the background under a fullscreen window saves a
little work while gaming or watching a video.

### Parallax

| Control | Key | Default | Range |
|---|---|---|---|
| Vertical | `background.parallax.vertical` | off | |
| Vertical for tall wallpapers | `background.parallax.autoVertical` | off | |
| Depends on workspace | `background.parallax.enableWorkspace` | on | |
| Depends on sidebars | `background.parallax.enableSidebar` | on | |
| Preferred wallpaper zoom (%) | `background.parallax.workspaceZoom` | 107 %, stored as 1.07 | 10–200 % |
| Widget movement (%) | `background.parallax.widgetsFactor` | 120 %, stored as 1.2 | 0–300 %, in steps of 10 |

**Vertical for tall wallpapers** pans vertically when the wallpaper is taller than it is wide.
**Widget movement** is how far the clock and weather widgets follow the wallpaper's movement.

### Widget: Clock

| Control | Key | Default |
|---|---|---|
| Enable | `background.widgets.clock.enable` | on |
| Draggable or Random | `background.widgets.clock.placementStrategy` | Random (`random`); Draggable is `free` |
| Show only when locked | `background.widgets.clock.showOnlyWhenLocked` | off |
| Clock style | `background.widgets.clock.style` | Cookie (`cookie`); or Digital (`digital`) |
| Clock style (locked) | `background.widgets.clock.styleLocked` | Cookie |

**Clock style** is hidden while the clock shows only when locked. The digital and cookie settings
below appear while either style uses them.

**Digital clock settings**, under `background.widgets.clock.digital`:

| Control | Key | Default | Range |
|---|---|---|---|
| Vertical | `vertical` | off | |
| Animate time change | `animateChange` | on | |
| Show date | `showDate` | on | |
| Use adaptive alignment | `adaptiveAlignment` | on | |
| Font family | `font.family` | Google Sans Flex | |
| Font weight | `font.weight` | 350 | 1–1000 |
| Font size | `font.size` | 90 | 50–700 |
| Font width | `font.width` | 100 | 25–125 |
| Font roundness | `font.roundness` | 0 | 0–100 |

Adaptive alignment aligns the date and the quote left, center or right by where the clock sits on
the screen. Width and roundness only change fonts that have those axes, such as Google Sans Flex.
The four font values are sliders whose tooltip shows the value.

**Cookie clock settings**, under `background.widgets.clock.cookie`:

| Control | Key | Default | Values |
|---|---|---|---|
| Use old sine wave cookie implementation | `useSineCookie` | off | |
| Sides | `sides` | 14 | 0–40 |
| Constantly rotate | `constantlyRotate` | off | |
| Hour marks | `hourMarks` | off | live only with the Dots or Full dial |
| Digits in the middle | `timeIndicators` | on | not with the Numbers dial |
| Dial style | `dialNumberStyle` | `full` | `none`, `dots`, `full`, `numbers` |
| Hour hand | `hourHandStyle` | `fill` | `hide`, `classic`, `hollow`, `fill` |
| Minute hand | `minuteHandStyle` | `medium` | `hide`, `classic`, `thin`, `medium`, `bold` |
| Second hand | `secondHandStyle` | `dot` | `hide`, `classic`, `line`, `dot` |
| Date style | `dateStyle` | `bubble` | `hide`, `bubble`, `border`, `rect` |

Constant rotation is very expensive to draw. The second hand shows only while clocks show seconds
(`time.secondPrecision`, on [System settings](settings-system.md)), and its choice is disabled
otherwise.

**Quote**: **Enable** (`background.widgets.clock.quote.enable`, off) and the text
(`background.widgets.clock.quote.text`).

### Widget: Weather

**Enable** (`background.widgets.weather.enable`, off) and Draggable or Random
(`background.widgets.weather.placementStrategy`, Draggable by default). The weather source is set on
[System settings](settings-system.md) under Services.

## Bar

See [Bar](bar.md) for the bar itself.

### Notifications

**Unread indicator: show count** (`bar.indicators.notifications.showUnreadCount`, off).

### Positioning

| Control | Key | Default | Values |
|---|---|---|---|
| Bar position | `bar.bottom` and `bar.vertical` | Top | Top, Left, Bottom, Right |
| Automatically hide | `bar.autoHide.enable` | No | No or Yes |
| Reveal bar and workspace numbers | `bar.autoHide.showWhenPressingSuper.enable` | on | |
| Hold delay (ms) | `bar.autoHide.showWhenPressingSuper.delay` | 140 | 0–1000, in steps of 20 |
| Push windows away | `bar.autoHide.pushWindows` | off | |
| Hover region thickness (px) | `bar.autoHide.hoverRegionWidth` | 2 | 1–50 |
| Bar style | `bar.cornerStyle` | Hug (`0`) | Hug `0`, Float `1`, Rect `2` |
| Group style | `bar.borderless` | Pills (`false`) | Pills or Line-separated (`true`) |
| Screen round corner | `appearance.fakeScreenRounding` | When not fullscreen (`2`) | No `0`, Yes `1`, When not fullscreen `2` |

The position sets two keys: Left is `vertical`, Bottom is `bottom`, and Right is both.

**Holding Super** applies whether the bar hides or not: holding the key reveals the bar and the
[workspace](workspaces.md) numbers after the hold delay, which is disabled while revealing is off.
With revealing off, numbers show only when the Workspaces subpage's **Always show numbers** is on.
**Push windows away** keeps the bar's space reserved while it is hidden and is live only while the
bar hides. The hover region is how far past the bar's edge the pointer still counts as touching it,
which is also what reveals a hidden bar. The [screen corners](screen-corners.md) page describes the
rounded corners.

### Appearance

| Control | Key | Default |
|---|---|---|
| Show background | `bar.showBackground` | on |
| Shadow when floating | `bar.floatStyleShadow` | on, live only with the background shown and the Float style |
| Verbose | `bar.verbose` | on |

**Verbose** shows the date next to the clock, and the utility buttons.

**Monitors** has a switch per monitor, labeled with its model and connector name. They write
`bar.screenList`, the connectors that get a bar; an empty list, which is also what turning every
monitor on stores, means all of them. At least one monitor always keeps the bar: turning off the last
one stores nothing and the switch springs back on.

### Resources

**Warning thresholds (%)** for **Memory** (`bar.resources.memoryWarningThreshold`, 95), **Swap**
(`bar.resources.swapWarningThreshold`, 85) and **CPU** (`bar.resources.cpuWarningThreshold`, 90),
from 0 to 100 in steps of 5.

### Tray

| Control | Key | Default |
|---|---|---|
| Make icons pinned by default | `tray.invertPinnedItems` | on |
| Tint icons | `tray.monochromeIcons` | on |
| Hide passive items | `tray.filterPassive` | on |
| Show item IDs in tooltips | `tray.showItemId` | off |
| Pinned items or Unpinned items | `tray.pinnedItems` | empty |

The last field is a comma-separated list of [tray](tray.md) item IDs. While icons are pinned by
default its title reads **Unpinned items** and the list names the items that are not; otherwise it
reads **Pinned items**. Showing item IDs in tooltips is how to find what to type there.

### Weather

**Enable** (`bar.weather.enable`, off).

### Utility buttons

A link row opens this subpage. Each switch shows one button in the bar:

| Control | Key | Default |
|---|---|---|
| Screen snip | `bar.utilButtons.showScreenSnip` | on |
| Color picker | `bar.utilButtons.showColorPicker` | off |
| Keyboard toggle | `bar.utilButtons.showKeyboardToggle` | on |
| Mic toggle | `bar.utilButtons.showMicToggle` | off |
| Dark/Light toggle | `bar.utilButtons.showDarkModeToggle` | on |
| Performance Profile toggle | `bar.utilButtons.showPerformanceProfileToggle` | off |
| Record | `bar.utilButtons.showScreenRecord` | off |
| System updates | `bar.utilButtons.showUpdates` | on |

A notice names whichever of `grim`, `magick`, `wl-copy`, `hyprpicker`, `ydotool`, `wpctl`,
`wf-recorder` and `slurp` are missing, and another says so when power-profiles-daemon
(`net.hadess.PowerProfiles`) is not on the system bus. The System updates button appears once
enough packages are out of date; the threshold is under Services on
[System settings](settings-system.md).

### Workspaces

A link row opens this subpage.

| Control | Key | Default | Range |
|---|---|---|---|
| Always show numbers | `bar.workspaces.alwaysShowNumbers` | off | |
| Show app icons | `bar.workspaces.showAppIcons` | on | |
| Tint app icons | `bar.workspaces.monochromeIcons` | on | |
| Nerd Font for workspace numbers | `bar.workspaces.useNerdFont` | off | |
| Workspaces shown | `bar.workspaces.shown` | 10 | 1–30 |

**Number style** stores the labels themselves in `bar.workspaces.numberMap`: Normal is an empty
list, Han chars is 一 to 二十, and Roman is I to XX.

## Panels

### On-screen keyboard

| Control | Key | Default |
|---|---|---|
| Pinned on startup | `osk.pinnedOnStartup` | off |
| Layout | `osk.layout` | English (US) |

The layouts are those of the [on-screen keyboard](on-screen-keyboard.md): English (US), German and
Russian, stored by name. A notice names `ydotool` when it is missing, since the keyboard types through it.

### Dock

A link row opens this subpage. See [Dock](dock.md).

| Control | Key | Default | Range |
|---|---|---|---|
| Enable | `dock.enable` | off | |
| Hover to reveal | `dock.hoverToReveal` | on | |
| Pinned on startup | `dock.pinnedOnStartup` | off | |
| Hover region height (px) | `dock.hoverRegionHeight` | 2 | 1–50, live only while hovering reveals |
| Tint app icons | `dock.monochromeIcons` | on | |
| Height (px) | `dock.height` | 60 | 30–150, in steps of 5 |
| Ignored apps | `dock.ignoredAppRegexes` | empty | |

Pinned apps are arranged in the [dock](dock.md) itself, by dragging.
**Ignored apps** is a comma-separated list of regular expressions; a window that matches one gets no
dock entry.

### Sidebars

A link row opens this subpage. See [Sidebar](sidebar.md).

| Subsection | Control | Key | Default | Range |
|---|---|---|---|---|
| Quick toggles | Classic or Android | `sidebar.quickToggles.style` | Android (`android`) | |
| Quick toggles | Columns | `sidebar.quickToggles.android.columns` | 5 | 1–8, Android only |
| Sliders | Enable | `sidebar.quickSliders.enable` | off | |
| Sliders | Brightness | `sidebar.quickSliders.showBrightness` | on | |
| Sliders | Volume | `sidebar.quickSliders.showVolume` | on | |
| Sliders | Microphone | `sidebar.quickSliders.showMic` | off | |
| Corner open | Enable | `sidebar.cornerOpen.enable` | on | |
| Corner open | Hover to trigger | `sidebar.cornerOpen.clickless` | off | |
| Corner open | Force hover open at absolute corner | `sidebar.cornerOpen.clicklessCornerEnd` | on | |
| Corner open | with vertical offset | `sidebar.cornerOpen.clicklessCornerVerticalOffset` | 1 | 0–20 |
| Corner open | Place at bottom | `sidebar.cornerOpen.bottom` | off | |
| Corner open | Value scroll | `sidebar.cornerOpen.valueScroll` | on | |
| Corner open | Visualize region | `sidebar.cornerOpen.visualize` | off | |
| Corner open | Region width | `sidebar.cornerOpen.cornerRegionWidth` | 250 | 1–300 |
| Corner open | Region height | `sidebar.cornerOpen.cornerRegionHeight` | 5 | 1–300 |

Which [quick toggles](quick-toggles.md) show, their size and their order are edited in the sidebar
itself, in its edit mode. The slider switches are disabled while sliders are off.

**Corner open** opens the sidebars from the [screen corners](screen-corners.md), by click or by
hover, wherever the bar is. Every control under it is disabled while it is off. When corners open by
click, hovering the corner's very end can still open the sidebar with **Force hover open at absolute
corner**; a non-zero vertical offset makes that end trigger only when the pointer reaches it along
the vertical edge. The two are live only while corners open by click. **Value scroll** turns the rest
of the corner region into a brightness and volume scroll.

### Cheat sheet

A link row opens this subpage. See [Cheat sheet](cheatsheet.md).

| Control | Key | Default | Range |
|---|---|---|---|
| Super key symbol | `cheatsheet.superKey` | | one of 19 Nerd Font glyphs |
| Use macOS-like symbols for mods keys | `cheatsheet.useMacSymbol` | off | |
| Use symbols for function keys | `cheatsheet.useFnSymbol` | off | |
| Use symbols for mouse | `cheatsheet.useMouseSymbol` | off | |
| Split buttons | `cheatsheet.splitButtons` | off | |
| Keybind font size | `cheatsheet.fontSize.key` | 12 | 8–30 |
| Description font size | `cheatsheet.fontSize.comment` | 12 | 8–30 |

`cheatsheet.superKey` can also hold any other text, written in the config by hand. **Split buttons**
draws the modifiers and the key as separate keycaps, "Ctrl + A" rather than "Ctrl A".
