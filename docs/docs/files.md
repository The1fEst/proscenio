---
title: Files and folders
sidebar_label: Files and folders
description: Every file and folder proscenio reads or writes, from config.toml and its state and cache to the runtime folder and the files of other programs it edits.
---

# Files and folders

proscenio keeps its own files in the XDG base directories, in a folder named `proscenio` in each.
The paths below use the defaults; `XDG_CONFIG_HOME`, `XDG_STATE_HOME`, `XDG_CACHE_HOME`,
`XDG_DATA_HOME` and `XDG_RUNTIME_DIR` move them.

| Folder | Holds |
|---|---|
| `~/.config/proscenio/` | What you write: the settings, launcher scripts, translations |
| `~/.local/state/proscenio/` | What the shell remembers: the generated colors, timers, the to-do list |
| `~/.cache/proscenio/` | What can be rebuilt: notification history, images, album art |
| `$XDG_RUNTIME_DIR/proscenio/` | Scratch files of the running session |
| `~/.local/share/proscenio/` | The built-in default wallpaper, once written out |

## Configuration

| Path | What |
|---|---|
| `~/.config/proscenio/config.toml` | Every setting of the shell |
| `~/.config/proscenio/actions/` | Scripts the launcher offers as actions |
| `~/.config/proscenio/translations/<code>.json` | Extra translated strings for one language |

### config.toml

The settings are TOML, nested by area. A key path such as `appearance.palette.accentColor` is the key
`accentColor` in the table `[appearance.palette]`:

```toml
# My settings
[bar]
bottom = true

[appearance.palette]
type = "scheme-content"
accentColor = "#8caaee" # a soft violet
```

A missing key means its default, and a missing file means every default; the settings window adds a
key only when you change it. It writes one key at a time and leaves the rest of the file as it is,
comments, order and spacing included. A changed value keeps the comment after it on its line, and a
table that does not exist yet is added at the end of the file. Each write goes to
`config.toml.tmp` first and replaces `config.toml` in one rename, so a reader never sees half a file.

The shell watches the file: when you edit it by hand, the panels on each monitor rebuild the parts
whose keys changed, without a restart.

### Launcher actions

Each file in `~/.config/proscenio/actions/` whose name does not start with `.` becomes a launcher
action named after the file without its extension: `backup.sh` is the action `backup`. Type the action
prefix, `/` unless `search.prefix.action` says otherwise, then the name and any words, and the file
runs with those words as its arguments. It has to be executable.

### Translations

The interface language is `language.ui`, or the session's locale when that is `auto`, the default. The
strings for a language come from the catalog built into the binary. A file named after the language
code, such as `~/.config/proscenio/translations/de_DE.json`, adds strings the catalog lacks: one JSON
object mapping each English string to its translation. Where both have a string, the catalog's wins.

## State

| Path | What |
|---|---|
| `~/.local/state/proscenio/generated/` | The colors generated from the wallpaper; see [Colors from the wallpaper](colors.md) |
| `~/.local/state/proscenio/states.json` | Timers and the stopwatch, the idle inhibitor, the calendar's last tab, the remembered screenshot region, the apps that have sent notifications, and the Hyprland instance proscenio last ran in |
| `~/.local/state/proscenio/todo.json` | The to-do list |
| `~/.local/state/proscenio/shaders/<filter>.frag` | The color filter shaders the accessibility settings hand to Hyprland's `decoration:screen_shader` |
| `~/.local/state/proscenio/first_run.txt` | Present once the welcome window has been shown |
| `~/.local/state/proscenio/defaults_applied.txt` | Present once the first-run defaults have been applied |

At start, a missing `first_run.txt` opens the [welcome window](first-run.md) and writes the file;
when `defaults_applied.txt` is missing too, the first-run defaults are applied first. Deleting
`first_run.txt` alone brings the welcome window back on the next start without applying the defaults
again.

The files in `generated/`:

| File | Contents |
|---|---|
| `colors.json` | The shell's palette, written by a matugen template |
| `color.txt` | The source color of the palette |
| `material_colors.scss` | The palette as SCSS variables, with the terminal colors and `$darkmode` |
| `terminal/kitty-theme.conf` | A kitty color theme |
| `terminal/sequences.txt` | Escape sequences that recolor a running terminal |

## Cache

| Path | What |
|---|---|
| `~/.cache/proscenio/notifications.json` | The notification history |
| `~/.cache/proscenio/notifications/<id>.png` | Images that notifications send as raw pixels, one per notification, removed with it |
| `~/.cache/proscenio/coverart/` | Album art the media controls download, named by the MD5 of its URL |
| `~/.cache/thumbnails/<size>/<md5>.png` | Wallpaper thumbnails, in the shared freedesktop thumbnail cache (`normal`, `large`, `x-large`, `xx-large`) |

## Runtime

These last only as long as the session; most are removed as soon as their job is done.

| Path | What |
|---|---|
| `$XDG_RUNTIME_DIR/proscenio/screenshot/image-<monitor>` | A monitor's full capture while a region is being chosen |
| `$XDG_RUNTIME_DIR/proscenio/cliphist/` | Clipboard images decoded for the launcher; removed when the overview closes |
| `$XDG_RUNTIME_DIR/proscenio/cava.conf` | The `cava` config behind the media controls' audio wave |
| `$XDG_RUNTIME_DIR/proscenio/micgate.png` | The icon of the microphone notification |
| `$XDG_RUNTIME_DIR/proscenio/face.png` | A user picture cropped to a square before the account takes it |
| `$XDG_RUNTIME_DIR/proscenio/autostarted` | Present once the autostart entries have run in this session |
| `$XDG_RUNTIME_DIR/proscenio-capture-<pid>-<n>` | Scratch memory for window and screen captures, deleted the moment it is opened |

## Data

`~/.local/share/proscenio/default_wallpaper.png` is the default wallpaper built into the binary,
written out the first time it is needed.

## What proscenio saves for you

| What | Where |
|---|---|
| Screenshots | `screenshot-<date>_<time>.png` in `screenSnip.savePath`, when `screenSnip.save` is on and the path is set; otherwise a copied screenshot only goes to the clipboard |
| Recordings | `recording_<date>_<time>.mp4` in `screenRecord.savePath`, else the XDG Videos folder |

## Files of other programs

proscenio also reads and writes files that belong to other programs. The settings window writes
them when the matching setting changes; `proscenio switchwall` writes the color files and the video
wallpaper files when it runs.

| Path | What proscenio does with it |
|---|---|
| `~/.config/hypr/settings/<area>.lua` | Writes Hyprland options, monitor and window rules and shortcuts, one file per area; see [Hyprland](hyprland.md) |
| `~/.config/hypr/settings.lua` | If present at start, moves its statements into the area files and renames it `settings.lua.bak` |
| `~/.config/hypr/hyprland/general.lua` | Reads the gestures defined there, to list them as defaults |
| `~/.config/hypr/hyprland/scripts/fuzzel-emoji.sh` | Reads the launcher's emoji list, the lines after `### DATA ###` |
| `~/.config/hypr/hypridle.conf` | Writes the idle timeouts |
| `~/.config/hypr/hyprlock.conf` | Switches the clock between `TIME` and `TIME12` with the 12-hour setting |
| `~/.config/hypr/custom/scripts/__restore_video_wallpaper.sh` | Writes a script that replays a video wallpaper |
| `~/.config/hypr/custom/scripts/mpvpaper_thumbnails/` | Stores the first frame of each video wallpaper |
| `~/.config/kdeglobals` | Writes Qt colors, fonts, the icon theme and the widget style; the shell takes its own icon theme from `[Icons]` `Theme` and follows it |
| `~/.local/share/color-schemes/MaterialYou{Light,Dark}.colors` | Writes the two Qt color schemes |
| `~/.config/gtk-3.0/settings.ini`, `~/.config/gtk-4.0/settings.ini` | Writes the GTK theme, icon theme, cursor and font |
| `~/.icons/default/index.theme` | Writes the cursor theme as `Inherits` |
| `~/.config/autostart/*.desktop` | Adds, removes, enables and disables autostart entries, and starts the enabled ones once per session |
| `~/.config/mimeapps.list` | Writes the default apps |
| `~/.config/kioslaverc` | Writes the proxy |
| `~/.config/<editor>/User/settings.json` | Writes `material-code.primaryColor` for VS Code and its forks |

The mode, the GTK theme, fonts, the icon theme and the cursor also go to `org.gnome.desktop.interface`
in GSettings.

The settings that change the system run `proscenio` as root through `pkexec` and touch system files;
[Command line](command-line.md) lists them:

| Path | Command |
|---|---|
| `/etc/ufw/` | `firewall`, through `ufw` |
| `/etc/systemd/logind.conf.d/50-proscenio.conf` | `power-settings button` |
| `/sys/class/power_supply/BAT*/charge_control_end_threshold`, `/etc/tmpfiles.d/proscenio-charge-limit.conf` | `power-settings charge-limit` |
| `/etc/locale.gen`, and the system locale through `localectl` | `set-system-locale` |

Everything else the shell shows, such as icons, the default wallpaper, the terminal templates and the
`cava` config, is built into the binary.
