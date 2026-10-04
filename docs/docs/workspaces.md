---
title: Workspaces
sidebar_label: Workspaces
description: The workspace indicator in the middle of the bar, what its pills, dots, numbers and app icons mean, how it shows the special workspace, and how to click, scroll and hold Super on it.
---

# Workspaces

The middle group of the [bar](bar.md) is a row of workspace cells, one per workspace ID. A capsule
marks the active workspace, a tinted band joins the workspaces that hold windows, and each cell shows
a dot, a number or the icon of its biggest window.

Each bar follows the active workspace of its own screen.

## Which workspaces it shows

The row has `bar.workspaces.shown` cells (10 by default). It shows the block of that many IDs that
holds the screen's active workspace: with 10 cells, workspaces 1 to 10 while workspace 3 is active, 11
to 20 while workspace 13 is. Moving into another block brings in the whole next block.

Each cell is 26 px wide, so the default row is 260 px. On a [vertical bar](bar.md) the cells stack
into a column.

## Occupied workspaces

A workspace counts as occupied when Hyprland lists it. Hyprland also lists the active workspace when
it is empty, so the active workspace counts only while a window has focus.

Occupied workspaces sit on a band in the secondary container color at 60 % opacity. Neighbors join:
the band of an occupied cell reaches half a cell into an occupied neighbor, so a run of occupied
workspaces reads as one capsule. When a workspace fills or empties, its part of the band grows out of
the cell's center or shrinks back into it over 350 ms.

## The active workspace

A capsule in the primary color, 22 px thick, sits on the active workspace. When the active workspace
changes, its leading end moves there in 100 ms and its trailing end follows in 300 ms, so the capsule
stretches across the cells in between and then catches up.

The dot or number under the capsule is drawn in a color that reads on the primary color, and so is
any mark the capsule passes over while it stretches.

## The hover indicator

A second capsule in the primary color follows the pointer cell by cell, using the same stretch. It
never disappears: when the pointer leaves the row it slides back under the active workspace.

| State | Its opacity |
|---|---|
| resting | 16 % |
| pointer over the row | 24 % |
| button held | 34 % |

## Dots, numbers and app icons

Each cell shows a 4.7 px dot or a number, and the two fade into each other over 200 ms. A mark is in
the on-secondary-container color on an occupied workspace and in a dimmer color on an empty one.

A cell shows its number:

- while Super is held (see below), on every cell;
- with **Always show numbers** (`bar.workspaces.alwaysShowNumbers`) on, on every cell that has no app
  icon to show.

Otherwise, it shows a dot.

### Numbers

The label is the workspace ID, or the entry for that ID in `bar.workspaces.numberMap`: the first
entry labels workspace 1, the second workspace 2, and so on. An empty entry, or an ID past the end of
the list, falls back to the number. **Number style** on the settings page fills the map with one of
three sets:

| Number style | Labels |
|---|---|
| Normal | the IDs |
| Han chars | 一, 二, 三 … 二十 |
| Roman | I, II, III … XX |

Labels are 15 px tall, 2 px smaller per character past the first, except `10`, which keeps the full
size. **Nerd Font for workspace numbers** (`bar.workspaces.useNerdFont`) draws them in the Nerd Font
set under `appearance.fonts.iconNerd` instead of the main font.

### App icons

With **Show app icons** (`bar.workspaces.showAppIcons`, on by default), each workspace with windows
shows the icon of its biggest window, by area, as an 18 px circle over the cell's mark.

The icon comes from the window's class: a desktop entry whose ID or name matches it, then a short
table of known mismatches (Steam games, Minecraft, VS Code and a few others), then the icon theme
under several spellings of the class. `application-x-executable` stands in when nothing matches.

Icons are recolored toward the theme while keeping their own light and dark: 80 % of the way with
**Tint app icons** (`bar.workspaces.monochromeIcons`, on by default), 50 % with it off. The tint is
the on-secondary-container color in dark mode and the on-primary color in light mode.

## The special workspace

While the screen has a special workspace open, the regular cells blur, darken and shrink slightly,
and a primary-colored pill with the special workspace's name takes their place, without its
`special:` prefix. On a vertical bar the pill reads `S`. Pointing at the row brings the regular
cells back into focus for as long as the pointer stays; leaving blurs them again.

## Input

| Input | Action |
|---|---|
| Left click | focuses the workspace under the pointer |
| Right click | opens or closes the [overview](overview.md) |
| Back button (button 8) | toggles the special workspace named `special` |
| Wheel down | the next workspace on this monitor, empty ones included (`r+1`) |
| Wheel up | the previous one (`r-1`) |

The shell sends these to Hyprland's socket in its Lua dispatch syntax:

```text
hl.dsp.focus({workspace = 3})
hl.dsp.focus({workspace = "r+1"})
hl.dsp.workspace.toggle_special("special")
```

## Holding Super

Holding Super for `bar.autoHide.showWhenPressingSuper.delay` milliseconds (140 by default) switches
every cell to its number. App icons shrink to about 80 % and slide into the bottom-right corner of
their cell, hanging slightly past it, so the number shows beside them. Releasing Super puts
everything back.

This needs **Reveal bar and workspace numbers** (`bar.autoHide.showWhenPressingSuper.enable`, on by
default) and the `workspaceNumber` shortcut bound to Super on press and on release. The same hold
reveals an auto-hidden bar; [The bar](bar.md) shows the Hyprland binds.

## Settings

The switches are on **Appearance → Bar → Workspaces** in the [settings window](settings-appearance.md),
under `[bar.workspaces]` in `~/.config/proscenio/config.toml`.

| Setting | Key | Default |
|---|---|---|
| Always show numbers | `alwaysShowNumbers` | `false` |
| Show app icons | `showAppIcons` | `true` |
| Tint app icons | `monochromeIcons` | `true` |
| Nerd Font for workspace numbers | `useNerdFont` | `false` |
| Workspaces shown | `shown` | `10`, from 1 to 30 |
| Number style | `numberMap` | `[]` |

The Super hold delay and its switch live on **Appearance → Bar** under **Holding Super**. A change to
any of these rebuilds the bar.

## How it is drawn

`src/panels/bar/workspaces.rs` draws the whole indicator in one Cairo pass on a single widget, in
this order:

1. the occupancy band: each cell's capsule drawn opaque into a group, then the group painted at 60 %
   opacity, so overlapping capsules do not darken where they meet;
2. the active capsule;
3. the hover capsule;
4. the dots and numbers in their own colors;
5. the dots and numbers again, clipped to the active capsule, in the color that reads on it;
6. the app icons, each clipped to a circle.

For the special workspace, steps 1 to 6 render into an offscreen surface that three box-blur passes
soften, and the result is scaled and darkened under the name pill. The drawing may reach 32 px past
the widget on every side, so the blur is not cut off.

The workspace list, the clients, the monitors and the focused window come from
`src/services/hyprstate.rs`, which reads them from Hyprland's socket again whenever Hyprland reports a
workspace, window, monitor or focus change.
