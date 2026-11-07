---
title: Title bars
sidebar_label: Title bars
description: The Hyprland plugin that draws a Material 3 title bar on floating or all windows that ask for one, its buttons, the hyprpm install, every option and the rules that turn a bar on or off.
---

# Title bars

`proscenio_bars` is a Hyprland plugin that draws a title bar above a window that asks for one: a bar
in the Material 3 palette with the window's title and three round buttons. It lives in `plugin/` and
is built and loaded by `hyprpm`, apart from the shell.

## Which windows get a bar

A window gets a bar when it is placed where bars are shown, has its `decorate` rule on and asks for a
bar. The `windows` option sets the placement:

| `windows` | Windows that can get a bar |
|---|---|
| `floating` (default) | Floating windows |
| `all` | Floating and tiled windows |

A config reload applies the option to the open windows, and a window that floats or tiles is checked
again at once. Hyprland tells
every client that the compositor draws the frame and keeps what each client requested; the plugin
reads those requests.

| Window | Request | Bar |
|---|---|---|
| Wayland, with an `xdg-decoration` object | Server side, or no mode set | Yes |
| Wayland, with an `xdg-decoration` object | Client side | No |
| Wayland, with an `org_kde_kwin_server_decoration` object | Server | Yes |
| Wayland, with an `org_kde_kwin_server_decoration` object | Client, or none | No |
| Wayland, with neither object | The window draws its own frame | No |
| X11 | Not read | Yes |
| X11 that overrides redirection (menus and tooltips) | Not read | No |

A window with both objects gets a bar when either one asks for it. The plugin reads the requests
when the window maps and whenever Hyprland updates the window's rules, such as on a change of focus
or title.

The plugin has built-in rules for a few windows, matched on the class and title Hyprland's window
rules see:

| Window | Built-in result |
|---|---|
| Empty class and empty title | No bar |
| Class `dev.fEst.Proscenio`, the shell's own windows | No bar |
| Class `steam` | No bar |
| Class starting with `hyprland-`, such as `hyprland-dialog` | A bar, as with `force_bar` |

Two window rule effects change the result:

| Effect | Result when set to a true value |
|---|---|
| `proscenio_bars:no_bar` | No bar |
| `proscenio_bars:force_bar` | A bar on a window that did not ask for one or that a built-in rule leaves without one |

`no_bar` wins over `force_bar` and over every built-in rule, and `force_bar` wins over a built-in
no bar. A tiled window while `windows` is `floating`, a window with the `decorate` rule off and a
window that overrides redirection get no bar with any of them.

```lua
hl.window_rule({ match = { class = "^(steam)$" }, ["proscenio_bars:force_bar"] = true })
hl.window_rule({ match = { class = "^(hyprland-dialog)$" }, ["proscenio_bars:no_bar"] = true })
```

### Listing the windows with a bar

The plugin adds the `hyprctl` command `proscenio_bars`. It replies with a JSON array of the addresses
of the mapped windows that show a bar, in the form `hyprctl clients` uses. A fullscreen window is not
among them, since its bar is not drawn.

```bash
$ hyprctl proscenio_bars
["0x55d0c8a1b2e0","0x55d0c8f03a10"]
```

## The bar

The bar sits on the top edge of the window and adds its height (`bar_height`) to the window's
extents. A floating window keeps its size and the bar sits above it. A tiled window's tile keeps its
place, gaps and border, and the bar takes its height from the top of the window inside it, in every
tiled layout. The title fills the space the buttons leave.

### Buttons

The buttons are 16 px discs, 8 px apart and 16 px from the bar's edge. The order depends on
`button_side`:

| `button_side` | Order from the left |
|---|---|
| `right` (default) | Minimize, Maximize, Close |
| `left` | Close, Minimize, Maximize |

A button that does not fit in a narrow bar is left out, last one first.

| Button | Icon | Action |
|---|---|---|
| Close | Cross | Closes the window |
| Minimize | Chevron pointing down | Moves the window to `special:minimized` without showing that workspace |
| Maximize | Chevron pointing up, or a diamond while the window is maximized | Toggles Hyprland's maximized state |

Hyprland has no minimized state. The [dock](dock.md) shows a window on `special:minimized` as
minimized, and focusing it there moves it back to the active workspace.

### States

- **Focused:** the bar and the title use `bar_color` and `title_color`, and the discs use their role
  colors.
- **Unfocused:** the bar uses `inactive_bar_color`, the title `inactive_title_color` and the discs
  `inactive_button_color`.
- **Pointer over the buttons:** all three discs take their role colors and show their icons in the
  matching icon color, whether or not the window has the focus. The icons are hidden otherwise.
- **Pointer over one button:** that disc gets a state layer of 8 % in its icon color.
- **Pressed:** the state layer is 10 %. A button fires on release, only while the pointer is still on
  the button it was pressed on.

The colors fade in and out. A window with the `no_anim` rule changes at once.

The icons are strokes with round caps and joins, drawn at the disc's size in device pixels. The
stroke is 2 px wide on a 16 px disc and scales with it.

### Pointer

- A left press on the bar focuses and raises the window. Presses with a modifier held pass through
  to the window.
- Pressing and moving the pointer more than 4 px starts a drag that moves the window. Dragging a
  maximized floating window restores it first and keeps it under the pointer.
- Dragging a tiled window works as a `SUPER` drag does: the window floats at a smaller size while it
  moves, with the bar under the pointer at the point it was grabbed, and tiles again where it is
  dropped.
- Two left clicks within 400 ms on the bar, away from the buttons, toggle maximized.
- The bar does not react while the window is fullscreen.

## Installing

The plugin is built against the headers of the running Hyprland, so `hyprpm` installs it:

```bash
hyprpm update
hyprpm add https://github.com/The1fEst/proscenio
hyprpm enable proscenio_bars
```

`hyprpm reload` loads the enabled plugins. Run `hyprpm update` after every Hyprland update, because
a plugin built against other headers is refused. The repository's `hyprpm.toml` names the plugin and
its build command, `make -C plugin`, which needs `pkg-config` entries for `pixman-1`, `libdrm`,
`hyprland`, `libinput`, `libudev`, `wayland-server`, `xkbcommon` and `pangocairo`.

## Options

Every option lives under `plugin:proscenio_bars:`.

| Option | Default | Sets |
|---|---|---|
| `bar_height` | 40 | The bar's height in px, at least 1 |
| `bar_color` | `#211F26` | The bar on a focused window |
| `inactive_bar_color` | `#211F26` | The bar on an unfocused window |
| `title_color` | `#E6E0E9` | The title on a focused window |
| `inactive_title_color` | `#CAC4D0` | The title on an unfocused window |
| `title_font` | `Google Sans` | The title's font family |
| `title_size` | 14 | The title's font size, at least 1 |
| `title_weight` | 500 | The title's weight, a number from 100 to 1000 or a name such as `bold` |
| `windows` | `floating` | `floating` or `all`, the [windows that can get a bar](#which-windows-get-a-bar) |
| `button_side` | `right` | `right` or `left` |
| `close_color` | `#F2B8B5` | The Close disc |
| `close_icon_color` | `#601410` | The Close icon and its state layer |
| `minimize_color` | `#EFB8C8` | The Minimize disc |
| `minimize_icon_color` | `#492532` | The Minimize icon and its state layer |
| `maximize_color` | `#D0BCFF` | The Maximize disc |
| `maximize_icon_color` | `#381E72` | The Maximize icon and its state layer |
| `inactive_button_color` | `#36343B` | All three discs on an unfocused window |

The [region selector](region-selector.md#picking-a-window) reads `bar_height` and the
[window list](#listing-the-windows-with-a-bar) to take a window together with its bar. **Appearance › Windows › Title bars** in the
[settings](settings-appearance.md#windows) sets `windows` and `button_side`.

### Setting them

Hyprland raises a config error for an option of a plugin that is not loaded, and `hyprpm` loads the
plugin after the config's first pass, then reads the config again. A line that sets a
`proscenio_bars` option therefore checks the loaded plugins first. `hl.plugin.proscenio_bars` does
not exist, since the plugin registers no Lua functions; the check walks the list instead:

```lua
for _, plugin in ipairs(hl.get_loaded_plugins()) do
    if plugin.name == "proscenio_bars" then
        hl.config({ plugin = { proscenio_bars = { bar_height = 36, button_side = "left" } } })
    end
end
```

### The bar's color

With `appearance.wallpaperTheming.enableQtApps` on, a [color run](colors.md#gtk-and-qt) sets
`bar_color` and `inactive_bar_color` to the `BackgroundNormal` of `Colors:Header` in the Qt scheme of
the current mode, so that a bar and a Qt app's toolbar have one color. The two options go in
[one guarded line](hyprland.md#title-bar-color) in `settings/appearance.lua`, which overrides what the
Hyprland config set before it. With `enableQtApps` off, a color run removes the line, and the bar
keeps the colors the rest of the config gives it.
