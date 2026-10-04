---
title: On-screen display
sidebar_label: On-screen display
description: The small indicator that shows volume, brightness or gamma for a moment when one of them changes, and the volume protection message under it.
---

# On-screen display

When the volume, the screen brightness or the gamma changes, a small pill appears at the top of the
screen with an icon, the name, the value in percent and a progress bar, and goes away after a
second. The source is `src/panels/osd.rs`.

## What shows it

| Change | Indicator |
|---|---|
| The default output's volume or mute changes | Volume |
| A monitor's brightness changes | Brightness |
| The gamma changes | Gamma |
| Volume protection steps in | Volume, with the reason under it |

Volume changes count only after the shell has read the output once, so starting the shell or
switching the default output does not show it.

| Indicator | Icon | Value | Bar |
|---|---|---|---|
| Volume | `volume_off` when muted, else `volume_up` | the output volume | 0 to 100% |
| Brightness | `routine` while night light is on, else `light_mode` | the focused monitor's brightness | 0 to 100% |
| Gamma | `wb_twilight` | the `hyprsunset` gamma | 25 to 100% |

The brightness icon grows and turns as the level rises: from 20 pixels and upright at 0% to 30
pixels and upside down at 100%, easing over 400 ms.

**Brightness and gamma.** The `brightness` IPC functions `increment` and `decrement` and the
brightness scroll areas step the focused monitor's brightness by 5%. Stepping down past 0% dims the
screen further through `hyprsunset` instead, in 5% steps of gamma down to 25%; stepping up brings
the gamma back to 100% first, then raises the brightness. That is when the gamma indicator shows.

## Where and for how long

There is one indicator at a time, on the focused monitor. It is centered horizontally, one bar
height from the top edge, or from the bottom edge when `bar.bottom` is on. If the focus moves to
another monitor while it shows, it moves there.

It stays for `osd.timeout` milliseconds (default 1000), counted again from every change. The
setting is *Stays on screen for (ms)* on the **Notifications** page of the settings app, under
*On-screen display* (see [System settings](settings-system.md)).

It closes early when:

- the pointer touches it;
- the pointer leaves, or moves 20 pixels away from, the place where you scrolled to change the
  value: the bar's scroll areas and the [screen corners](screen-corners.md) close the indicator they
  opened this way.

The window exists only while the indicator shows: it is built on the first change and destroyed
when it closes. Each opening therefore starts at the current value, and only changes while it is
open are animated.

## Volume protection

With volume protection on, a volume change that is too large is undone and the indicator shows why,
in a box in the error color under the pill:

- *Illegal increment* when the volume jumps up by more than `audio.protection.maxAllowedIncrease`
  percent at once; the volume goes back to where it was.
- *Exceeded max allowed* when it goes over `audio.protection.maxAllowed` percent; the volume is set
  to the previous volume or the limit, whichever is lower.

| Setting | Key | Default |
|---|---|---|
| Enable | `audio.protection.enable` | off |
| Max allowed increase | `audio.protection.maxAllowedIncrease` | 10 |
| Volume limit | `audio.protection.maxAllowed` | 99 |

These are on the **Sound** page, under *Earbang protection* (see [Sound settings](settings-sound.md)).

## Shortcuts and IPC

| Global shortcut | IPC (`osdVolume`) | Action |
|---|---|---|
| `osdVolumeTrigger` | `trigger` | Show the indicator that changed last (volume after start-up) for the usual time. |
| `osdVolumeHide` | `hide` | Close it. |
| none | `toggle` | Close it if it shows; otherwise show it with no timeout. |

```bash
proscenio ipc call osdVolume trigger
```

See [Shortcuts](shortcuts.md) and [IPC](ipc.md).
