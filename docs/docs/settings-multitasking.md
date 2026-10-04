---
title: Multitasking settings
sidebar_label: Multitasking
description: The Multitasking page and its Overview and Swiping between workspaces subpages, covering tiling layouts, gaps, snapping, focus, workspaces, the overview grid and workspace swipes.
---

# Multitasking settings

The **Multitasking** page sets how Hyprland lays out, moves, focuses and switches windows. Two
subpages hang off it: **Overview**, for the workspace grid of the [overview](overview.md), and
**Swiping between workspaces**, for the touchpad gesture that changes workspaces.

## Where the values go

Every setting on the Multitasking page and on the swiping subpage lives in Hyprland's config. All
but Smart gaps are Hyprland options: the page reads each from the running compositor with
`getoption`, and a change becomes one line of `~/.config/hypr/settings/multitasking.lua`:

```lua
hl.config({ general = { gaps_in = 5 } })
```

A line that already sets the option is replaced in place; otherwise the line is appended. Changes
made within 50 ms of each other are written together, then Hyprland reloads its config. Hyprland
applies the file only if its config loads the `settings` folder, as
[Building and installing](installing.md) shows.

The Overview subpage is the exception: it writes the shell's own config,
`~/.config/proscenio/config.toml`. Its keys below are dotted TOML paths, so `overview.rows` is the
`rows` key of the `[overview]` table.

## Tiling

**Layout** sets `general:layout` to Dwindle, Master, Scrolling or Monocle. Below it the page shows
the subsection of the layout in use, if it has one: Dwindle, Master or Scrolling.

### Spacing

| Control | Hyprland option | Range |
|---|---|---|
| Gap between windows | `general:gaps_in` | 0–100 px |
| Gap around the screen | `general:gaps_out` | 0–200 px |
| Smart gaps | four lines in `multitasking.lua` | on or off |

**Smart gaps** removes the gaps, the border and the rounding around a workspace's only tiled
window, and around a maximized one. It is on while `multitasking.lua` holds all four of these lines,
and turning it on or off adds or removes them together, then reloads Hyprland:

```lua
hl.workspace_rule({ workspace = "w[tv1]", gaps_out = 0, gaps_in = 0 })
hl.workspace_rule({ workspace = "f[1]", gaps_out = 0, gaps_in = 0 })
hl.window_rule({ name = "no-gaps-wtv1", match = { float = false, workspace = "w[tv1]" }, border_size = 0, rounding = 0 })
hl.window_rule({ name = "no-gaps-f1", match = { float = false, workspace = "f[1]" }, border_size = 0, rounding = 0 })
```

### Dwindle

| Control | Hyprland option | Range |
|---|---|---|
| Keep the split direction when a window closes | `dwindle:preserve_split` | on or off |
| Split along the longer side | `dwindle:smart_split` | on or off |
| Resize towards the edge being dragged | `dwindle:smart_resizing` | on or off |
| New window takes (%) | `dwindle:default_split_ratio` | 10–190 %, stored as 0.1–1.9 |

### Master

| Control | Hyprland option | Values |
|---|---|---|
| Where a new window goes | `master:new_status` | becomes the master (`master`), joins the stack (`slave`), or takes the place of the window it opened from (`inherit`) |
| Which side the master sits on | `master:orientation` | left, right, top, bottom or center |
| New windows join at the top | `master:new_on_top` | on or off |
| Master takes (%) | `master:mfact` | 10–90 %, stored as 0.1–0.9 |

### Scrolling

| Control | Hyprland option | Values |
|---|---|---|
| Column width (%) | `scrolling:column_width` | 10–100 %, stored as 0.1–1.0 |
| Where new windows open | `scrolling:direction` | right, left, below (`down`) or above (`up`) |
| How the focused column comes into view | `scrolling:focus_fit_method` | centered (`0`), or scrolled just enough to show it (`1`) |
| Scroll to the focused window | `scrolling:follow_focus` | on or off |
| A single column fills the screen | `scrolling:fullscreen_on_one_column` | on or off |

### Snapping

Snapping applies to floating windows being dragged.

| Control | Hyprland option | Range |
|---|---|---|
| Snap to other windows and to the screen | `general:snap:enabled` | on or off |
| To windows (px) | `general:snap:window_gap` | 0–100 px |
| To the screen (px) | `general:snap:monitor_gap` | 0–100 px |

The two distances are disabled while snapping is off.

### Moving & resizing

| Control | Hyprland option | Range |
|---|---|---|
| Resize windows by dragging their borders | `general:resize_on_border` | on or off |
| Grab area around the border (px) | `general:extend_border_grab_area` | 0–100 px, disabled while dragging borders is off |
| Animate manual resizes | `misc:animate_manual_resizes` | on or off |
| Animate windows being dragged | `misc:animate_mouse_windowdragging` | on or off |

## Focus

| Control | Hyprland option | Values |
|---|---|---|
| What the pointer does to focus | `input:follow_mouse` | focus follows the pointer (`1`); click to focus (`0`); click to focus, while hover and scroll still reach the window under the pointer (`2`); the pointer never moves keyboard focus, not even with a click (`3`) |
| Where focus goes after a window closes | `input:focus_on_close` | the next window (`0`), the window under the pointer (`1`), or the window used last (`2`) |
| Let apps take focus when they ask for it | `misc:focus_on_activate` | on or off |

A window rule makes an exception for one application in either direction; see
[Apps settings](settings-apps.md).

After Focus, a link row opens the Overview subpage.

## Workspaces

| Control | Hyprland option |
|---|---|
| Switching to the current workspace goes back to the last one | `binds:workspace_back_and_forth` |
| Moving past the last workspace wraps around | `binds:allow_workspace_cycles` |
| Animate the wrap around | `animations:workspace_wraparound` |
| Hide the special workspace when switching | `binds:hide_special_on_workspace_change` |
| Close the special workspace once it is empty | `misc:close_special_on_empty` |

A link row opens the Swiping between workspaces subpage. Under **Distance between workspaces**,
**Gap (px)** sets `general:gaps_workspaces`, from 0 to 500 px in steps of 10: how far apart two
workspaces sit while the switch between them animates.

## Overview

The subpage's controls are keys of `~/.config/proscenio/config.toml`.

| Control | Key | Default | Range |
|---|---|---|---|
| Enable | `overview.enable` | on | |
| Center icons | `overview.centerIcons` | on | |
| Scale (%) | `overview.scale` | 18 %, stored as 0.18 | 1–100 % |
| Rows | `overview.rows` | 2 | 1–20 |
| Columns | `overview.columns` | 5 | 1–20 |
| Left to right or Right to left | `overview.orderRightLeft` | Left to right (`false`) | |
| Top-down or Bottom-up | `overview.orderBottomUp` | Top-down (`false`) | |

**Scale** sizes each workspace in the grid as a share of the screen's area, less what the bar and
other panels reserve. The two order choices set which way the workspace numbers run across the grid.

## Swiping between workspaces

Which fingers start the swipe is a gesture in the Hyprland config, listed and edited with the
touchpad gestures on [Mouse, keyboard and accessibility](settings-input.md). This subpage sets the
numbers behind the swipe, all of them in `multitasking.lua`:

| Control | Hyprland option | Range |
|---|---|---|
| Full swipe (px) | `gestures:workspace_swipe_distance` | 100–2000 px, in steps of 50 |
| Give up under (%) | `gestures:workspace_swipe_cancel_ratio` | 0–100 %, stored as 0–1 |
| Flick speed that switches anyway | `gestures:workspace_swipe_min_speed_to_force` | 0–100 |
| A swipe keeps the direction it started in | `gestures:workspace_swipe_direction_lock` | on or off |
| Locks after (px) | `gestures:workspace_swipe_direction_lock_threshold` | 0–200 px, disabled while the direction lock is off |
| Swiping past the last workspace makes a new one | `gestures:workspace_swipe_create_new` | on or off |
| Keep swiping without lifting the fingers | `gestures:workspace_swipe_forever` | on or off |
