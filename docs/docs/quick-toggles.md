---
title: Quick toggles
sidebar_label: Quick toggles
description: The grid of toggles in the sidebar — tiles and what a click, a right-click or a hold does, every toggle, edit mode for arranging them, the classic style, and the dialogs the tiles open.
---

# Quick toggles

Under the system row of the [sidebar](sidebar.md) sits a grid of quick toggles: tiles that switch
something on or off and, on a right-click, open a dialog or a settings page for it.

## Two styles

`sidebar.quickToggles.style` picks the look:

| Style | Looks like |
|---|---|
| `android`, the default | a grid of tiles, one or two cells wide, arranged in edit mode |
| `classic` | one row of round buttons in a fixed order, described at the end of this page |

It is also under **Appearance › Panels › Sidebars › Quick toggles** in the settings window; see
[Appearance settings](settings-appearance.md). The rest of this page describes the `android` style
unless it says otherwise.

## The grid

The grid is `sidebar.quickToggles.android.columns` cells wide, 5 by default and 1 to 8 in the
settings window. A tile is 56 px tall and one or two cells wide. Tiles fill a row in order until the
next one does not fit, and that one starts the next row; a gap left at the end of a row stays empty.

The tiles and their order are a list of `{ type, size }` entries:

```toml
[sidebar.quickToggles.android]
columns = 5
toggles = [
    { type = "network", size = 2 },
    { type = "bluetooth", size = 2 },
    { type = "idleInhibitor", size = 1 },
    { type = "mic", size = 1 },
    { type = "audio", size = 2 },
    { type = "nightLight", size = 2 },
]
```

That is the default list. With five columns it packs Wi-Fi, Bluetooth and Keep awake on the first
row, and Audio input, Audio output and Night Light on the second. An unknown type is skipped.
Edit mode writes this list, so it rarely needs editing by hand.

## A tile

A one-cell tile shows only its icon. A two-cell tile adds the toggle's name and a status line.

An off tile is fully rounded. An on tile squares its corners and fills with the primary color, with
one exception: a two-cell tile that has a secondary action keeps its neutral
background and lights up the disc behind its icon instead, since that disc is a button of its own.

A tile whose toggle is unavailable, such as Bluetooth without an adapter, is faded and ignores
clicks. Hovering a tile shows its tooltip.

## Click routing

Most toggles have a main action, which switches them, and some have a secondary action, which opens
a dialog or a settings page.

| Input | One-cell tile | Two-cell tile |
|---|---|---|
| Click | main action | secondary action if the toggle has one, else main action |
| Click on the icon disc | — | main action, on a tile with a secondary action |
| Right-click, or press and hold for 800 ms | secondary action | secondary action |

## The toggles

| Type | Tile | Lit while | Click | Secondary action |
|---|---|---|---|---|
| `network` | Wi-Fi | the Wi-Fi radio is on | turns the radio on or off | the Wi-Fi dialog |
| `ethernet` | Ethernet | the wired device is connected | connects or disconnects it | the settings window's Network page |
| `bluetooth` | Bluetooth | the adapter is powered | powers it on or off | the Bluetooth dialog |
| `audio` | Audio output | the default output is not muted | mutes or unmutes it | the settings window's Volume Levels page |
| `mic` | Audio input | the default input is not muted | mutes or unmutes it | the settings window's Volume Levels page |
| `notifications` | Notifications | Do Not Disturb is off | turns Do Not Disturb on or off | — |
| `nightLight` | Night Light | night light is active | turns it on or off | the night light dialog |
| `darkMode` | Dark Mode | the palette is dark | switches the palette to the other mode | — |
| `idleInhibitor` | Keep awake | the idle and sleep inhibitor is held | takes or releases it | — |
| `powerProfile` | Power Profile | the profile is not Balanced | moves to the next profile | — |
| `easyEffects` | EasyEffects | EasyEffects is running | starts it in the background or quits it | opens its window |
| `cloudflareWarp` | Cloudflare WARP | WARP is connected | connects or disconnects | — |
| `wireGuard` | WireGuard | the connection named `WireGuard` is active | brings that connection up or down | the WireGuard dialog |
| `screenSnip` | Screen snip | never | opens the region selector for a screenshot | — |
| `colorPicker` | Color picker | never | starts a color picker | — |
| `onScreenKeyboard` | Virtual Keyboard | the on-screen keyboard is open | opens or closes it | — |

A secondary action that opens the settings window or another program also closes the sidebar.

The status line of a two-cell tile reads:

| Tile | Status |
|---|---|
| Wi-Fi | the network's name, `Disconnected` or `Off` |
| Ethernet | the connection's name or `Disconnected` |
| Bluetooth | the first connected device's name or `Not connected` |
| Audio output | `Unmuted` or `Muted` |
| Audio input | `Enabled` or `Muted` |
| Notifications | `Show` or `Silent` |
| Night Light | `Active` or `Inactive`, after `Auto, ` while the schedule is on |
| Dark Mode | `Dark` or `Light` |
| Power Profile | `Power Saver`, `Balanced` or `Performance` |
| Screen snip, Color picker | no status line |
| the rest | `On` or `Off` |

### Wi-Fi and Ethernet

Both need NetworkManager. The Wi-Fi tile runs `nmcli radio wifi on|off`. Its icon shows the signal
bars of the access point in use, `wifi_find` while it is not connected, and `signal_wifi_off` with
the radio off.

The Ethernet tile works on one wired device: the first one `nmcli` lists as connected, else the first
disconnected one, else the first unavailable one (no cable). It runs `nmcli device connect` or
`nmcli device disconnect` on it. Without any wired device the tile is unavailable. Its secondary
action opens the Network page; see [Network settings](settings-network.md).

### Bluetooth

Talks to BlueZ on the system bus, and is unavailable without an adapter. The settings window's
Bluetooth page, which the dialog's **Details** button opens, is described in
[Bluetooth and devices](settings-devices.md).

### Audio output and Audio input

Mute the default output and the default input. Their secondary action opens **Sound › Volume
Levels** in the settings window, which lists every app playing and every app recording, each with
its own volume; see [Sound settings](settings-sound.md).

### Notifications

Flips `notifications.silent`, the same flag as the button under the sidebar's notification list
and the **Do not disturb** switch in the settings window. See [Notifications](notifications.md).

### Night Light

Turning it on runs `hyprsunset` at `light.night.colorTemperature`, 5000 K by default; turning it off
sets hyprsunset back to 6000 K. With `light.night.automatic` on, the default, night light follows
the schedule from `light.night.from` to `light.night.to`, 19:00 to 06:30 by default, easing in and
out over `light.night.transition` minutes, 30 by default, and a click switches at once and
overrides the schedule until its next start or end time. The icon is `night_sight_auto` while the
schedule is on and `bedtime` otherwise. The schedule and the temperature are also under **Displays ›
Night light**; see [Display settings](settings-displays.md).

### Dark Mode

Runs `proscenio switchwall --mode light` or `--mode dark` with `--noswitch`, which generates the
palette again in the other mode from the same wallpaper. The tile reads the mode from the generated
palette, `~/.local/state/proscenio/generated/material_colors.scss`. See [Colors](colors.md).

### Keep awake

Holds `systemd-inhibit --what=idle:sleep --who=proscenio --why="Keep awake" cat` while it is on.
`cat` reads a pipe from the shell, so the inhibitor ends when the shell exits. The state is kept in
`~/.local/state/proscenio/states.json` under `idle.inhibit`: a shell restarted within the same
Hyprland session takes the inhibitor again, and a fresh Hyprland session starts with it off.

### Power Profile

Steps through power-profiles-daemon's profiles: Power Saver, Balanced, Performance and back to Power
Saver, skipping Performance on a machine that does not offer it. The icons are
`energy_savings_leaf`, `airwave` and `local_fire_department`.

### EasyEffects

Available when `easyeffects` is installed, either as a program or as the Flatpak
`com.github.wwmm.easyeffects`. A click starts it with `--hide-window --service-mode` or ends it; the
secondary action opens its window.

### Cloudflare WARP

Available when `warp-cli status` prints anything. A click runs `warp-cli connect` or
`warp-cli disconnect`. While WARP is available, the status is read again on every polling interval,
`resources.updateInterval`. When the status contains `Unable`, the shell runs
`warp-cli registration new` and then `warp-cli connect`, and sends a notification if either fails.

### WireGuard

The tile switches one NetworkManager connection, the one named exactly `WireGuard`, with
`nmcli connection up|down WireGuard`, and is lit while that connection is active. The WireGuard
dialog reaches every WireGuard connection, whatever its name. The tile's icon is the WireGuard logo.

### Screen snip and Color picker

Both close the sidebar and wait 300 ms, so it is off the screen, before they start: screen snip
opens the [region selector](region-selector.md) for a screenshot, and the color picker runs
`hyprpicker -a`, which copies the picked color.

### Virtual Keyboard

Opens and closes the [on-screen keyboard](on-screen-keyboard.md). The icon is `keyboard`, or
`keyboard_hide` while it is open.

## Edit mode

The `edit` button in the sidebar's system row turns edit mode on and off; closing the sidebar turns
it off too. While it is on:

- a divider and a second section appear under the grid, holding every toggle that is not placed, one
  cell each; the section stays one cell tall even when every toggle is placed, so it always takes a
  drop;
- the grid ends in an empty row, and a toggle dropped there goes to the end of the list;
- tiles do not run their actions, and unavailable tiles can be moved like the rest.

| To | Do |
|---|---|
| Move a toggle | drag its tile; the grid shows where it will land as the pointer moves |
| Add a toggle | drag it from the lower section into the grid; it lands one cell wide |
| Remove a toggle | drag its tile into the lower section |
| Resize a toggle | right-click its tile in the grid, or hold it still for 800 ms, to switch between one and two cells |

A drag starts once the pointer has moved 6 px. A dropped tile lands before the first tile, in the
row under the pointer, whose middle the pointer has not passed. Every change is written to
`sidebar.quickToggles.android.toggles` in `~/.config/proscenio/config.toml` at once.

## The classic style

With `style = "classic"`, the toggles are one centered row of round 40 px buttons in a fixed order:
Wi-Fi, Bluetooth, Night Light, Keep awake, EasyEffects, Cloudflare WARP and WireGuard. Bluetooth,
EasyEffects and Cloudflare WARP show only while they are available. There is no edit mode, and the
column count does not apply.

A click does what the tile of the same toggle does. A right-click, or a hold for 800 ms, does this
instead:

| Button | Right-click |
|---|---|
| Wi-Fi | opens the settings window's Wi-Fi page, or the Network page while the main connection is wired, and closes the sidebar |
| Bluetooth | opens the settings window's Bluetooth page and closes the sidebar |
| Night Light | turns the schedule on or off |
| EasyEffects | opens its window and closes the sidebar |
| WireGuard | opens the WireGuard dialog |

The Wi-Fi button shows `lan` while the main connection is wired, and its tooltip names the
connection in use.

## The dialogs

A tile's secondary action can open one of four dialogs. Each is a card that drops in over the
sidebar and dims it. A click on the dimmed part, Escape, or **Done** closes it. One dialog is open at
a time, and closing the sidebar removes it.

### Wi-Fi

**Connect to Wi-Fi.** Opening it turns the Wi-Fi radio on and starts a scan, with a progress bar
while the scan runs. Each network in the list shows its signal strength, its name, a lock when it is
secured, and a check when it is the one in use.

A click on a network connects to it. A secured network without a saved profile, or one whose
connection NetworkManager refuses for missing secrets, opens a password field in its row instead,
with **Cancel** and **Connect**; Enter connects too. The password goes to NetworkManager over D-Bus,
never on a command line, and once the connection succeeds it replaces any saved profile of the same
name. An enterprise (802.1X) network without a saved profile closes the sidebar and opens the
settings window on a connection page to set it up.

**Details** closes the sidebar and opens the settings window's Wi-Fi page, or its Network page while
the main connection is wired.

### Bluetooth

**Bluetooth devices.** Opening it powers the adapter on and starts discovery, with a progress bar
while it runs; closing the dialog stops discovery. The list puts connected devices first, then paired
ones, then the rest, each with an icon for its kind (headphones, speaker, phone, mouse, keyboard).
A paired device shows `Connected` or `Paired`, and its battery level when it reports one.

A click on a device expands it:

| Device | Buttons |
|---|---|
| not paired | **Always connect**, which pairs it, and **Connect** |
| paired | **Forget**, which removes it, and **Connect** or **Disconnect** |

**Details** closes the sidebar and opens the settings window's Bluetooth page.

### Night light

**Eye protection.** The Night Light section has the **Enable now** switch, which does what a click on
the tile does; the **Automatic** switch, which turns the schedule on or off; and the **Intensity**
slider, the color temperature from 6500 K at the left to 1200 K at the right, saved as
`light.night.colorTemperature`. Under it, a **Brightness** slider sets this monitor's backlight and a
**Gamma** slider its gamma from 25 to 100 %: the two controls the sidebar's brightness slider
combines.

### WireGuard

**WireGuard Connections.** Lists every WireGuard connection NetworkManager has, each `Connected` or
`Disconnected`. A click brings that connection up or down, and the list is read again half a second
later. **New Connection** closes the sidebar and opens the settings window's Network page.
