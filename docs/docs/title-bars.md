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

A window with both objects gets a bar when either one asks for it.

A Wayland window whose process has `libgtk-3.so` or `libgtk-4.so` mapped counts as a GTK window;
libadwaita apps are GTK 4 apps. The plugin reads `/proc/<pid>/maps` once, when the window maps, and
a window whose entry it cannot read is not a GTK window. A Flatpak app's window carries the pid of
the app's own process, so it is read the same way. While `windows` is `floating`, a floating GTK
window gets a bar whatever it asks for, including one that draws its own header bar. While
`windows` is `all`, a GTK window follows its request as any other window does, so a GTK window
with a header bar gets no bar there. X11 windows are not checked. The plugin reads the requests
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
| A floating GTK window while `windows` is `floating` | A bar, as with `force_bar` |

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
| Maximize | Chevron pointing up, or a diamond while the window is maximized | On a floating window or a maximized one, toggles Hyprland's maximized state; on any other tiled window, [toggles monocle](#monocle) on the window's workspace |

A tiled window also counts as maximized while its workspace's tiled layout is `monocle`.

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
- Two left clicks within 400 ms on the bar, away from the buttons, do what Maximize does.
- The bar does not react while the window is fullscreen.

### Monocle

The plugin adds the Lua function `hl.plugin.proscenio_bars.toggle_monocle(workspace)`. It switches a
workspace between its tiled layout and Hyprland's `monocle` layout, in which every tiled window fills
the workspace and only the focused one shows. `workspace` is a workspace object or selector, such as
`3` or `"special:magic"`; without it, the function takes the special workspace open on the focused
monitor, or else that monitor's active workspace.

The plugin keeps one workspace rule with `layout = "monocle"` per workspace it toggled, apart from the
rules of the config. The first toggle of a workspace adds the rule, on unless the workspace is in
`monocle` already, and every later toggle turns it on or off. The toggle cannot leave a `monocle`
layout that the config itself sets. A config reload drops the rules, so every workspace goes back to
the layout its config gives it, and unloading the plugin turns them off.

Leaving `monocle` through the toggle puts the tiled windows back where they were before it:

| Layout | Restored |
|---|---|
| `dwindle` | The split tree: which windows share each split, its direction and ratio |
| `master` | The order of the windows, which ones are masters, the master size and each window's size share |
| `scrolling` | The columns in order, the windows in each column, column widths and window heights |

A window closed during `monocle` leaves the arrangement: in `dwindle` its sibling takes the space of
their split, in `master` the first window left becomes the master when no master is left, and in
`scrolling` the rest of its column share its height. A window opened during `monocle` comes after
the others: in `dwindle` it splits the last window, in `master` it joins the end of the stack, and in
`scrolling` it gets a column of the default width at the end. After a config reload, or when the
layout differs from the one before `monocle`, the layout arranges the windows itself.

`hl.plugin.proscenio_bars` exists only while the plugin is loaded, so a bind can fall back to a toggle
of its own:

```lua
hl.bind("SUPER + F", function()
    if hl.plugin.proscenio_bars then
        hl.plugin.proscenio_bars.toggle_monocle()
    else
        my_monocle_toggle()
    end
end)
```

From a shell, `hyprctl eval 'hl.plugin.proscenio_bars.toggle_monocle()'` toggles the active workspace.

### GTK window buttons

GTK apps read the buttons of their own header bar from `button-layout` in
`org.gnome.desktop.wm.preferences`. The shell sets it when it starts and whenever **Show on** or
**Button side** changes in the [settings](settings-appearance.md#windows):

| Title bars | `button-layout` |
|---|---|
| `windows` is `floating` | `:`, no buttons, since floating GTK windows get a bar and GTK cannot show buttons on tiled windows alone |
| `windows` is `all`, buttons on the right | `appmenu:minimize,maximize,close` |
| `windows` is `all`, buttons on the left | `close,minimize,maximize:appmenu` |
| The plugin not enabled in `hyprpm` | The value for `all`, on the button side set |

With `windows` at `floating`, a tiled GTK window with its own header bar has no buttons.

## Installing

The plugin is built against the headers of the running Hyprland, so `hyprpm` installs it:

```bash
sudo pacman -S --needed hyprpm git gcc make pkgconf
hyprpm update
hyprpm add https://github.com/The1fEst/proscenio
hyprpm enable proscenio_bars
```

The `hyprpm` package brings `cmake`, `cpio`, `meson`, `glaze`, `hyprwayland-scanner` and the
Hyprland headers' other build needs; `git` fetches the plugin and the Hyprland source, and `gcc`,
`make` and `pkgconf` build both. Until `proscenio_bars` is enabled in `hyprpm`,
**Appearance › Windows › Title bars** in the [settings](settings-appearance.md#windows) shows these
commands with a copy button each in place of its options; once it is enabled, it shows the options
and `hyprpm update`.

The repository's `hyprpm.toml` names the plugin and its build command, `make -C plugin`, which needs
`pkg-config` entries for `pixman-1`, `libdrm`, `hyprland`, `libinput`, `libudev`, `wayland-server`,
`xkbcommon` and `pangocairo`.

### Rebuilding after a Hyprland update

`hyprpm` refuses a plugin built against other headers, so the plugin has to be rebuilt after every
update of Hyprland or of the libraries in its ABI. The shell checks this when it starts and after
every system update from [Updates](settings-system.md#updates) that succeeds, if the `state.toml`
of a repository under `/var/cache/hyprpm/<user>/` has `proscenio_bars` enabled. The built hash is
`hash` under `[state]` in `/var/cache/hyprpm/<user>/state.toml`, and counts as missing while
`proscenio_bars` is marked `failed`; the installed one is `abiHash` of `Hyprland --version-json`,
the running one `abiHash` of `hyprctl version -j`.

| Check | Result |
|---|---|
| The plugin is not loaded and the built hash equals the running one | `hyprpm reload` |
| The built hash differs from the installed one | `hyprpm update`, the first time in the session for this installed hash; the failure notification below after that |
| The built hash equals the installed one, the plugin is not loaded, and the running hash is unknown | `hyprpm reload` |
| Anything else | Nothing |

The check needs neither a password nor the network. A rebuild asks for the password once, in the
shell's [polkit prompt](polkit.md) for the action `dev.fEst.Proscenio.hyprpm`, with the message
"Authentication is required to rebuild the window title bars plugin after a Hyprland update":
`pkexec /usr/lib/proscenio/proscenio-packages hyprpm` opens a root session that lasts for the
rebuild. Then `hyprpm update` runs as the user with `PATH` set to `~/.cache/proscenio/hyprpm-tools`
alone, a directory made again before each rebuild with a symbolic link to every program in
`/usr/bin` except `sudo`, `doas` and `run0`, and a link named `run0` to `proscenio-packages`.
`hyprpm` takes the first of `sudo`, `doas` and `run0` it finds in `PATH`, so each step it runs as
root goes to `proscenio-packages`, which passes it to the session over the same token-guarded
abstract socket a system update uses: the session takes it only from a process of the same user
whose parent is this `hyprpm update`, with the session's random token, and closes the socket when
the rebuild ends. `hyprpm` runs a plugin's build step through `/bin/sh -c`, a child of its own with
the token in its environment, so a step that runs `run0` itself in that shell can ask for the same
root steps, and only for those in the table below; a process the build starts below that shell is
refused. Without `pkexec` or `proscenio-packages`,
or with `XDG_RUNTIME_DIR` other than `/run/user/<uid>`, no rebuild starts. A check asked for while a
rebuild runs follows it.

The session runs only the root steps of `hyprpm update`, with `<user>` the name of the account
that opened it and `<uid>` its uid, and refuses every other command:

| Step from `hyprpm` | What the session does |
|---|---|
| `echo hyprland` | Nothing |
| `mkdir -p -m 755` `/var/cache/hyprpm`, `/var/cache/hyprpm/<user>`, `…/<user>/headersRoot` or `…/<user>/<repository>` | Creates the missing directories, mode 755, refusing a path through a symbolic link |
| `rm -fr /var/cache/hyprpm/<user>/<repository>` | Removes that directory; refuses a file there |
| `install -m644 -o 0 -g 0 /run/user/<uid>/hyprpm/.temp-state` to `…/<user>/state.toml` or `…/<user>/<repository>/state.toml` | Writes the file, mode 644 |
| `install -m0755 -o 0 -g 0 /run/user/<uid>/hyprpm/<user>/…` to `…/<user>/<repository>/<name>.so` | Writes the file, mode 755 |
| `/bin/sh -c` with hyprpm's exact `make -C '…/hyprland-<user>' installheaders && chmod … && find …` | Runs `make installheaders` as the user into `/run/user/<uid>/hyprpm/hyprland-<user>/proscenio-headers`, then copies `include/hyprland/` and `share/pkgconfig/hyprland.pc` from there into `…/<user>/headersRoot`, files 644 and directories 755 |

A repository name is letters, digits, `.`, `_` and `-`, not starting with `.`, and neither
`headersRoot` nor `state.toml`. Every file the
session copies is read by `proscenio-packages export` running as the user, so a symbolic link in the
user's directories reaches only what the user can read; the session refuses entries that are not
regular files or directories. `make installheaders` does not copy `hyprland.pc` to
`/usr/share/pkgconfig` in this way. The session reads at most 64 MiB from a file or the headers'
stage and passes on the first 4 MiB of `make`'s output. It accepts the stage only when every entry
sits in a directory the stage also lists, writes it into `…/<user>/headersRoot/.proscenio-new`, and
only when that succeeds replaces `include/hyprland` and `share/pkgconfig/hyprland.pc` with it.

Once the password is accepted, a notification titled **Rebuilding the title bars** shows the last line of
`hyprpm`'s output or the session's, at most twice a second. When it ends:

| Result | What follows |
|---|---|
| `state.toml` has the installed `abiHash`, the same Hyprland runs | `hyprpm reload`, `hyprctl dismissnotify`, and **Title bars rebuilt** |
| `state.toml` has the installed `abiHash`, an older Hyprland runs | `hyprctl dismissnotify`, and **Title bars rebuilt** with **Hyprland was updated. The title bars come back after you log in again.** |
| Anything else | **Could not rebuild the title bars**, with **Run hyprpm update in a terminal to rebuild them.** below the reason: the session's last error, **The authentication was canceled or failed.** when the session never started, **A rebuild needs pkexec, … and XDG_RUNTIME_DIR set to …** when it could not start, or else the last output line |

`hyprpm update` builds against the installed Hyprland, `Hyprland --version-json`, while the session
keeps the Hyprland it started with, so after a system update the rebuilt plugin loads at the next
login. The failure notification shows once per installed hash in a session.

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
`proscenio_bars` option therefore checks the loaded plugins first:

```lua
for _, plugin in ipairs(hl.get_loaded_plugins()) do
    if plugin.name == "proscenio_bars" then
        hl.config({ plugin = { proscenio_bars = { bar_height = 36, button_side = "left" } } })
    end
end
```

### The bar's colors

A [color run](colors.md#gtk-and-qt) sets every color option from the palette the shell computes from
the source color in `color.txt`, the scheme type and the current mode:

| Options | Role |
|---|---|
| `bar_color`, `inactive_bar_color` | The `BackgroundNormal` of `Colors:Header` in the Qt scheme with `appearance.wallpaperTheming.enableQtApps` on, so that a bar and a Qt app's toolbar have one color; `surface_container` with it off |
| `title_color` | `on_surface` |
| `inactive_title_color` | `on_surface_variant` |
| `inactive_button_color` | `surface_container_highest` |
| `close_color`, `close_icon_color` | `error`, `on_error` |
| `minimize_color`, `minimize_icon_color` | `tertiary`, `on_tertiary` |
| `maximize_color`, `maximize_icon_color` | `primary`, `on_primary` |

The options go in [one guarded line](hyprland.md#title-bar-colors) in `settings/appearance.lua`, which
overrides what the Hyprland config set before it. The shell also writes the line when it starts, from
`color.txt` and the scheme type in `scheme.txt`, so that the bars match the palette before the next
color run. A run or a start that changes the line reloads Hyprland. With
`appearance.wallpaperTheming.enableAppsAndShell` off, the shell does not write the line at start.
