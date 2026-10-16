---
title: Volume mixer
sidebar_label: Volume mixer
description: The panel opened from the bar that picks the output device, sets the master volume and the volume of every app playing sound, with the shortcuts and IPC that drive it.
---

# Volume mixer

A panel for the sound that is playing now: the output device, the master volume with its mute button,
and one row per app playing sound, each with its own slider and mute button. It talks to the sound
server through the PulseAudio client library, as the [Sound settings](settings-sound.md) do, and
exists only when such a server was running as the shell started.

The panel is `src/panels/mixer/`; the rows are the ones of Volume Levels, from
`src/panels/sound/streams.rs`, and the device picker is `src/panels/sound/devices.rs`.

## Opening and closing

- A left click on the mixer ring of the [bar](bar.md) toggles the panel.
- The global shortcuts `volumeMixerToggle`, `volumeMixerOpen` and `volumeMixerClose` (see
  [Shortcuts](shortcuts.md)) act on the focused monitor.
- Over [IPC](ipc.md), the `volumeMixer` target has `toggle`, `open` and `close`, which also act on
  the focused monitor.
- A click anywhere outside the panel, or `Escape`, closes it.

With a horizontal bar the panel opens just past the bar, centered on the mixer ring and kept inside
the screen. With a vertical bar it opens beside the bar, level with the top edge of the ring, and
moves up only as far as it needs to stay inside the screen as the panel grows.

## What it shows

From top to bottom:

| Part | What it does |
|---|---|
| Device | The default output (sink). Clicking it unfolds the list of outputs inside the panel; picking one makes it the default and folds the list. A device whose description starts with its short name shows as "*short name* · *the rest*". |
| Volume | The default output's volume, 0 to 100 %. The speaker beside it mutes or unmutes the output. |
| Apps | One row per app playing sound, as on the Volume Levels page: its icon, its name and what it plays, a volume slider, and a click on the icon to mute it. *Nothing is playing* while there are none. |
| Volume Levels | Closes the panel and opens the Volume Levels page of the [Sound settings](settings-sound.md). |

The window is as tall as its content, so it grows and shrinks as apps start and stop or the device
list unfolds. The app list scrolls once it is taller than 320 px. How a row names what it plays is
described under [Volume Levels](settings-sound.md#volume-levels). The device list folds again
whenever the panel closes.

## Staying current

While the panel is open, a change to the server, such as an app starting or stopping, a volume moved
elsewhere or another default device, and a change to the media players refreshes it. A row stays in
place while the same apps play in the same order, so only the values change. While the panel is
closed it reads nothing.
