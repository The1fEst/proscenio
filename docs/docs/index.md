---
title: Overview
sidebar_label: Overview
sidebar_position: 0
slug: /
description: proscenio is a desktop shell for Hyprland, written in Rust with GTK 4, that draws the bar, the panels, the lock screen and a settings app in one process.
---

# proscenio

proscenio is a desktop shell for Hyprland, written in Rust with GTK 4. It draws everything around
the windows: the bar, the sidebar with its quick toggles, notifications, the dock, the launcher and
workspace overview, on-screen indicators, the session and lock screens, and a settings app for the
shell and for Hyprland itself. Its colors come from the wallpaper.

It is built to stay small and quick:

- every panel is compiled Rust, with no scripting engine underneath;
- GTK's Cairo renderer is the default, so the shell's surfaces hold no GPU memory;
- a hidden panel gives back its surface and buffers instead of waiting off screen.

*Proscenio* is Italian for the proscenium, the front of the stage before the curtain.

## Built on Hyprland

proscenio runs only on Hyprland, and leans on it in three places:

- the IPC sockets `.socket.sock` and `.socket2.sock` under `$HYPRLAND_INSTANCE_SIGNATURE`, for
  workspaces, windows, monitors, the keyboard layout and every dispatch (`hl.dsp.*`);
- Hyprland's Wayland protocols: `hyprland-global-shortcuts-v1` for keybinds,
  `hyprland-focus-grab-v1` for closing panels on an outside click, `hyprland-toplevel-export-v1`
  for window previews, and `hyprland-lock-notify-v1` for the session's lock state;
- Hyprland's Lua config, which the settings window writes and reads back with `getoption`.

[Hyprland integration](hyprland.md) has the details.

## Start here

| Page | What it covers |
|---|---|
| [Building and installing](installing.md) | What proscenio needs, where it goes, how to start it, and the programs each feature calls |
| [First run](first-run.md) | What the first start sets up and the welcome window |
| [Files and folders](files.md) | Every file proscenio reads or writes |
| [Command line](command-line.md) | The subcommands of the `proscenio` binary |

## The shell

| Page | What it covers |
|---|---|
| [The bar](bar.md) | The bar on each screen and every group, indicator and popup in it |
| [Workspaces](workspaces.md) | The workspace indicator in the bar |
| [System tray](tray.md) | Tray icons, the overflow menu and item menus |
| [The sidebar](sidebar.md) | The panel on the right edge: system buttons, sliders, notification list |
| [Quick toggles](quick-toggles.md) | The toggle grid, edit mode and the dialogs the tiles open |
| [Notifications](notifications.md) | The notification server, popups, history, Do Not Disturb and quiet screen sharing |
| [The dock](dock.md) | Pinned and running apps with live window previews |
| [Overview and launcher](overview.md) | Search, math, commands, clipboard and emoji, over a grid of workspaces |
| [The background](background.md) | The wallpaper, its parallax, and the clock and weather over it |
| [Screen corners](screen-corners.md) | Rounded corners and the corner regions that open panels |
| [Calendar](calendar.md) | The month view, the to-do list, the pomodoro timer and stopwatch |
| [Media controls](media-controls.md) | Player cards colored from the album art |
| [On-screen display](osd.md) | The volume, brightness and gamma indicator |
| [On-screen keyboard](on-screen-keyboard.md) | The keyboard that types through `ydotool` |
| [Cheatsheet](cheatsheet.md) | Every Hyprland keybind, grouped |
| [Screenshots and recording](region-selector.md) | The region selector and screen recording |
| [Wallpaper selector](wallpaper-selector.md) | The folder browser that picks a wallpaper |
| [Session screen](session-screen.md) | The power menu |
| [Lock screen](lock.md) | The session lock, PAM and fingerprint unlocking |

## Settings

| Page | What it covers |
|---|---|
| [The settings window](settings.md) | Layout, search, how controls behave and where values are stored |
| [Network settings](settings-network.md) | Wi-Fi, connections, the hotspot, proxy and firewall |
| [Bluetooth and devices](settings-devices.md) | Pairing devices and turning input devices on or off |
| [Display settings](settings-displays.md) | Monitor layout, modes, color, HDR and night light |
| [Sound settings](settings-sound.md) | Devices, volume levels per app and card profiles |
| [Power and screen lock](settings-power.md) | Idle timeouts, buttons and lid, battery, power profiles, the lock |
| [Multitasking settings](settings-multitasking.md) | Layouts, gaps, focus, workspaces, the overview and swiping |
| [Appearance settings](settings-appearance.md) | Colors, fonts, windows, the background, the bar and the panels |
| [Apps settings](settings-apps.md) | Default apps, file types and window rules |
| [Mouse, keyboard and accessibility](settings-input.md) | Pointer, touchpad and gestures, keyboard and shortcuts, accessibility |
| [System settings](settings-system.md) | Notifications, privacy, language, date and time, users, services |

## Look and feel

| Page | What it covers |
|---|---|
| [Colors from the wallpaper](colors.md) | `proscenio switchwall`, matugen and the generated palette |
| [Design system](design.md) | The palette roles, transparency, rounding, fonts, motion and widgets |

## Integration

| Page | What it covers |
|---|---|
| [Global shortcuts](shortcuts.md) | Every shortcut name and how to bind it in Hyprland |
| [IPC](ipc.md) | Calling the running shell from scripts |
| [Polkit agent](polkit.md) | The authentication dialog for polkit |
| [Keyring prompt](keyring-prompt.md) | The password prompt for gnome-keyring and other gcr clients |

## Internals

| Page | What it covers |
|---|---|
| [Architecture](architecture.md) | How the process is organized and the mechanisms the panels share |
| [Hyprland integration](hyprland.md) | The sockets, protocols and config files proscenio works through |
