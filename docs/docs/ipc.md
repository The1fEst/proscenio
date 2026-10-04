---
title: IPC
sidebar_label: IPC
description: Calling the running shell from scripts with proscenio ipc, the D-Bus interface behind it, and every target and function it answers.
---

# IPC

Scripts, key binds and other programs reach the running shell through `proscenio ipc`:

```bash
proscenio ipc show
proscenio ipc call <target> <function> [arguments…]
```

`show` lists every target and its functions. `call` runs one function, with exactly as many arguments
as it takes:

```bash
proscenio ipc call sidebarRight toggle
proscenio ipc call settings openPage sound
proscenio ipc call wallpapers apply ~/Pictures/Wallpapers/dunes.jpg
```

The client starts no GTK and builds no window: it makes one D-Bus call to the shell, prints the reply
and exits. It waits at most five seconds.

## Output and exit status

| Exit status | When |
|---|---|
| 0 | The shell answered, even if it answered with an error |
| 1 | The command line is not `show` or `call …`; the usage goes to stderr |
| 255 | No shell answered; `No running instance of proscenio.` goes to stderr |

`proscenio ipc show >/dev/null` is therefore a cheap test for a running shell. Every function returns
nothing, so a successful `call` prints nothing. A call the shell cannot run prints one of these on
stderr and still exits with 0:

| Message | Cause |
|---|---|
| `Target required to send message.` | `call` with no target |
| `Function required to send message.` | A target with no function |
| `Target not found.` | No such target |
| `Function not found.` | No such function on that target |
| `Too few arguments provided (1 required but 0 were provided.)` | Fewer arguments than the function takes; a second line gives the function's definition |
| `Too many arguments provided (…)` | More arguments than the function takes, with the same second line |

`show` prints each target as `target <name>` and each of its functions, indented, as
`function <name>(<parameters>): void`:

```text
target settings
  function open(): void
  function openPage(page: string): void
  function close(): void
  function toggle(): void
```

## The D-Bus interface

The shell exports the interface on the session bus, under its application ID:

| Part | Name |
|---|---|
| Bus name | `dev.fEst.Proscenio` |
| Object path | `/dev/fEst/Proscenio` |
| Interface | `dev.fEst.Proscenio.Ipc` |

| Method | Signature | Returns |
|---|---|---|
| `Show` | `() → (s targets)` | The text `ipc show` prints |
| `Call` | `(as arguments) → (s output, s error)` | `arguments` is what follows `call` on the command line; `error` is one of the messages above, or empty |

Any D-Bus client can call it:

```bash
gdbus call --session --dest dev.fEst.Proscenio --object-path /dev/fEst/Proscenio \
    --method dev.fEst.Proscenio.Ipc.Call "['settings', 'openPage', 'sound']"
```

The `proscenio ipc` client calls without auto-start, so it never starts a shell.

## Targets

Functions of panels that exist on every monitor act on the monitor Hyprland reports as focused, or
the first monitor when there is none.

| Target | Function | Effect |
|---|---|---|
| `bar` | `toggle`, `open`, `close` | Shows or hides the [bar](bar.md) on every monitor |
| `sidebarRight` | `toggle`, `open`, `close` | The [sidebar](sidebar.md) |
| `session` | `toggle`, `open`, `close` | The [session screen](session-screen.md) |
| `calendar` | `toggle`, `open`, `close` | The [calendar](calendar.md), opened at the bar's clock |
| `cheatsheet` | `toggle`, `open`, `close` | The [cheatsheet](cheatsheet.md) |
| `mediaControls` | `toggle`, `open`, `close` | The [media controls](media-controls.md); opening them also hides the notification popups |
| `search` | `toggle`, `open`, `close` | The [overview](overview.md) with its launcher |
| | `workspacesToggle`, `workspacesClose` | The same as `toggle` and `close` |
| | `clipboardToggle` | Toggles the overview with the clipboard prefix typed in |
| | `emojiToggle` | Toggles the overview with the emoji prefix typed in |
| `osdVolume` | `trigger` | Shows the on-screen [indicator](osd.md), volume unless another one was shown last, for `osd.timeout` milliseconds, 1000 by default |
| | `toggle` | Shows the indicator until the next `toggle`, or hides it |
| | `hide` | Hides it |
| `region` | `screenshot`, `record`, `recordWithSound` | Opens the [region selector](region-selector.md) to take a screenshot or record, with or without sound |
| `mpris` | `playPause`, `previous`, `next` | The active player: the one playing most recently, else the first. `next` seeks to the end of the track when the player cannot skip but can seek |
| | `pauseAll` | Pauses every player that can pause |
| `brightness` | `increment`, `decrement` | The focused monitor's brightness, in steps of 5 %. At zero, `decrement` dims further through `hyprsunset`'s gamma, down to 25 %, and `increment` brings the gamma back to 100 % before it raises the brightness |
| `theme` | `toggleLightDark` | Switches between dark and light mode with `proscenio switchwall --noswitch`; see [Colors from the wallpaper](colors.md) |
| `cliphistService` | `update` | Reads the clipboard history from `cliphist` again |
| `wallpaperSelector` | `toggle` | The [wallpaper selector](wallpaper-selector.md) |
| | `random` | Applies a random wallpaper from the selector's folder |
| `wallpapers` | `apply(path: string)` | Sets `path` as the wallpaper and recolors, keeping the mode |
| `settings` | `toggle`, `open`, `close` | The [settings](settings.md) window |
| | `openPage(page: string)` | Opens the settings at a page |
| `osk` | `toggle`, `open`, `close` | The [on-screen keyboard](on-screen-keyboard.md) |
| `lock` | `activate` | Locks the session with the [lock screen](lock.md), or with `hyprlock` when `lock.useHyprlock` is on and it is installed |
| | `focus` | Gives the lock screen the keyboard again, for after resuming |
| `welcome` | `toggle`, `open`, `close` | The [welcome window](first-run.md) |
| `renderer` | `keep` | Keeps the renderer on trial |
| | `revert` | Goes back to the renderer before it and restarts the shell |
| | `reset` | Goes back to Cairo and restarts the shell |

`renderer reset` is the way out from a terminal when a renderer leaves the screen unusable.

### Settings pages

`settings openPage` takes a page ID. These open a page of the sidebar:

`quick`, `wifi`, `network`, `bluetooth`, `displays`, `sound`, `power`, `multitasking`, `appearance`,
`apps`, `notifications`, `search`, `mouse`, `keyboard`, `devices`, `accessibility`, `privacy`,
`system`

These open a page inside one of them, with the way back to it:

| Under | Page IDs |
|---|---|
| `wifi` | `savednetworks`, `hiddennetwork`, `hotspot` |
| `network` | `connection` (and inside it `ipv4`, `ipv6`, `eap`), `proxy`, `firewall` |
| `displays` | `displaycolor`, `nightlight` |
| `sound` | `volumelevels`, `soundcards` |
| `multitasking` | `overview`, `swiping` |
| `appearance` | `background`, `colors`, `fonts`, `windows`, `bar` (and inside it `utilitybuttons`, `barworkspaces`), `panels` (and inside it `dock`, `sidebars`, `cheatsheet`) |
| `apps` | `filetypes`, `windowrules` |
| `mouse` | `mousedevice`, `touchpad` |
| `keyboard` | `shortcuts`, `keyoptions` |
| `privacy` | `lock`, `capture` |
| `system` | `region`, `datetime`, `users`, `autostart`, `about`, `services`, `advanced` |

An unknown ID opens the settings where they were.

## From Hyprland

A bind runs a call like any other command:

```lua
hl.bind("XF86MonBrightnessUp", hl.dsp.exec_cmd("proscenio ipc call brightness increment"),
    { locked = true, repeating = true })
hl.bind("XF86MonBrightnessDown", hl.dsp.exec_cmd("proscenio ipc call brightness decrement"),
    { locked = true, repeating = true })
```

Most panels also answer a global shortcut, which needs no process per key press; see
[Global shortcuts](shortcuts.md).
