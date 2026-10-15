---
title: Sound settings
sidebar_label: Sound
description: The Sound page and its Volume Levels and Sound cards subpages, which pick the default devices, set volumes per device and per app, choose card profiles, and set up alert sounds and volume protection.
---

# Sound settings

The Sound pages control the sound server through the PulseAudio client library, which a PipeWire
session serves with `pipewire-pulse`. When no such server is running, every Sound page is only a
notice saying so.

## Sound

**Output** and **Input** each have:

| Control | Does |
|---|---|
| Device | the default output (sink) or input (source); a device whose description starts with its short name shows as "*short name* · *the rest*" |
| Volume | the default device's volume, 0 to 100 % |
| Mute | mutes the default device |

Two link rows follow: **Volume Levels**, the volume of each app playing or recording, and **Sound
cards**, which input and output configuration each card uses.

**Alert Sound** chooses which events play a sound. Its settings, and those of Earbang protection
below, are keys in `~/.config/proscenio/config.toml`.

| Control | Plays when | Key, default |
|---|---|---|
| Battery | the battery runs low or critical, is full, or the charger is plugged in or out | `sounds.battery`, off |
| Pomodoro | a Pomodoro timer runs out | `sounds.pomodoro`, off |
| Microphone | the microphone is muted or unmuted | `sounds.microphone`, on |
| USB devices | a USB device is connected or disconnected | `sounds.devices`, on |
| Sound theme | one of the themes in `/usr/share/sounds` that has a `stereo` folder | `sounds.theme`, `freedesktop` |

An alert plays the theme's `.oga` file, or its `.ogg`, with `paplay`. With no sound theme installed a
notice says that alerts stay silent; the default theme comes with the sound-theme-freedesktop
package.

**Earbang protection** guards the default output against sudden loud sound:

| Control | Does | Key, default |
|---|---|---|
| Enable | turns the protection on | `audio.protection.enable`, off |
| Max allowed increase | a volume step larger than this, in percent, is undone | `audio.protection.maxAllowedIncrease`, 10 |
| Volume limit | the volume is held at or below this percent | `audio.protection.maxAllowed`, 99 (0 to 154) |

The two numbers are inactive while protection is off.

## Volume Levels

**Playback** lists the apps playing sound and **Recording** the apps recording it (PulseAudio's sink
inputs and source outputs), or "Nothing is playing" and "Nothing is recording".

Each entry shows the app's icon, its name and a volume slider from 0 to 100 %. The name is followed
by what it plays when that differs from the name: the title of a media player belonging to the same
process or one of its parents, a playing one first, else the stream's own media name. Clicking the
icon mutes or unmutes the stream; a muted stream's icon is dimmed under a crossed-out speaker or
microphone.

The secondary action of the sidebar's audio and microphone [quick toggles](quick-toggles.md) opens
this page.

## Sound cards

One drop-down per sound card, titled with the card's description, lists the card's available
profiles, the input and output configurations PipeWire can use with it, in the order
`pactl list cards` gives them. Picking one runs `pactl set-card-profile <card> <profile>`, and the
list is read again afterwards.
