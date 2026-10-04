---
title: Bluetooth and devices
sidebar_label: Bluetooth and devices
description: The Bluetooth page, which pairs and connects devices through BlueZ, and the Devices page, which turns individual input devices on or off in Hyprland.
---

# Bluetooth and devices

Two pages of the [settings window](settings.md) look after hardware: **Bluetooth** pairs and connects
wireless devices, and **Devices** chooses which input devices Hyprland listens to.

## Bluetooth

The Bluetooth page talks to BlueZ (`org.bluez`) on the system bus. When BlueZ is neither running nor
ready to start on demand, the page is only a notice; it comes with the bluez package and runs as
`bluetooth.service`. A second notice appears when `bluetoothctl` (bluez-utils) is missing, since
pairing a device that asks for confirmation then fails.

| Control | Does |
|---|---|
| Bluetooth | powers the adapter on or off (its `Powered` property) |

Without an adapter the page says "No Bluetooth Found", and with it powered off, "Bluetooth Turned
Off".

While the adapter is on, **Devices** lists what BlueZ knows: connected devices first, then paired
ones, then the rest, each group by name, with devices that have only an address last. A spinner beside
the title shows while discovery runs. The page keeps discovery going while it is open and nothing is
being set up, starting it again within a second whenever it stops, and ends it on leaving.

A device card shows its name, or its address, over its state: "Pairing…", "Connecting…",
"Connected", "Paired", "Not set up", or "Could not connect. Wake the device and try again" after a
failed connection. Its buttons:

| Button | Shown | Does |
|---|---|---|
| Pair | not paired | pairs, trusts and connects the device |
| Connect | paired, not connected | connects |
| Disconnect | connected | disconnects |
| Forget | paired | removes the device from the adapter (`RemoveDevice`) |

Pair, Connect and Disconnect are inactive while that device is pairing or connecting.

**Pairing** keeps `bluetoothctl --agent NoInputNoOutput` running for its length, so a device that
asks to confirm a code is answered. It stops discovery, pairs (giving up after 90 seconds), marks the
device trusted, waits 5 seconds for the bonding link to drop, and then tries to connect up to four
times, 1.5 seconds apart. **Connecting** stops discovery too and gives up after 15 seconds.

The sidebar's Bluetooth dialog lists the same devices; its **Details** button opens this page
([Sidebar](sidebar.md)).

## Devices

The Devices page turns individual input devices on or off. A mouse or keyboard often shows up as
several devices; turning off an extra one keeps it from taking over the keyboard layout. The
consumer control and system control devices are the ones that carry the media and power keys.

**Input devices** has one group per piece of hardware and a switch per device in Hyprland's
`devices` list (keyboards, mice, tablets, touch devices and switches), each with its kind's icon and
Hyprland's name for it. The main keyboard's row adds "main keyboard" and its active layout.

Devices are grouped by matching Hyprland's names to `/proc/bus/input/devices`. Devices whose
physical path is the same up to `/inputN` are one piece of hardware; devices without a physical path
are virtual and group by the first word of their name. A group is titled with the words its members'
names share. Hardware comes first, then virtual devices, each in order of title. The list is read
again every 2 seconds while the page is open.

A switch writes the device's `enabled` setting as a line in `~/.config/hypr/settings/devices.lua`,
then Hyprland reloads:

```lua
hl.device({ name = "at-translated-set-2-keyboard", enabled = false })
```

Turning a device back on writes `enabled = true` rather than removing the line, because Hyprland
keeps a device off after its `enabled = false` line goes away. The same file holds the per-device
settings of the This mouse only subpage on [Mouse & Touchpad](settings-input.md).
