---
title: The settings window
sidebar_label: Settings
description: How the settings window is laid out, how to move around and search it, how its controls behave and where they store values, and its Quick and Search pages.
---

# The settings window

The settings window configures the shell and the parts of Hyprland and the system it runs on. It is
an ordinary Hyprland window, 1100 × 750 when it opens and no smaller than 750 × 500, titled
`proscenio Settings` for window rules to match. There is only one: opening it while it is already
open brings it forward and focuses it on its workspace.

![The settings window on the Appearance page, in dark mode](/img/screenshots/settings-dark.webp)

![The settings window on the Appearance page, in light mode](/img/screenshots/settings-light.webp)

## Opening it

| From | Opens |
|---|---|
| The gear in the [sidebar](sidebar.md) | the window, on the Quick page unless it is already open |
| The sidebar's Wi-Fi dialog, **Details** | Wi-Fi, or Network while the connection is wired |
| The sidebar's Bluetooth dialog, **Details** | Bluetooth |
| The sidebar's WireGuard dialog, **New Connection** | Network |
| The secondary action of a [quick toggle](quick-toggles.md) | Network for Ethernet, Volume Levels for audio and the microphone |
| `proscenio ipc call settings open` | the window |
| `proscenio ipc call settings openPage <id>` | the page or subpage with that id, from the table below |
| `proscenio ipc call settings close`, `toggle` | closes it, or opens it when it is closed |

The IPC target is described with the rest on [IPC](ipc.md).

## The window

A title bar runs across the top: "Settings", centered, or at the left with `windows.centerTitle`
off, and hidden altogether with `windows.showTitlebar` off. On its right are three round buttons:

| Button | Does |
|---|---|
| Highlight changed settings | turns the changed-value markers on or off (`settings.highlightChanged`) |
| Reset this page to defaults | asks first, then puts the current page's shell settings back to their defaults |
| Close | closes the window |

Below it the navigation rail sits on the left and the current page fills the rest.

| Keys | Do |
|---|---|
| Escape | closes an open dialog, otherwise the window |
| Ctrl+Page Down, Ctrl+Page Up | the next or previous rail page, stopping at the first and last |
| Ctrl+Tab, Ctrl+Shift+Tab | the next or previous rail page, wrapping around |
| A letter, digit or symbol | expands the rail and types into its search field, while no text field has focus and no dialog is open |

## The navigation rail

From the top:

- **The toggle** expands and collapses the rail. The rail starts expanded when the window is wider
  than 900 px and follows the window's width until the toggle is first pressed; from then on only
  the toggle changes it. Collapsed, the rail shows icons only and each one's name as a tooltip.
- **Config file** opens `~/.config/proscenio/config.toml` with `xdg-open`. A right click copies the
  file's path instead.
- **The search field**, shown while the rail is expanded, finds pages and settings (see below).
- **The pages**, in groups divided by a thin line: Quick; Wi-Fi, Network, Bluetooth; Displays, Sound,
  Power, Multitasking, Appearance; Apps, Notifications, Search; Mouse & Touchpad, Keyboard, Devices;
  Accessibility, Privacy & Security, System. A press selects a page, and the list scrolls to keep the
  current one in view.

### Finding a setting

While the search field holds text, the page list gives way to results: pages, found by name and by
keywords (searching "volume" finds Sound), subpages, and every section, subsection and control by its
title. Each result shows where it lives below its title, such as `Screen Lock › Style: Blurred`.

- Every word typed has to appear in the title, in that trail, or in a page's keywords.
- Titles that start with the query come first, then titles that hold every word, then the rest.
  Within each group pages come before settings, and a title that is exactly the query comes before
  longer ones.
- Titles match in English and in every bundled translation, and results show in the interface
  language, so a word in any of those languages finds the setting.
- Up to 60 results show; "No settings found" stands in for none.

Pressing a result, or Enter for the first one, opens its page, scrolls the setting a third of the way
down and tints it for a moment. Collapsing the rail clears the search.

The list of sections and controls is collected from the page sources in
`src/panels/settings/pages/` when the shell is built (`build.rs`), so search finds settings on pages
that have not been opened.

## The pages

Each page is built when it is shown and dropped when another one is. A page can open a subpage, which
takes its place under a header with a back button and the subpage's name; subpages can open further
subpages. Back walks the way that was taken, one page at a time, down to the rail's page, and choosing
a page on the rail starts over. A subpage opened from search or IPC gets its parent pages on the way
back.

| Rail page | Id | Subpages (id) |
|---|---|---|
| Quick | `quick` | — |
| [Wi-Fi](settings-network.md) | `wifi` | Saved Networks (`savednetworks`), Connect to Hidden Network… (`hiddennetwork`), Hotspot (`hotspot`) |
| [Network](settings-network.md) | `network` | Connection (`connection`), which has IPv4 (`ipv4`), IPv6 (`ipv6`) and Authentication (`eap`); Proxy (`proxy`); Firewall (`firewall`) |
| [Bluetooth](settings-devices.md) | `bluetooth` | — |
| [Displays](settings-displays.md) | `displays` | Color (`displaycolor`), Night light (`nightlight`) |
| [Sound](settings-sound.md) | `sound` | Volume Levels (`volumelevels`), Sound cards (`soundcards`) |
| [Power](settings-power.md) | `power` | — |
| [Multitasking](settings-multitasking.md) | `multitasking` | Overview (`overview`), Swiping between workspaces (`swiping`) |
| [Appearance](settings-appearance.md) | `appearance` | Background (`background`), Colors (`colors`), Fonts (`fonts`), Windows (`windows`), Bar (`bar`) with Utility buttons (`utilitybuttons`) and Workspaces (`barworkspaces`), Panels (`panels`) with Dock (`dock`), Sidebars (`sidebars`) and Cheat sheet (`cheatsheet`) |
| [Apps](settings-apps.md) | `apps` | File types (`filetypes`), Window rules (`windowrules`) |
| [Notifications](settings-system.md) | `notifications` | — |
| Search | `search` | — |
| [Mouse & Touchpad](settings-input.md) | `mouse` | This mouse only (`mousedevice`), Touchpad (`touchpad`) |
| [Keyboard](settings-input.md) | `keyboard` | Keyboard Shortcuts (`shortcuts`), Keyboard options (`keyoptions`) |
| [Devices](settings-devices.md) | `devices` | — |
| [Accessibility](settings-input.md) | `accessibility` | — |
| [Privacy & Security](settings-system.md) | `privacy` | [Screen Lock](settings-power.md) (`lock`), Screenshots & Recording (`capture`) |
| [System](settings-system.md) | `system` | Region & Language (`region`), Date & Time (`datetime`), Users (`users`), Autostart (`autostart`), About (`about`), Services (`services`), Advanced (`advanced`) |

Quick and Search are described at the end of this page.

## How the controls behave

Most controls apply a change the moment it is made; there is no Apply button.

| Control | Behavior |
|---|---|
| Switch | applies on a click, then shows the value that was actually stored, so a change the target refuses springs back |
| Spin box | − and + step the value, holding one repeats, and the number in the middle can be typed |
| Slider | applies while it moves; its tooltip shows the value |
| Selection buttons | a row of buttons with one current; a press applies |
| Drop-down list | picking an entry applies it |
| Text field | applies on Enter or when the field loses focus; a field holding a list splits it at commas |
| Link row | a row with a chevron at its end, which opens a subpage |

A subsection title followed by an info symbol explains itself when the symbol is hovered, and
tooltips on rows appear at once. A row that only matters while another setting is on, such as a
timeout next to its switch, is dimmed and inactive while that setting is off.

Two editors work differently: the [connection editor](settings-network.md) collects changes into a
draft that **Save** writes, and the [proxy settings](settings-network.md) are written once the
changes pause.

## Where values are stored

| Store | What it holds | How a change takes effect |
|---|---|---|
| `~/.config/proscenio/config.toml` | the shell's own settings | written at once; the shell watches the file and applies it, and edits made by hand show in the open window |
| `~/.config/hypr/settings/<area>.lua` | Hyprland options, monitor rules, per-device input settings, window rules and shortcuts | written, then Hyprland reloads; the window reads the values back from Hyprland |
| `~/.config/hypr/hypridle.conf` | idle timeouts, lock before sleep, idle inhibition | written, then `systemctl --user restart hypridle.service` |
| system services | networks, Bluetooth, sound devices, power buttons, the firewall, the system proxy | through D-Bus or the service's own tool; changes that need root run `pkexec proscenio …` ([polkit](polkit.md)) |

**`config.toml`.** A setting is named on these pages by its key path: `search.prefix.app` is the
key `app` in the `[search.prefix]` table. Storing a value changes only that key and keeps the rest
of the file, comments included, and the file is replaced in one step, never left half written.

**Hyprland's files.** There is one file per area: `appearance.lua`, `displays.lua`,
`multitasking.lua`, `keyboard.lua`, `accessibility.lua`, `mouse.lua`, `devices.lua`, `apps.lua`,
`binds.lua` and `other.lua`. An option is one line, such as
`hl.config({ general = { allow_tearing = true } })`; a page reads the current value with Hyprland's
`getoption`. Option changes made within 50 ms of each other are written together, followed by one
reload. Hyprland applies these files only if its config loads them; [Building and
installing](installing.md) shows how.

### Changed settings and resetting a page

While **Highlight changed settings** is on, every row whose `config.toml` value differs from its
default carries a tinted background and a bar along its left edge. A key missing from the file counts
as its default, and numbers compare as numbers. The markers follow the file as it changes. Rows that
hold a Hyprland option, a system setting or another program's file are never marked.

**Reset this page to defaults** counts the page's settings that the markers would show, whether they
are on or not, and asks before writing each one's default back to `config.toml`. Settings kept by
Hyprland or the system stay as they are.

### Missing programs and services

Where a control depends on an optional program that is not on `PATH`, the page shows a notice naming
the program, what does not happen without it and the package it comes in. Services are looked up on
the system bus, running or ready to start on demand. A page that cannot work at all without its
service shows only the notice; a button that needs a missing program is disabled, and its tooltip
says why. [Building and installing](installing.md) lists every such program.

## Quick

The first page gathers the settings changed most often.

**Wallpaper & Colors**

| Control | Does |
|---|---|
| Wallpaper preview | shows `background.wallpaperPath` |
| Choose file | runs `proscenio switchwall`, which asks `kdialog` for an image, starting in the Pictures folder, and makes it the wallpaper; disabled without `kdialog` |
| Light, Dark | run `proscenio switchwall --mode light` or `--mode dark` with `--noswitch`, which regenerates the colors in that mode without changing the wallpaper; the current mode is highlighted |
| Palette | the color scheme generated from the wallpaper: Auto, Content, Expressive, Fidelity, Fruit Salad, Monochrome, Neutral, Rainbow or Tonal Spot (`appearance.palette.type`); a change regenerates the colors |
| Accent color | a color such as `#8caaee` to generate from instead of the wallpaper's; empty uses the wallpaper (`appearance.palette.accentColor`) |
| Extra background tint | tints shell surfaces more strongly with the accent color (`appearance.extraBackgroundTint`, on) |
| Transparency | makes shell surfaces translucent (`appearance.transparency.enable`, off) |
| Automatic transparency values | derives the two amounts below from the wallpaper (`appearance.transparency.automatic`, on) |
| Background (%) | `appearance.transparency.backgroundTransparency`, 11 %; editable while transparency is on and the automatic values are off |
| Content (%) | `appearance.transparency.contentTransparency`, 57 %; editable while the automatic values are off, and it affects how surfaces are layered even with transparency off |

The palette and transparency controls are the same ones the Colors subpage of
[Appearance](settings-appearance.md) shows; [Colors](colors.md) explains how the scheme is made.

**Bar & screen**

| Control | Choices | Key |
|---|---|---|
| Bar position | Top, Left, Bottom, Right | `bar.bottom` and `bar.vertical` |
| Bar style | Hug, Float, Rect | `bar.cornerStyle`: 0, 1, 2 |
| Screen round corner | No, Yes, When not fullscreen | `appearance.fakeScreenRounding`: 0, 1, 2 (default 2) |

The [bar](bar.md) and the [screen corners](screen-corners.md) pages describe what each style looks
like.

## The Search page

The Search page sets up how the launcher in the [overview](overview.md) finds things.

| Control | Does | Key, default |
|---|---|---|
| Use Levenshtein distance-based algorithm instead of fuzzy | ranks apps, clipboard entries and emojis by edit distance instead of fuzzy matching, keeping matches that score above 0.2; it copes better with many typos but can miss acronyms | `search.sloppy`, off |
| Non-app result delay (ms) | waits this long after the last keystroke before calculating the math result, so typing stays smooth; 0 to 500 in steps of 10 | `search.nonAppResultDelay`, 30 |
| Show default actions without a prefix | ends every result list with the entries that run the query as a command and calculate it, even when neither prefix was typed | `search.prefix.showDefaultActionsWithoutPrefix`, on |

Under **Prefixes**, one field per prefix sets the text that switches the launcher to that kind of
result:

| Field | Key | Default |
|---|---|---|
| Apps | `search.prefix.app` | `>` |
| Action | `search.prefix.action` | `/` |
| Clipboard | `search.prefix.clipboard` | `;` |
| Emojis | `search.prefix.emojis` | `:` |
| Math | `search.prefix.math` | `=` |
| Shell command | `search.prefix.shellCommand` | `$` |

Math results need `qalc` and the clipboard prefix needs `cliphist`; a notice says so when either is
missing.
