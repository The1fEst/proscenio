# proscenio

proscenio is a desktop shell for Hyprland, written in Rust with GTK 4. It is
part of the [dots-hyprland](https://github.com/The1fEst/dots-hyprland)
dotfiles, where it replaces the Quickshell `ii` shell, and is built for speed
and a small memory footprint:

- no QML or JavaScript engine: every panel is compiled Rust;
- Cairo is the default renderer, so the shell's surfaces hold no GPU memory
  (OpenGL and Vulkan are a setting away);
- a hidden panel is unrealized and gives back its surface and buffers.

proscenio expects the environment dots-hyprland sets up: its Hyprland config
and keybinds, its color pipeline and the packages its installer brings. It has
never been tested outside that environment.

It works only on Hyprland, because it depends on Hyprland in these places:

- the IPC sockets `.socket.sock` and `.socket2.sock` under
  `$HYPRLAND_INSTANCE_SIGNATURE` for workspaces, windows, monitors, the
  keyboard layout and every dispatch (`hl.dsp.*`);
- Hyprland's Wayland protocols: `hyprland-global-shortcuts-v1` for the
  keybinds, `hyprland-focus-grab-v1` for closing panels on an outside click,
  `hyprland-toplevel-export-v1` for window previews in the dock and the
  overview, `hyprland-lock-notify-v1` for the session's lock state;
- Hyprland's config: the settings window writes options, monitor rules,
  window rules and shortcuts to one file per area in `~/.config/hypr/settings/`
  in Hyprland's Lua syntax and idle timeouts to `hypridle.conf`, and reads live
  values with `getoption`.

*Proscenio* is Italian for the proscenium, the front of the stage before the
curtain.

## Building

```
cargo build --release          # proscenio on its own
cargo release-compat           # with the layer that stands in for the QML shell
```

Both write `target/release/proscenio`. [compat.md](compat.md) describes the
compat layer.

## Documentation

Each document describes one part of the QML shell,
`dots/.config/quickshell/ii` in dots-hyprland, and ends with a **Status** part
that says what proscenio does with it and what is *missing* or *differs*. QML
paths are relative to that directory; paths under `src/` are in this
repository. Numbers are given as the QML constants they come from.

| Document | Covers |
| --- | --- |
| [foundations.md](foundations.md) | `modules/common` — the tokens, colors, curves and reusable widgets everything else is built from |
| [bar.md](bar.md) | `modules/ii/bar` — the bar window, its layout, every widget in it, and the tray |
| [sidebar-right.md](sidebar-right.md) | `modules/ii/sidebarRight` — the panel, the quick toggles, the six dialogs, the notification list |
| [background.md](background.md) | `modules/ii/background` — the wallpaper, its parallax, the widget canvas |
| [notifications.md](notifications.md) | `services/Notifications.qml` — the daemon, the store, the unread count |
| [screen-corners.md](screen-corners.md) | `modules/ii/screenCorners` — the fake screen rounding and the corner-open regions |
| [calendar-panel.md](calendar-panel.md) | `modules/ii/calendarPanel` — the month view, the to-do list and the timers the clock opens |
| [osd.md](osd.md) | `modules/ii/onScreenDisplay` — the volume, brightness and gamma indicators |
| [osk.md](osk.md) | `modules/ii/onScreenKeyboard` — the on-screen keyboard typing through `ydotool` |
| [welcome.md](welcome.md) | `modules/ii/welcome` — the first-run window and what the first run sets |
| [session-screen.md](session-screen.md) | `modules/ii/sessionScreen` — the power menu and its eight actions |
| [media-controls.md](media-controls.md) | `modules/ii/mediaControls` — the player cards the bar's media widget opens |
| [dock.md](dock.md) | `modules/ii/dock` — the hiding app dock |
| [cheatsheet.md](cheatsheet.md) | `modules/ii/cheatsheet` — the keybind list, and the periodic table `proscenio` leaves out |
| [overview.md](overview.md) | `modules/ii/overview` — the launcher and the workspace grid |
| [region-selector.md](region-selector.md) | `modules/ii/regionSelector` — region screenshots and recording |
| [wallpaper-selector.md](wallpaper-selector.md) | `modules/ii/wallpaperSelector` — the folder browser that picks a wallpaper |
| [settings.md](settings.md) | `modules/ii/settings` — the settings window, its navigation rail and page switching |
| [polkit.md](polkit.md) | `modules/ii/polkit` — the authentication agent and its password dialog |
| [keyring-prompt.md](keyring-prompt.md) | no qs counterpart — the gcr prompter for gnome-keyring and its dialog |
| [lock.md](lock.md) | `modules/ii/lock` — the session lock, its password field and the desktop behind it |
| [shortcuts.md](shortcuts.md) | the global shortcut names every panel is reached by |
| [compat.md](compat.md) | the layer that lets `proscenio` stand in for the QML shell, and how to build without it |
| [colors.md](colors.md) | `scripts/colors` — `switchwall.sh`, the palette generator and what follows a wallpaper change |
| [files.md](files.md) | where `proscenio` keeps its config, state and cache, and its other commands |
| [ipc.md](ipc.md) | the `ipc call` targets scripts and binds reach the shell through |
| [inventory.md](inventory.md) | every QML file in the shell, grouped, with what `proscenio` has of it |

## Parity

At the same configuration proscenio matches the QML shell in pixel sizes,
color expressions (the same formula over the same palette), animation
durations and curves, and response to input. Where the QML relies on a Qt
behavior with no GTK equivalent, the document describes the visible result,
and proscenio reproduces that result.
