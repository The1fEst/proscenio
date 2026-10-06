---
title: The sidebar
sidebar_label: Sidebar
description: The panel on the right edge of the screen — how it opens and closes, the system button row, the brightness, volume and microphone sliders, and the notification list it ends with.
---

# The sidebar

The sidebar is a full-height panel on the right edge of the screen. From top to bottom it holds a
row of system buttons, an optional group of sliders, a row of update buttons while there is
something to update, the [quick toggles](quick-toggles.md), and the [notification](notifications.md)
list, which takes whatever height is left.

## Opening and closing

Every monitor the shell runs on has a sidebar of its own.

| To open or close it | Acts on |
|---|---|
| Click the right side of the [bar](bar.md), away from its buttons | that monitor |
| Click or hover a [screen corner](screen-corners.md) set up to open it | that monitor |
| `proscenio ipc call sidebarRight toggle`, or `open`, `close` | Hyprland's focused monitor |
| The global shortcuts `sidebarRightToggle`, `sidebarRightOpen`, `sidebarRightClose` | Hyprland's focused monitor |

[IPC](ipc.md) and [Shortcuts](shortcuts.md) show how to call and bind these.

The sidebar closes on:

- **Escape.** With a [dialog](quick-toggles.md) open, the first Escape closes the dialog and the
  second the sidebar.
- **A click outside it**, on a window, the bar or the wallpaper. This is Hyprland's
  `hyprland-focus-grab-v1`; the [on-screen keyboard](on-screen-keyboard.md) is the one surface that
  does not count as outside, so it can type into the sidebar.
- **Anything in it that hands off elsewhere**: the settings button, the tiles that open a settings
  page or another program, screen snip and the color picker.

Closing also removes an open dialog and ends the quick toggles' edit mode. The content is built
once and kept; about a second after the sidebar closes, its window gives back its surface and
buffers.

While a sidebar is open, notifications do not pop up, and opening one marks them all as read. See
[Notifications](notifications.md).

With `background.parallax.enableSidebar` on, the default, a wallpaper wider than the screen pans a
little to the side while the sidebar is open. See [Background](background.md).

## The window

| Property | Value |
|---|---|
| Layer | top |
| Namespace | `proscenio:sidebarRight`, for Hyprland layer rules |
| Anchors | top, right and bottom |
| Width | 460 px: the panel, 5 px of gap above, below and to the right, and 10 px on the left for its shadow |
| Exclusive zone | none of its own; it keeps clear of the bar's, so it starts below a top bar |
| Keyboard focus | on demand |

## The system button row

On the left, the uptime pill: the distribution's logo, chosen by `ID` in `/etc/os-release` (a
generic Linux logo for one it does not know), and the time since boot from `/proc/uptime`, as in
`Up 2d, 3h, 14m`. It refreshes on the shell's polling interval, `resources.updateInterval`
(3000 ms by default).

On the right, four buttons:

| Icon | Tooltip | Does |
|---|---|---|
| `edit` | Edit quick toggles | turns the quick toggles' [edit mode](quick-toggles.md) on and off; shown only with the `android` toggle style |
| `restart_alt` | Reload Hyprland & proscenio | runs `hyprctl reload`, then replaces the running proscenio with a fresh start of the same binary and arguments |
| `settings` | Settings | closes the sidebar and opens the [settings window](settings.md) |
| `power_settings_new` | Session | opens the [session screen](session-screen.md) |

## Quick sliders

A group of up to three sliders under the system row. It is off by default and appears only when it
is enabled and at least one slider is picked:

```toml
[sidebar.quickSliders]
enable = true          # default false
showBrightness = true  # default true
showVolume = true      # default true
showMic = false        # default false
```

The same switches are under **Appearance › Panels › Sidebars › Sliders** in the settings window;
see [Appearance settings](settings-appearance.md).

While a slider's handle is held, a tooltip over it shows the value. A value changed elsewhere, by a
key or another panel, moves the slider too.

### Brightness

One slider drives two controls of the monitor the sidebar is on:

| Part of the slider | Controls |
|---|---|
| Upper 70 % | the backlight, 0 to 100 %: `brightnessctl` for a built-in screen, `ddcutil` (DDC/CI) for an external monitor |
| Lower 30 % | the gamma, 25 to 100 %, through the screen's color matrix (see [Night light](settings-displays.md#night-light)) |

Dragging into the lower part turns the backlight down to zero and dims further with gamma; dragging
back up sets gamma to 100 % again. A `wb_twilight` icon marks the split. The tooltip shows the
backlight percentage, or `Gamma` and its percentage in the lower part. When gamma is below 100 %
while the backlight is above zero, as the night light dialog's two sliders can leave it, the handle
shows the gamma and a dot on the track marks the backlight level.

### Volume and microphone

The volume of the default output and of the default input, 0 to 100 %. Muting is on the
[quick toggles](quick-toggles.md).

## Updates

Under the sliders, a row of two wide buttons appears while the shell is behind its repository or
enough packages wait. The bar shows the `deployed_code_update` indicator at the same time (see
[Indicators](bar.md#indicators)).

| Button | Status | Click runs |
|---|---|---|
| Shell update | how many commits `main` of `The1fEst/proscenio` has that the running build lacks, or **Up to date** | `apps.shellUpdate` |
| System update | how many packages can be upgraded, or **Up to date** | `apps.update` |

A button with nothing to update is dimmed. The System update button turns the primary color from
`updates.stronglyAdviseUpdateThreshold` packages (200). Clicking a button closes the sidebar and runs
its command; both are on the [Apps](settings-apps.md) page.

While `updates.enableCheck` is on, the shell checks at start and then every
`updates.checkInterval` minutes (120):

- **Packages**: it syncs a private copy of the package databases in `~/.cache/proscenio/pacman`
  with `unshare -r pacman -Sy`, which needs no root and leaves the system databases alone, then
  asks the first of `paru`, `yay` and `pacman` it finds for `-Qu` against that copy. `paru` and
  `yay` count AUR packages too. Ignored packages do not count. The row and the indicator appear
  once the count reaches `updates.adviseUpdateThreshold` (1).
- **The shell**: the build's version, `r<commits>.<hash>`, names the commit it was built from. The
  shell asks the GitHub API to compare that commit with `main` and counts the commits `main` is
  ahead. A build from a commit GitHub does not have counts as up to date.

## The notification list

The last group fills the rest of the column, and is at least 170 px tall. It lists every
notification the shell holds, grouped by app with the newest group first, on the same cards the
popups use; [Notifications](notifications.md) describes the cards and what they take as input. An
empty list shows a placeholder that reads **Nothing**.

Under the list are two buttons with the count between them:

| Item | Does |
|---|---|
| `notifications_paused` | turns Do Not Disturb on and off; it is highlighted while on |
| *N* notifications | the number of notifications held; a label, not a button |
| `delete_sweep` | dismisses every notification |
