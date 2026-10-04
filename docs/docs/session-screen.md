---
title: Session screen
sidebar_label: Session screen
description: The full-screen power menu with its eight actions, how to move around it, what each action runs, and the warnings it shows before you leave.
---

# Session screen

The session screen is a full-screen menu for leaving the session: locking, sleeping, logging out,
restarting or shutting down. It covers the focused monitor, with the desktop showing faintly through
its background.

## Opening it

| Global shortcut | `proscenio ipc call session …` | Does |
|---|---|---|
| `sessionToggle` | `toggle` | Opens or closes the screen |
| `sessionOpen` | `open` | Opens it |
| `sessionClose` | `close` | Closes it |

The **Session** button in the [sidebar](sidebar.md) opens it too. It closes when the screen locks.

## The actions

Eight buttons sit in a grid of four columns:

| Button | Runs |
|---|---|
| Lock | `loginctl lock-session` |
| Sleep | `systemctl suspend`, or `loginctl suspend` if that fails |
| Logout | Closes every window, then `systemctl --user stop graphical-session.target` and `pkill -i Hyprland` |
| Reboot to Windows | Closes every window, makes the firmware's "Windows Boot Manager" entry the next boot with `efibootmgr`, then reboots |
| Hibernate | `systemctl hibernate`, or `loginctl hibernate` |
| Shutdown | Closes every window, then `systemctl poweroff`, or `loginctl poweroff` |
| Reboot | Closes every window, then `reboot`, or `loginctl reboot` |
| Reboot to firmware settings | Closes every window, then `systemctl reboot --firmware-setup`, or the same through `loginctl` |

Closing every window sends `SIGTERM` to the process of each window Hyprland lists, so apps get the
chance to save and quit on their own.

**Lock** asks logind to lock the session. What locks it is the program that answers logind's lock
signal, usually `hypridle` through its `lock_cmd`; the [lock screen](lock.md) page covers how to
point that at proscenio.

**Logout** stops `graphical-session.target` first, so the services that are part of the session,
the shell included, stop cleanly before Hyprland goes. The command runs in a scope of its own, so
stopping the shell does not cut it short.

**Reboot to Windows** reads the boot entries with `efibootmgr`, sets `BootNext` to the one labeled
"Windows Boot Manager" through `pkexec efibootmgr --bootnext`, which asks for authentication, and
reboots. Without such an entry, or if authentication fails, the windows are closed all the same, but
the machine does not reboot.

## Moving around

When the screen opens, **Lock** has the focus. Under the grid, a caption names the focused button.

| Key or mouse | Does |
|---|---|
| Arrow keys | Move the focus; at an edge of the grid they stop instead of wrapping |
| Enter | Runs the focused button |
| Escape | Closes the screen |
| Click on a button | Runs it |
| Click anywhere else | Closes the screen |

Running an action closes the screen.

## Warnings

Every time it opens, the screen checks for two things that leaving the session could break, and
shows a warning under the grid for each it finds:

| Warning | Shown when |
|---|---|
| There might be a download in progress. Check your Downloads folder. | `curl`, `wget`, `aria2c` or `yt-dlp` is running, or `~/Downloads` holds a `.crdownload` or `.part` file |
| Your package manager is running | `yay`, `paru`, `dnf`, `zypper`, `apt`, `apx`, `xbps`, `snap`, `apk`, `yum`, `epsi` or `pikman` is running, or `/var/lib/pacman/db.lck` exists |

## How it works

Each monitor the shell draws on has its own session screen, and the shortcuts and IPC calls act on
the focused monitor's. It is a surface on the `overlay` layer, namespace `proscenio:session`,
anchored to every edge and taking the keyboard exclusively while it is shown. The window is
`src/panels/sessionscreen.rs`, and the actions are in `src/services/session.rs`.
