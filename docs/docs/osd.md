---
title: On-screen display
sidebar_label: On-screen display
description: The small indicator that shows volume, brightness, gamma, keyboard brightness, the lock keys, the keyboard layout, the microphone, the audio output or the power profile for a moment when one of them changes, and the volume protection message under it.
---

# On-screen display

When the volume, the screen brightness, the gamma, the keyboard brightness, a lock key, the keyboard
layout, the microphone mute, the default output or the power profile changes, a small pill appears at the top of the
screen with an icon and a name, usually with a value, and goes away after a second. The source is
`src/panels/osd.rs`.

![The volume indicator at 40 percent](/img/screenshots/osd.webp)

## What shows it

| Change | Indicator |
|---|---|
| The default output's volume or mute changes | Volume |
| A monitor's brightness changes | Brightness |
| The gamma changes | Gamma |
| The keyboard backlight's level changes, on a machine that has one | Keyboard brightness |
| Volume protection steps in | Volume, with the reason under it |
| Caps Lock or Num Lock is switched | Caps Lock or Num Lock |
| The keyboard switches to another layout | Keyboard layout |
| The default microphone is muted or unmuted | Microphone |
| The default output becomes another device | Audio output |
| power-profiles-daemon switches to another profile | Power profile |

Volume changes count only after the shell has read the output once, so starting the shell or
switching the default output does not show it. The same holds for the first layout, the first
profile and the first lock state the shell reads, and a layout without a name shows nothing.

| Indicator | Icon | Name | Value | Bar |
|---|---|---|---|---|
| Volume | `volume_off` when muted, else `volume_up` | Volume | the output volume | 0 to 100% |
| Brightness | `routine` while night light is on, else `light_mode` | Brightness | the focused monitor's brightness | 0 to 100% |
| Gamma | `wb_twilight` | Gamma | the gamma of the screen's color matrix | 25 to 100% |
| Keyboard brightness | `backlight_high` | Keyboard brightness | the keyboard backlight's level | 0 to 100% |
| Caps Lock | `shift_lock` when on, else `shift_lock_off` | Caps Lock | On or Off | none |
| Num Lock | `dialpad` | Num Lock | On or Off | none |
| Keyboard layout | `keyboard` | the layout's name, as the bar's layout button tooltip shows it | none | none |
| Microphone | `mic_off` when muted, else `mic` | Microphone | Muted, or the microphone volume while unmuted | 0 to 100% while unmuted |
| Audio output | `headphones` for a headphone, headset or hands-free form factor, `tv` for HDMI, `bluetooth_audio` for a Bluetooth device, else `speaker` | the device's description | none | none |
| Power profile | the icon of the [quick toggle](quick-toggles.md) | Power Saver, Balanced or Performance | none | none |

Only a mute change shows the microphone. A volume change does not, since apps adjust the
microphone gain on their own all the time.

The brightness icon grows and turns as the level rises: from 20 pixels and upright at 0% to 30
pixels and upside down at 100%, easing over 400 ms.

**Brightness and gamma.** The `brightness` IPC functions `increment` and `decrement` and the
brightness scroll areas step the focused monitor's brightness by 5%. Stepping down past 0% dims the
screen further through its color matrix instead, in 5% steps of gamma down to 25%; stepping up brings
the gamma back to 100% first, then raises the brightness. That is when the gamma indicator shows.

**Lock keys.** Hyprland has no event for the lock state. A block in `binds.lua` binds `Caps_Lock`
and `Num_Lock`, non-consuming and on release, with any modifiers held (see
[Lock keys](hyprland.md#lock-keys)). Each bind posts the custom event `proscenio-lock-key`, which
makes the shell read `j/devices`; it shows a lock only when
the main keyboard's `capsLock` or `numLock` differs from the last reading. A lock that the layout
does not produce as the keysym `Caps_Lock` or `Num_Lock`, for example Caps Lock remapped to Escape
by an XKB option, shows nothing.

**Microphone.** With `osd.microphone` on, the indicator replaces the notification that says the
microphone is muted or unmuted; with it off, the notification is sent as before. The alert sound
under `sounds.microphone` plays either way.

**Switches.** Each of the six new indicators has a switch, all on by default, in the settings app
under *On-screen display* (see [System settings](settings-system.md)).

| Indicator | Key |
|---|---|
| Caps Lock | `osd.capsLock` |
| Num Lock | `osd.numLock` |
| Keyboard layout | `osd.keyboardLayout` |
| Microphone | `osd.microphone` |
| Audio output | `osd.audioOutput` |
| Power profile | `osd.powerProfile` |

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
