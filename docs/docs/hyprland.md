---
title: Hyprland integration
sidebar_label: Hyprland integration
description: How proscenio talks to Hyprland through its IPC sockets and Wayland protocols, and how the settings window writes Hyprland's Lua config, monitor rules, window rules and hypridle.conf.
---

# Hyprland integration

proscenio runs only on Hyprland, and works with it in three ways: it sends requests and follows
events over Hyprland's IPC sockets, it speaks four Wayland protocols of Hyprland's own, and its
settings window writes Hyprland's Lua config and reads the live values back.

## IPC

Both sockets live in `$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/`. The code is in
`src/platform/hyprland/hypr.rs`.

### Requests

A request opens a connection to `.socket.sock`, writes the command and reads the reply until
Hyprland closes the connection. Each request has its own connection, and requests run on the main
thread, except the state snapshot described below.

| Request | Used for |
|---|---|
| `j/<query>` | Replies as JSON: `monitors`, `monitors all`, `workspaces`, `clients`, `activewindow`, `devices`, `binds`, `layers`, `cursorpos` |
| `getoption <name>` | The live value of one option; `j/getoption <name>` for JSON |
| `dispatch hl.dsp.…` | Every action on windows, workspaces, the cursor and the renderer |
| `eval <lua>` | Lua run in the live Hyprland, without writing a file |
| `reload` | Reloading the config after the settings window wrote it |
| `configerrors` | The errors of the last config load |

Dispatches use Hyprland's Lua dispatcher syntax: a function under `hl.dsp` called with a table of
arguments.

```
dispatch hl.dsp.focus({ window = "address:0x55d0c3a1b2c0" })
dispatch hl.dsp.focus({ workspace = 3 })
dispatch hl.dsp.window.move({ workspace = 4, follow = false, window = "address:0x55d0c3a1b2c0" })
dispatch hl.dsp.window.close({ window = "address:0x55d0c3a1b2c0" })
dispatch hl.dsp.cursor.move({ x = 960, y = 540 })
dispatch hl.dsp.workspace.toggle_special("special")
dispatch hl.dsp.force_renderer_reload()
```

`eval` changes the running Hyprland without touching its config. The proxy settings use it to set
the proxy variables with `hl.env`, at startup and whenever they change, so programs Hyprland starts
later inherit them. A screenshot that includes the pointer, and a focused fullscreen
window, use it to turn `cursor:hide_on_key_press` off for as long as they last.

A few features run `hyprctl` instead: switching every keyboard's layout
(`hyprctl switchxkblayout all <index>`), applying the cursor theme (`hyprctl setcursor`), and the
sidebar's button that reloads Hyprland and restarts the shell.

### Events

`.socket2.sock` streams one `EVENT>>DATA` line per event. The shell reads it asynchronously on the
main loop and hands each line to whoever subscribed to `Events`.

| Events | What follows |
|---|---|
| `workspace`, `focusedmon`, `createworkspace`, `destroyworkspace`, `moveworkspace`, `changeworkspaceid`, `renameworkspace`, `activespecial`, `openwindow`, `closewindow`, `movewindow`, `activewindow`, `fullscreen`, `monitoradded`, `monitorremoved`, `configreloaded`, with their `v2` forms where they have one | The state snapshot is read again |
| `urgent` | `src/services/hyprland/urgentwindows.rs` marks the window as wanting attention, and the [dock](dock.md) marks its app; `activewindowv2` for that window, or `closewindow`, clears the mark |
| `activelayout` | The keyboard layout button changes |
| `configreloaded` | The screen rounding is read again, and the shortcut editors read the binds again |
| `bell` | The screen flashes, when `accessibility.flashOnBell` is on |

### The state snapshot

`src/services/hyprland/hyprstate.rs` keeps one snapshot of Hyprland's state for the whole shell:
`monitors`, `workspaces` and `clients`. After each event in the first row above, it reads all three on a worker
thread and notifies its subscribers when the reply lands. Events that arrive while a read is under
way fold into one more read. The bar's workspace indicator, the wallpaper's
[parallax](background.md) and the fullscreen handling described in [Architecture](architecture.md)
read this snapshot.

## Wayland protocols

| Protocol | Code | Used for |
|---|---|---|
| `hyprland-global-shortcuts-v1` | `src/platform/wayland/shortcuts.rs` | Publishing every shell action as a global shortcut |
| `hyprland-focus-grab-v1` | `src/platform/wayland/grab.rs` | Closing a panel on a click outside it |
| `hyprland-toplevel-export-v1` | `src/platform/wayland/capture.rs` | Window previews in the overview and the dock |
| `hyprland-lock-notify-v1` | `src/platform/wayland/locknotify.rs` | Following the session's lock state |
| `linux-dmabuf-v1` | `src/platform/wayland/capture.rs` | Receiving those previews as GPU buffers |

The XML is vendored in `protocols/` and turned into client code at build time with `wayland-scanner`.
The shortcuts and the lock notifications open a Wayland connection of their own; the focus grab and
the previews use GTK's connection with an event queue of their own. Each socket is watched on the
GLib loop. The layer surfaces and the lock screen come through gtk4-layer-shell, which speaks
`wlr-layer-shell-unstable-v1` and `ext-session-lock-v1`.

### Global shortcuts

Every action the shell registers in `src/core/actions.rs` is published with the app id `proscenio`,
its name and a description. A Hyprland bind fires one with `hl.dsp.global("proscenio:<name>")`.
Both the press and the release arrive, so an action can also do something on release: the workspace
numbers show while their key is held. [Shortcuts](shortcuts.md) lists the actions.

### Focus grab

Each panel that closes on an outside click holds a grab with its own surface, plus the on-screen
keyboard's while it is open, and closes when Hyprland clears the grab.
[Architecture](architecture.md) has the details.

### Window previews

A preview asks Hyprland to capture one window, named by its address. When the compositor offers a
format and modifier that both GTK and the GPU can sample, checked through Vulkan, the frame arrives in
a GPU buffer allocated with GBM; otherwise it arrives in shared memory. A preview that stays open asks
for the next frame at most 30 times a second.

### Lock state

Hyprland reports when the session locks and unlocks. Once the shell's own [lock screen](lock.md)
holds the lock and Hyprland then reports the session unlocked, the shell releases its lock as well.

## Writing Hyprland's config

The settings window keeps everything it changes in Hyprland's config in one Lua file per area, under
`~/.config/hypr/settings/`. Hyprland applies them only if its config loads that folder;
[Building and installing](installing.md) shows how, loading it last so that a change made in the
settings window wins. The code is in `src/platform/hyprland/hyprconfig.rs` and the modules next to it.

A change reads the area's file, edits its text and writes it back. When the file exists but cannot
be read, or is read-only, nothing is written. The new text goes to `<file>.part` next to the file,
following a symbolic link to its target, with the file's permissions, is flushed to disk and then
replaces the file in one rename, so a failed write leaves the old file whole. When the file's folder
does not allow creating `<file>.part`, the file is written in place. Moving lines between the area
files at start writes nothing while any of them cannot be read.

| File | Holds |
|---|---|
| `appearance.lua` | Window options (borders, rounding, blur, shadows, dimming, opacity), the border color lines, the shadow color line, the fullscreen window rules, the cursor and icon theme variables, the workspace group rules |
| `displays.lua` | HDR, VRR, tearing and XWayland scaling, the monitor rules and the primary display |
| `multitasking.lua` | Layout, gap, snapping, focus, workspace and workspace swipe options, and smart gaps |
| `keyboard.lua` | Layouts, variants, XKB options, Num Lock at start, binds resolved by symbol |
| `accessibility.lua` | Animations, key repeat, zoom, the screen shader, the bell sound |
| `mouse.lua` | Pointer and touchpad options, and gestures |
| `devices.lua` | Per-device input settings |
| `apps.lua` | Window rules |
| `binds.lua` | The workspace number and arrow binds, then the changed shortcuts; see [Workspace groups](#workspace-groups) |
| `other.lua` | Anything that belongs to no page |

### Options

Each option is one line in its area's file. The option's path becomes nested tables, and a dash in a
name becomes an underscore:

```lua
hl.config({ general = { gaps_in = 4 } })
hl.config({ input = { touchpad = { tap_to_click = true } } })
hl.config({ general = { layout = "master" } })
```

`true`, `false` and numbers are written bare, anything else as a quoted string. Setting an option
replaces its line, or appends one if the file has none, and leaves every other line as it is, so the
files can be edited by hand.

A page reads its options' live values with `j/getoption`, taking whichever of the `int`, `float`,
`bool`, `str`, `css` or `vec2` fields the reply carries. Changes made in quick succession are written
together 50 ms after the last one, followed by a `reload`, and the page then reads the values again.

### Line sets

Some switches add or remove a fixed set of lines, and show as on only when every line is present.

| Switch | Lines |
|---|---|
| Smart gaps (`multitasking.lua`) | Workspace rules that set no gaps on the workspaces matching `w[tv1]` and `f[1]`, and window rules that take the border and rounding off tiled windows there |
| Keep fullscreen windows opaque (`appearance.lua`) | `hl.window_rule({ name = "opaque-fullscreen", match = { fullscreen = true }, opacity = "1 override 1 override" })` |
| Keep fullscreen windows undimmed (`appearance.lua`) | `hl.window_rule({ name = "no-dim-fullscreen", match = { fullscreen = true }, no_dim = true })` |

### Border colors

Window border colors come from the wallpaper palette, and are written as guarded lines in
`appearance.lua`:

```lua
if border_colors and border_colors.primary then hl.config({ general = { col = { active_border = "rgba(" .. border_colors.primary .. "CC)" } } }) end
```

`border_colors` is a Lua table of six-digit hex colors that the Hyprland config has to define; the
guard leaves the border alone when the table or the key is missing. The settings window picks the key
(`active` or `inactive` by default, or `outline`, `outline_variant`, `primary`, `secondary` or
`tertiary`) and the two-digit alpha after it. Until a line exists, it starts from `active` at `77`
for the focused window's border and `inactive` at `33` for the others.

### Shadow color

The shadow color is one plain line in `appearance.lua`; the settings window stores only the two-digit
alpha, and the color is always black:

```lua
hl.config({ decoration = { shadow = { color = "rgba(00000080)" } } })
```

Until the line exists, the settings window shows `20`, the alpha of the dotfiles' shadow color.

### Workspace groups

Each display gets its own group of `bar.workspaces.shown` workspaces, the number of cells the bar's
[workspace row](workspaces.md) shows. The shell writes the rules that pin each group to its display
and the number and arrow binds that follow the groups.

The rules are a block in `appearance.lua`. With 5 and two displays:

```lua
hl.workspace_rule({ workspace = "1", monitor = "DP-1", persistent = true, default = true })
hl.workspace_rule({ workspace = "2", monitor = "DP-1", persistent = true })
-- … 3 to 5 on DP-1
hl.workspace_rule({ workspace = "6", monitor = "DP-2", persistent = true, default = true })
-- … 7 to 10 on DP-2
```

`default` makes the group's first workspace the one the display starts on. Hyprland skips the rules
of a display that is not connected.

The binds are a block at the top of `binds.lua`, between the lines `-- proscenio workspace keys` and
`-- end proscenio workspace keys`; the `shortcut` lines of the Shortcuts page follow it. With 5 and
two displays it holds the group size, a table of each display's first workspace, and the binds:

```lua
do
    local size = 5
    local starts = { ["DP-1"] = 1, ["DP-2"] = 6 }
    -- … find_group_start, find_workspace_in_group, find_adjacent_workspace, send and focus,
    -- … then four loops of five binds, one loop of the arrow binds and the Up and Down binds
end
```

The number binds are Super with the keys 1 to 5 and with the matching keypad key codes, described
`Workspace: Focus <n>`, and the same with Super and Alt, described `Window: Send to workspace <n>`,
which moves the focused window without following it. Group sizes above 10 get ten keys, 1 to 9 and
0. The arrow binds are Ctrl + Super with Left, H, Right and L, described
`Workspace: Focus left adjacent` and `Workspace: Focus right adjacent`, and the same with Alt,
described `Window: Send to left adjacent workspace and follow` and `Window: Send to right adjacent
workspace and follow`, which moves the focused window and follows it. Ctrl + Super + Up and
Ctrl + Super + Down go to the group's first and last workspace, described
`Workspace: Focus first in group` and `Workspace: Focus last in group`.

When a key is pressed, the group is the one of the focused display: `starts[<focused display>]` up
to that plus the size minus 1. Number key n goes to the nth workspace of the group, so Super + 3 on
DP-2 goes to workspace 8 whichever workspace DP-2 shows. Left, H, Right and L go to the previous
or next workspace of the group and wraps at its ends. When the display shows a workspace outside its group,
Right and L go to the group's first workspace and Left and H to its last. A display without an entry
uses the group of the focused workspace, `floor((id - 1) / size) * size + 1`. A config that loads
`settings/` and defines `shortcut` gets the binds without anything else.

The shell rewrites the rules and the binds one second after it starts, after `bar.workspaces.shown`
changes, and after Hyprland's `monitoradded`, `monitoraddedv2` and `configreloaded` events, counting
the second from the last of several in a row, and reloads Hyprland only when a text changed. It
writes nothing while Hyprland does not answer the `monitors all` request. The rules are written
first, the binds after them. Both blocks are rewritten in place; the binds block returns to the top
of `binds.lua` if it was moved. Only lines of exactly the shape above belong to the rules; a
workspace rule with other keys is left alone. A leftover `settings/workspaces.lua` is deleted.

Hyprland does not switch a display to its `default` workspace on reload. After the reload that
follows a changed block, the shell switches every display that is on and not mirroring, and whose
active workspace lies outside its own group, to the first workspace of that group, then focuses the
display that had focus. Special workspaces are not touched, and a plain `configreloaded` moves
nothing.

A display keeps the group its rules already give it, read back from the block, also while it is
unplugged, off or mirroring. Displays without one get the next groups: the main display
(`WAYLANDDRV_PRIMARY_MONITOR` in `displays.lua`, when it is on) first, then the others by connector
type and number, the order the Displays page uses to pick a main display. A display that is off or
mirrors another one gets no new group. The code is in `src/platform/hyprland/workspacegroups.rs`,
`src/platform/hyprland/workspacekeys.rs` and `src/services/hyprland/workspacegroups.rs`.

### Monitor rules

`displays.lua` holds one block per display:

```lua
hl.monitor({
	output = "DP-1",
	mode = "2560x1440@144.00",
	position = "0x0",
	scale = 1,
	transform = 0,
	bitdepth = 10,
	cm = "srgb",
})
hl.env("WAYLANDDRV_PRIMARY_MONITOR", "DP-1")
```

A change rewrites only the keys it touches inside the display's block, or adds a block for a display
that has none. Each write carries the display's whole running state, its mode, position, scale,
transform, bit depth and color mode, so the rule never leaves Hyprland to guess the rest. Turning a
display off writes `disabled = true`. The primary display is the `WAYLANDDRV_PRIMARY_MONITOR`
variable, and moving displays around writes their positions relative to it, so the primary display
stays at `0x0`. After each write the shell reloads Hyprland and reads `j/monitors all` again.

### Window rules

`apps.lua` holds window rules, one per line, matched on the window class:

```lua
hl.window_rule({ match = { class = "^(kitty)$" }, float = true })
hl.window_rule({ match = { class = "^(steam)$" }, workspace = "3" })
```

The settings window lists only the lines in exactly this shape, and leaves other lines in the file
alone. Adding a rule for a class and a rule name it already has replaces that line. The class is
written with regular expression characters escaped and the value as an escaped Lua string, so a
class matches only itself: `org.kde.dolphin` becomes `"^(org\\.kde\\.dolphin)$"`.

### Devices, gestures, shortcuts and environment

| What | Where | Line |
|---|---|---|
| One input device | `devices.lua` | `hl.device({ name = "<device>", <key> = <value> })`, one line per setting |
| Gestures | `mouse.lua` | `hl.gesture({ … })` |
| Shortcuts | `binds.lua` | `shortcut("<Category>: <Name>", "<keys>", "<keys>")`, one per changed shortcut |
| Cursor and icon themes | `appearance.lua` | `hl.env("XCURSOR_THEME", …)`, `hl.env("XCURSOR_SIZE", …)`, `hl.env("QT_ICON_THEME", …)`, and a `hyprland.start` hook that runs `hyprctl setcursor` |

The gestures page also lists the gestures in `~/.config/hypr/hyprland/general.lua`; removing one of
those writes the same gesture with `action = "unset"`. After a gesture change the shell reloads
Hyprland and reads `configerrors`: if a fresh error names `hl.gesture`, it puts the previous file back
and reloads again.

A shortcut line names the shortcut by its description and gives up to two key combinations, with
`false` for an empty slot. `shortcut` is not part of Hyprland's API: the Hyprland config that loads
`binds.lua` has to define it. The shortcut editors read the current binds from `j/binds`, skipping
binds without a description and binds inside a submap.

### At startup

Before anything else reads them, the shell tidies the files:

1. If `~/.config/hypr/settings.lua` exists, its statements are split into the area files by what they
   are: options go to their page's file, monitor rules to `displays.lua`, window rules to `apps.lua`,
   and so on. The file is then renamed to `settings.lua.bak`.
2. An option line found in a file other than its page's moves to that page's file, unless that file
   already sets the option.

## hypridle.conf

Idle timeouts go to `~/.config/hypr/hypridle.conf`, the file `hypridle` reads. The shell edits that
file, and when it does not exist creates it with the written keys and, unless they set one, a
`lock_cmd` that dispatches the `proscenio:lock` global. It finds each `listener` block by what its
`on-timeout` command does:

| Timeout | Recognized by | Written as |
|---|---|---|
| Screen off | a command containing `dpms` | `hyprctl dispatch 'hl.dsp.dpms({ action = "disable" })'`, with the matching `enable` as `on-resume` |
| Lock | a lock command: `loginctl lock-session`, `hyprlock`, `swaylock`, `gtklock`, `waylock`, `$lock_cmd`, `proscenio ipc call lock activate` or the `proscenio:lock` global | `$lock_cmd` if the file defines it, or `loginctl lock-session` |
| Suspend | a command containing `suspend` | `$suspend_cmd` if the file defines it, or `systemctl suspend \|\| loginctl suspend` |

Changing a timeout rewrites the `timeout` line of its block, a timeout of zero removes the block, and
a missing block is added at the end. The settings also set keys in the `general` block:
`before_sleep_cmd = loginctl lock-session` to lock before sleep, and `ignore_dbus_inhibit`,
`ignore_systemd_inhibit` and `ignore_wayland_inhibit` set to `true` when idle should ignore those
inhibitors. After a write the shell restarts `hypridle.service` with `systemctl --user`.
