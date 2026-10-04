---
title: Global shortcuts
sidebar_label: Global shortcuts
description: The global shortcuts proscenio registers with Hyprland through hyprland-global-shortcuts-v1, what each one does, and how to bind them in Hyprland's Lua config or fire them from a script.
---

# Global shortcuts

proscenio binds no keys itself. At start, it registers its actions with Hyprland through the
`hyprland-global-shortcuts-v1` protocol, under the app ID `proscenio`, and Hyprland's config decides
which keys fire them. Hyprland lists what is registered:

```bash
hyprctl globalshortcuts
```

```text
proscenio:oskToggle -> Toggles on screen keyboard on press
proscenio:oskOpen -> Opens on screen keyboard on press
…
```

## Binding a shortcut

In Hyprland's Lua config, the `global` dispatcher fires a shortcut by its full name,
`proscenio:<name>`:

```lua
hl.bind("SUPER + Slash", hl.dsp.global("proscenio:cheatsheetToggle"),
    { description = "Shell: Toggle cheatsheet" })
hl.bind("SUPER + A", hl.dsp.global("proscenio:searchToggle"), { description = "Shell: Toggle search" })
hl.bind("SUPER + N", hl.dsp.global("proscenio:sidebarRightToggle"))
hl.bind("Print", hl.dsp.global("proscenio:regionScreenshot"), { description = "Utilities: Screen snip" })
```

A bind's `description` is what the [cheatsheet](cheatsheet.md) shows for it; the text before the
first colon names the group it is listed under. Binds without a description stay out of the
cheatsheet.

A shortcut that acts on a panel acts on the monitor Hyprland reports as focused, or on the first
monitor when there is none.

### Press and release

Hyprland tells proscenio when a bound key goes down, and a bind with `release = true` tells it when
the key comes up. Only `workspaceNumber` uses the release, so it takes two binds per key:

```lua
for _, key in ipairs({ "SUPER_L", "SUPER_R" }) do
    hl.bind(key, hl.dsp.global("proscenio:workspaceNumber"), { ignore_mods = true, transparent = true })
    hl.bind(key, hl.dsp.global("proscenio:workspaceNumber"),
        { ignore_mods = true, transparent = true, release = true })
end
```

While Super is held, the workspace indicator shows numbers instead of app icons, and a bar set to
hide itself shows up.

### From a script

`hyprctl dispatch` fires a shortcut from anywhere, with the same `hl.dsp` expression a bind uses. In
`~/.config/hypr/hypridle.conf`, for instance:

```ini
general {
    lock_cmd = hyprctl dispatch 'hl.dsp.global("proscenio:lock")'
    after_sleep_cmd = hyprctl dispatch 'hl.dsp.global("proscenio:lockFocus")'
}
```

## The shortcuts

### Panels

| Name | Does |
|---|---|
| `barToggle`, `barOpen`, `barClose` | Shows or hides the [bar](bar.md) on every monitor |
| `sidebarRightToggle`, `sidebarRightOpen`, `sidebarRightClose` | The [sidebar](sidebar.md) |
| `calendarToggle` | The [calendar](calendar.md), centered on the monitor |
| `sessionToggle`, `sessionOpen`, `sessionClose` | The [session screen](session-screen.md) |
| `mediaControlsToggle`, `mediaControlsOpen`, `mediaControlsClose` | The [media controls](media-controls.md) |
| `cheatsheetToggle`, `cheatsheetOpen`, `cheatsheetClose` | The [cheatsheet](cheatsheet.md) |
| `oskToggle`, `oskOpen`, `oskClose` | The [on-screen keyboard](on-screen-keyboard.md) |
| `osdVolumeTrigger` | Shows the on-screen [indicator](osd.md) for its timeout |
| `osdVolumeHide` | Hides it |

### Overview and launcher

| Name | Does |
|---|---|
| `searchToggle` | The [overview](overview.md) with its launcher |
| `overviewWorkspacesToggle` | The same as `searchToggle` |
| `overviewWorkspacesClose` | Closes the overview |
| `overviewClipboardToggle` | The overview with the clipboard prefix typed in |
| `overviewEmojiToggle` | The overview with the emoji prefix typed in |

### Screenshots and recording

| Name | Does |
|---|---|
| `regionScreenshot` | The [region selector](region-selector.md), to take a screenshot |
| `regionRecord` | The region selector, to record without sound |
| `regionRecordWithSound` | The region selector, to record with sound |
| `recordStop` | Stops the running recording |

### Wallpaper

| Name | Does |
|---|---|
| `wallpaperSelectorToggle` | The [wallpaper selector](wallpaper-selector.md) |
| `wallpaperSelectorRandom` | Applies a random wallpaper from the selector's folder |

### Session and input

| Name | Does |
|---|---|
| `lock` | Locks the session with the [lock screen](lock.md) |
| `lockFocus` | Gives the lock screen the keyboard again, for after resuming |
| `workspaceNumber` | Shows workspace numbers while held |
| `micMuteToggle` | Mutes or unmutes the default microphone |
| `xkbLayoutNext` | Switches every keyboard to the next layout |

`micMuteToggle` exists when the shell reached the sound server at start. Whenever the default
microphone is muted or unmuted, from this shortcut or anywhere else, a short-lived notification says
so, and with `sounds.microphone` on, the default, the sound theme's `device-removed` or
`device-added` plays.

## Shortcuts and IPC

Most shortcuts have a twin among the [IPC](ipc.md) functions. `workspaceNumber`, `micMuteToggle`,
`xkbLayoutNext` and `recordStop` exist only as shortcuts; the settings window, the media players,
brightness and dark mode only through IPC, so a key reaches them with `hl.dsp.exec_cmd`:

```lua
hl.bind("XF86AudioPlay", hl.dsp.exec_cmd("proscenio ipc call mpris playPause"), { locked = true })
```

Buttons inside the shell that stand for a shortcut run the same action without going through
Hyprland: the bar's screen snip and keyboard buttons, the screenshot and on-screen keyboard
[quick toggles](quick-toggles.md), the dock's overview button, the launcher's `/wallpaper` action and
the welcome window's keybinds button.

## Registration

Each action is registered once, with its name as the shortcut ID, its description, and an empty
trigger description; Hyprland passes the pressed and released events back on the same Wayland
connection. An app ID and shortcut ID pair can be registered only once per compositor; a second
registration of the same pair is the protocol error `already_taken`.
