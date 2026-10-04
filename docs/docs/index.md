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

## Start here

[Building and installing](installing.md) covers what proscenio needs, where it goes and how to start
it with a session.
