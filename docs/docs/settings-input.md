---
title: Mouse, keyboard and accessibility
sidebar_label: Mouse, keyboard and accessibility
description: The Mouse & Touchpad, Keyboard and Accessibility pages, with their subpages for one mouse, the touchpad and its gestures, keyboard options and shortcuts, and the Hyprland options and lines each control writes.
---

# Mouse, keyboard and accessibility

Three pages of the settings window set up input: **Mouse & Touchpad**, **Keyboard** and
**Accessibility**. Nearly everything on them is a Hyprland option.

## Where the values go

A Hyprland option is read from the running compositor with `getoption` and written as one line in
the page's file under `~/.config/hypr/settings/`, such as
`hl.config({ input = { natural_scroll = true } })`. A line that already sets the option is replaced
in place. Changes made within 50 ms of each other are written together, then Hyprland reloads.
Hyprland applies these files only if its config loads the `settings` folder; see
[Building and installing](installing.md).

| Page or subpage | File |
|---|---|
| Mouse & Touchpad, Touchpad, and the touchpad gestures | `mouse.lua` |
| This mouse only | `devices.lua` |
| Keyboard, Keyboard options | `keyboard.lua` |
| Keyboard Shortcuts | `binds.lua` |
| Accessibility | `accessibility.lua` |

The few controls over the shell's own behavior are keys of `~/.config/proscenio/config.toml`,
written as dotted TOML paths below.

## Mouse & Touchpad

### General

**Primary button**, Left or Right, sets `input:left_handed` (Right is `true`). It orders the
physical buttons of mice and touchpads alike.

### Mouse

| Control | Hyprland option | Values |
|---|---|---|
| Speed, under Pointer speed | `input:sensitivity` | a slider from −100 to 100, stored as −1.0 to 1.0; its tooltip shows the value |
| Mouse acceleration | `input:accel_profile` | on writes `adaptive`, off writes `flat`; on whenever the profile is not `flat` |
| Natural scrolling | `input:natural_scroll` | on or off; scrolling moves the content rather than the view |
| Scroll method | `input:scroll_method` | two fingers or wheel (`2fg`), along the edge (`edge`), while a button is held (`on_button_down`), no scrolling (`no_scroll`) |
| Scroll amount (%) | `input:scroll_factor` | 10–500 %, in steps of 10, stored as 0.1–5.0 |

With acceleration off, the pointer moves exactly as far as the mouse did, which games and drawing
want; with it on, the pointer speeds up as the mouse moves faster. A link row opens
**This mouse only**.

### Pointer

| Control | Hyprland option | Values |
|---|---|---|
| Hide when still for (s) | `cursor:inactive_timeout` | 0–120 s; 0 keeps the pointer on screen |
| Hide while typing | `cursor:hide_on_key_press` | on or off |
| Who draws the pointer | `cursor:no_hardware_cursors` | the screen, except while tearing (`2`); always the screen (`0`); drawn with the rest of the screen (`1`) |

A pointer the screen draws itself stays smooth whatever the rest of the screen is doing. The last
choice is the one to pick when the pointer disappears or is drawn in the wrong place.

A link row after this section opens **Touchpad**.

### Scrolling in the shell

These keys tune scrolling in the shell's own panels, not in apps.

| Control | Key | Default | Range |
|---|---|---|---|
| Faster touchpad scrolling | `interactions.scrolling.fasterTouchpadScroll` | off | |
| Mouse scroll distance | `interactions.scrolling.mouseScrollFactor` | 120 | 10–1000, in steps of 10 |
| Touchpad scroll distance | `interactions.scrolling.touchpadScrollFactor` | 450 | 10–1000, in steps of 10 |
| Mouse detection threshold | `interactions.scrolling.mouseScrollDeltaThreshold` | 120 | 1–500 |

A scroll event at least as large as the detection threshold counts as coming from a mouse rather
than a touchpad.

## This mouse only

This subpage gives one mouse settings of its own. With no mouse connected it says "No pointing
device is connected". Otherwise a box picks one of the mice `hyprctl devices` reports, and the rows
below it show that mouse's own value where it has one, and the general value where it does not:

| Control | Device key | Values |
|---|---|---|
| Speed | `sensitivity` | −100 to 100, stored as −1.0 to 1.0 |
| Enabled | `enabled` | `true` or `false`; a disabled mouse stops moving the pointer until it is turned back on here |
| Mouse acceleration | `accel_profile` | `adaptive` or `flat` |
| Natural scrolling | `natural_scroll` | `true` or `false` |

Each setting is one line of `devices.lua`:

```lua
hl.device({ name = "logitech-g305-1", sensitivity = -0.25 })
```

Changes within 50 ms of each other are written together, then Hyprland reloads. **Follow the
general settings** removes the mouse's speed, acceleration and scrolling lines, leaving `enabled`
alone; it is disabled while the mouse has no line of its own. The
[Devices settings](settings-devices.md) page turns any input device on or off.

## Touchpad

| Subsection | Control | Hyprland option | Values |
|---|---|---|---|
| | Disable while typing | `input:touchpad:disable_while_typing` | on or off |
| Clicking | Tap to click | `input:touchpad:tap_to_click` | on or off |
| Clicking | Tap and drag | `input:touchpad:tap_and_drag` | on or off |
| Clicking | Middle click with three fingers | `input:touchpad:middle_button_emulation` | on or off |
| Secondary click | Corner push or Two finger push | `input:touchpad:clickfinger_behavior` | `false` or `true` |
| Tap with two or three fingers | Right, then middle or Middle, then right | `input:touchpad:tap_button_map` | `lrm` or `lmr` |
| Scrolling | Natural scrolling | `input:touchpad:natural_scroll` | on or off |
| Scrolling | Scroll amount (%) | `input:touchpad:scroll_factor` | 10–500 %, in steps of 10 |

### Gestures

Hyprland has no request that lists gestures, so the subpage works them out from the files. The
defaults are the `hl.gesture` calls in `~/.config/hypr/hyprland/general.lua`. Then the lines of
`mouse.lua` apply in order: one whose action is `"unset"` takes away the default with the same
fingers, direction and modifiers, and any other adds a gesture. No other Hyprland file is read.

Each gesture in effect is a card with its modifiers, its fingers and direction ("3 fingers · Swipe
any way"), its action, and a remove button. An action written as a Lua function reads "Custom
action". Removing a gesture added in `mouse.lua` deletes its line; removing a default appends an
`unset` line for it.

**Add a gesture** has three boxes:

| Box | Choices |
|---|---|
| Fingers | 3, 4 or 5 |
| Direction | swipe any way (`swipe`), left or right (`horizontal`), up or down (`vertical`), left, right, up, down, pinch in or out (`pinch`), pinch in (`pinchin`), pinch out (`pinchout`) |
| Action | switch workspace (`workspace`), move the window (`move`), resize the window (`resize`), close the window (`close`), toggle floating (`float`), toggle fullscreen (`fullscreen`), toggle the special workspace (`special`), scroll the layout (`scroll_move`) |

**Add gesture** appends one line to `mouse.lua`, or, when it brings back a default, removes that
default's `unset` line instead:

```lua
hl.gesture({ fingers = 3, direction = "pinchout", action = "fullscreen" })
```

A gesture that one already in effect would shadow is refused with a message naming that gesture. By
Hyprland's rule, a gesture with the same fingers and modifiers shadows the new one when it has the
same direction or the direction's axis, or when it is a swipe any way and the new one is horizontal
or vertical. After each write Hyprland reloads; if the reload reports a new `hl.gesture` error, the
file is put back, Hyprland reloads again and the error shows under the button.

The numbers behind the workspace swipe are on [Multitasking settings](settings-multitasking.md).

## Keyboard

### Input Sources

The keyboard layouts, in the order they are cycled through, come from `input:kb_layout` and
`input:kb_variant`. Each is a card with its place in the cycle, its name, its code and variant, and
buttons to move it up, move it down or remove it. The first cannot move up, the last cannot move
down, and the only layout cannot be removed.

**Add input source** opens **Add an input source** and turns into **Cancel**. Its search field
filters every layout and variant by all the words typed, against the name, the code and the
variant. Clicking one that is not in the cycle yet adds it at the end and closes the list. Names
come from the xkb registry, `/usr/share/X11/xkb/rules/evdev.xml` or the same file under
`/usr/local`.

### Input Source Switching

| Control | Writes |
|---|---|
| Switch between layouts with | the `grp:` entry of `input:kb_options`, from the registry's layout-switching options; **Only the shell shortcut** leaves none |
| Num Lock when the session starts | `input:numlock_by_default` |

A link row opens **Keyboard options**:

| Section | Control | xkb options |
|---|---|---|
| Special Character Entry | Alternate characters key | `lv3:…` |
| Special Character Entry | Compose key | `compose:…` |
| Modifier Keys | Caps Lock | `caps:…` |
| Modifier Keys | Ctrl | `ctrl:…` |
| Modifier Keys | Alt and Super | `altwin:…` |

Each box lists its group's options from the registry, led by **None** or **Default** for no option.
A choice replaces only its own group's entry in `input:kb_options`, which is a comma-separated list,
and keeps the rest.

### Keyboard Shortcuts

**Shortcuts follow the symbol on the key** sets `input:resolve_binds_by_sym`. On, a shortcut is the
letter it types, so it moves with the layout; off, it is the place on the keyboard, so it stays put
in any layout.

The shortcuts are Hyprland's binds that have a description and sit outside any submap, read with
`hyprctl binds`. A description such as `Window: Close` names a category, the text before the colon,
and a label, the rest. Binds that share a description are one shortcut with up to two key
combinations, primary and secondary, in the order Hyprland lists them. For what the default binds
are, see [Shortcuts](shortcuts.md).

Under a search field, one link row per category opens the **Keyboard Shortcuts** subpage for that
category, its detail listing the category's shortcuts. Shell, App, Window, Workspace, Media,
Utilities, Screen, Input and Session come first, any others after them by name. While the search
field holds text, the link rows give way to the matching shortcuts, grouped by category and editable
in place; a shortcut matches when every typed word is in its label or its keys.

A shortcut's row holds its label, a reset button, and two key buttons showing its combinations as
keycaps, or an add symbol for an empty slot. Rows are sorted by label, numbers by value. Mouse binds
show `LMB`, `RMB` or `MMB` and cannot be edited; keypad keys show as `KP 0` to `KP 9`, `KP −` and
`KP +`.

A key button opens **Press the new shortcut** with the shortcut's label. While it is open, the
settings window holds back Hyprland's own shortcuts, so a combination Hyprland uses reaches the
dialog. Held modifiers show as keycaps followed by "…"; the first other key completes the
combination, named by the unshifted key under it. If another shortcut has the combination, the
dialog warns "Used by “…”, which loses it". **Clear** empties the slot, **Cancel** closes, and **Set**
applies the combination. Taking the combination of the shortcut's other slot empties that slot, and
taking another shortcut's combination empties it there.

Every changed shortcut, including one that lost its combination, is one line of `binds.lua`, and
Hyprland reloads after each change:

```lua
shortcut("Window: Close", "SUPER + Q", false)
```

The arguments are the description, the primary combination and the secondary one, `false` for an
empty slot. `shortcut` is a Lua function that the Hyprland config has to define for these lines to
take effect; see [Hyprland](hyprland.md). A shortcut with a line shows its reset button, which
removes the line, and is highlighted while **Highlight changed settings** is on in the settings
window.

## Accessibility

| Section | Control | Hyprland option or key | Values |
|---|---|---|---|
| Seeing | Reduced motion | `animations:enabled` | on writes `false`: windows and workspaces appear at once instead of moving |
| Seeing | Color filter | `decoration:screen_shader` | see below |
| Bell | Play the bell sound | `misc:bell_sound` | `default` or `none`; shown only when Hyprland has the option |
| Bell | Flash the screen | `accessibility.flashOnBell` | off by default |
| Typing | Delay (ms), under Repeat keys | `input:repeat_delay` | 100–2000 ms, in steps of 25 |
| Typing | Rate (per second), under Repeat keys | `input:repeat_rate` | 1–100 |
| Zoom | Magnification (%), under Magnifier | `cursor:zoom_factor` | 100–500 %, in steps of 10; 100 % is no magnification |
| Zoom | Keep the magnified image sharp | `cursor:zoom_disable_aa` | on or off |

The magnifier enlarges the whole screen around the pointer.

### Color filter

The filter applies to the whole screen: None, Grayscale, Inverted colors, Red–green
(deuteranopia), Red–green (protanopia) and Blue–yellow (tritanopia). Picking one writes its
fragment shader to `~/.local/state/proscenio/shaders/<filter>.frag` (`grayscale`, `inverted`,
`deuteranopia`, `protanopia`, `tritanopia`) and points `decoration:screen_shader` at it; None clears
the option. The three color-blindness filters simulate the missing cone and move the color
difference it loses into channels that are still seen, so colors that are hard to tell apart shift
toward ones that are not. While `decoration:screen_shader` names some other file, a **Custom
shader** choice shows as the current one.

### Bell flash

With **Flash the screen** on, Hyprland's `bell` event lights up the display with the focused window:
a white layer that takes no input starts at 35 % and fades out over 300 ms, one flash at a time.
