---
title: First run
sidebar_label: First run
description: What proscenio sets up the first time it starts for a user, the marker files that record it, and the welcome window that opens afterward.
---

# First run

The first time proscenio starts for a user, it applies a set of defaults and opens the welcome
window: one page with the settings a fresh setup most often needs, such as the language, the
displays, the bar and the wallpaper.

## What the first start does

At every start, proscenio looks for `first_run.txt` in `~/.local/state/proscenio/`
(`$XDG_STATE_HOME/proscenio/` when that variable is set). When the file is missing, it:

1. writes `first_run.txt`;
2. if `defaults_applied.txt` is missing too, writes it and applies the defaults below;
3. opens the welcome window.

| File in `~/.local/state/proscenio/` | While it exists |
|---|---|
| `first_run.txt` | The welcome window does not open on its own |
| `defaults_applied.txt` | The defaults are never applied again |

Deleting `first_run.txt` brings the welcome window back at the next start, without applying the
defaults again. Deleting both files repeats the whole first run.

## The defaults

**The main display.** The display set as main on the [Displays](settings-displays.md) page, or else
the first by connector type and number, becomes the main one: its monitor rule is written, and the
layout moves so that it sits at 0,0.

**Default apps**, in `~/.config/mimeapps.list`, each only when the app is installed:

| Role | Desktop entry |
|---|---|
| Web | `brave-origin.desktop` |
| Mail | `org.mozilla.Thunderbird.desktop` |
| Calendar | `org.gnome.Calendar.desktop` |
| Music, Video | `vlc.desktop` |
| Photos | `satty.desktop` |
| Text | `com.microsoft.VSCode.desktop` |
| Files | `org.kde.dolphin.desktop` |

**Fonts, themes and the cursor**, written the way the [Appearance](settings-appearance.md) pages
write them:

| Setting | Value |
|---|---|
| General and menu fonts | Google Sans Medium 11 |
| Toolbar and window title fonts | Google Sans Medium 10 |
| Small font | Google Sans Medium 9 |
| Fixed-width font | JetBrainsMono Nerd Font Medium 11 |
| GTK theme | `adw-gtk3-dark` |
| Qt style | `Darkly` |
| Icon theme | `Papirus-Dark` |
| Cursor | `Bibata-Modern-Ice` at 36 |

**The wallpaper.** The default wallpaper is built into the binary. It is written to
`~/.local/share/proscenio/default_wallpaper.png` and applied with `proscenio switchwall --image`,
so the shell's [colors](colors.md) come from it.

## The welcome window

The welcome window is an ordinary window, 900 × 650 and at least 600 × 400, built from the same
parts as the [settings](settings.md) window. Its title bar follows the same two settings,
`windows.showTitlebar` and `windows.centerTitle`. It reads "Hi there! First things first...", and
holds a **Show next time** switch and a close button.

| Section | Holds |
|---|---|
| Language | **Auto (System)** and every translation proscenio ships |
| Displays | With more than one monitor, a monitor choice and the arrangement; then the resolution and refresh rate |
| Sound | The output and input devices |
| Bar | The bar's position (Top, Left, Bottom, Right) and style (Hug, Float, Rect) |
| Style & wallpaper | Large Light and Dark buttons, each a small preview of the shell in that mode, and **Choose file**, which opens `kdialog` to pick a wallpaper |
| Power saving | The automatic screen blank and the suspend timeouts, which need `hypridle` |
| Info | **Keybinds**, which opens the [cheat sheet](cheatsheet.md), and two links to guides on the web |
| Useless buttons | Two more links to web pages |

Every control acts at once, as it does in the settings window.

**Choosing a language** writes `language.ui` and restarts the shell, because the shell's language is
read only at start. A language other than **Auto (System)** first becomes the system language too,
the way **System › Region & Language** sets it (see [System](settings-system.md)), which asks for
authentication. The restarted shell opens the welcome window again.

**Show next time** deletes `first_run.txt` when turned on and writes it back when turned off, so it
decides whether the window opens at the next start. It never brings the defaults back.

Escape or the close button closes the window. Closing it sends a notification that names the
shortcuts for reopening it and for the settings window.

`proscenio ipc call welcome open`, `close` and `toggle` reach the window at any time.

## How it works

`src/panels/welcome/mod.rs` builds the window and holds the first-run check,
`src/panels/welcome/defaults.rs` applies the defaults, and `src/panels/welcome/preference.rs` draws
the Light and Dark buttons. The window is built when it opens and dropped when it closes. The
language restart passes `PROSCENIO_OPEN_WELCOME` to the restarted process, which opens the window
and clears the variable.
