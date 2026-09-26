# Global shortcuts

The shell registers its keybinds with `hyprland_global_shortcuts_v1`, not with
Hyprland's config: each `GlobalShortcut { name: "…" }` becomes a registered
shortcut under the app id **`quickshell`**, and the Hyprland config
triggers it with

```
bind = Super, Slash, global, quickshell:cheatsheetToggle
```

Every panel in the shell is reachable this way, and several — the cheatsheet,
the overview, the wallpaper and region selectors — have no other entry point
at all.

## The names

| Name | Owner |
| --- | --- |
| `barToggle` `barOpen` `barClose` | the bar |
| `sidebarRightToggle` `sidebarRightOpen` `sidebarRightClose` | the right sidebar |
| `calendarToggle` | the calendar panel |
| `sessionToggle` `sessionOpen` `sessionClose` | the session screen |
| `mediaControlsToggle` `mediaControlsOpen` `mediaControlsClose` | the media panel |
| `osdVolumeTrigger` `osdVolumeHide` | the on-screen display |
| `cheatsheetToggle` `cheatsheetOpen` `cheatsheetClose` | the cheatsheet |
| `searchToggle` `searchToggleRelease` `searchToggleReleaseInterrupt` `overviewWorkspacesToggle` `overviewWorkspacesClose` `overviewClipboardToggle` `overviewEmojiToggle` | the overview |
| `oskToggle` `oskOpen` `oskClose` | the on-screen keyboard |
| `regionScreenshot` `regionRecord` `regionRecordWithSound` `recordStop` | the region selector |
| `wallpaperSelectorToggle` `wallpaperSelectorRandom` | the wallpaper selector |
| `lock` `lockFocus` | the lock screen |
| `workspaceNumber` | `GlobalStates` |
| `micMuteToggle` | `Audio`: mutes or unmutes the default source |
| `brightnessIncrease` `brightnessDecrease` | `Brightness` |
| `xkbLayoutNext` | `shell.qml`: `HyprlandXkb.cycleLayout()`, the next layout on every keyboard |

---

**Status (proscenio).** The names above are actions in `src/core/actions.rs`:
`src/main.rs` registers one handler per name once, not per monitor, and each
one acts on the surfaces belonging to the monitor Hyprland reports as focused,
falling back to the first. Buttons inside `proscenio` that stand for one of
them (the dock's overview button, the screen snip, the wallpaper launcher
action) run the action directly instead of going through Hyprland.

`src/platform/shortcuts.rs` publishes every action with the same protocol
under the app id **`proscenio`**, so a bind reads
`hl.dsp.global("proscenio:cheatsheetToggle")`. The compat layer
([compat.md](compat.md)) publishes them a second time under `quickshell`,
which is what the binds in the dots use.

Registering an id that is already taken is a **fatal protocol error**: the
compositor disconnects the client. The compat layer therefore leaves
`quickshell` alone while a `qs` or `quickshell` process is running.
`proscenio`'s own names never collide with the QML shell.

Every name in the table is wired except `searchToggleRelease`,
`searchToggleReleaseInterrupt`, `brightnessIncrease` and
`brightnessDecrease`; the brightness keys in the dots use
`ipc call brightness`.

Whenever the default source's mute changes, from the shortcut or anywhere
else, a transient "Microphone: Muted/Unmuted" notification appears with
`micgate.png`, and with `sounds.microphone` on (the default) the sound theme's
`device-removed` or `device-added` plays. The first three seconds after start
stay quiet, so the source appearing at start does not count as a change.

Several dots binds come in pairs: the global, and a fallback such as
`shellIsAlive || hyprctl switchxkblayout all next` for when no shell runs.
`shellIsAlive` asks both shells through their IPC (`TEST_ALIVE`, see
[ipc.md](ipc.md)), so under `proscenio` only the global fires.
