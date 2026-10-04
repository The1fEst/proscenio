---
title: Cheatsheet
sidebar_label: Cheatsheet
description: The card that lists every Hyprland keybind with a description, grouped by category, and the options for how its key caps look.
---

# Cheatsheet

A card in the middle of the screen listing every Hyprland keybind that has a description, grouped
into categories and drawn as key caps. The source is `src/panels/cheatsheet.rs`.

## Opening and closing

- The global shortcuts `cheatsheetToggle`, `cheatsheetOpen` and `cheatsheetClose` (see
  [Shortcuts](shortcuts.md)).
- The `cheatsheet` target over [IPC](ipc.md): `toggle`, `open` and `close`, for example
  `proscenio ipc call cheatsheet toggle`.

Both act on the focused monitor. Escape or the round close button in the card's top right corner
closes it.

The window covers its whole monitor on the overlay layer, with the card centered in it. The list
takes 70% of the monitor's width and height and scrolls vertically when it is longer.

## Where the list comes from

The shell asks Hyprland for its keybinds (`binds` over the IPC socket) and keeps those that have a
description. The text before the first `:` in a description is the bind's category, and the rest is
what the row says: a bind described as `Apps: Terminal` shows as *Terminal* in the *Apps*
category. Binds whose description has no `:` go into a last category, *Uncategorized*.

The categories flow left to right and wrap onto new rows, 10 pixels apart. Within a category, the
key caps line up in a column as wide as the widest of them, with the descriptions beside them.

The list is read from Hyprland each time the cheatsheet opens, so a keybind added to Hyprland's
config shows the next time it opens.

## Repeated binds

Binds that differ only in a number or a direction show once:

- A bind whose key contains a digit other than `1` is left out, and so is one whose key starts with
  `right`, `up` or `down`. Keys that mention a mouse button or a page key are always kept.
- In the binds that remain, the first `1` in the key becomes `<Number>` and the first `Left` becomes
  `<Direction>`. When the key had either, the description gets the same treatment: its first `1`
  becomes `<Number>`, and its first word `left`, `right`, `up` or `down` becomes `<Direction>`.

So the binds for Super with each number key, described as `Workspace: Focus workspace 1` and so
on, become one row: Super with the key `<Number>`, described as `Focus workspace <Number>`.

## Key caps

The modifiers come from the bind's modifier mask and always show in this order: Ctrl, Super, Shift,
Alt, Caps, Mod2, Mod3, Mod5. A `+` separates them from the key. For a bind on the `SUPER_L` or
`SUPER_R` key itself, the key cap is left out and only the modifiers show.

Some names are shortened: `mouse_up` reads *Scroll ↓*, `mouse_down` *Scroll ↑*, `mouse:272` *LMB*,
`mouse:273` *RMB*, `mouse:275` *MouseBack*, `Slash` `/`, `Hash` `#` and `Return` *Enter*.

These options are on **Appearance › Panels › Cheat sheet** (see
[Appearance settings](settings-appearance.md)). Their keys are under `cheatsheet`:

| Setting | Key | Default | Effect |
|---|---|---|---|
| Super key symbol | `superKey` | the Windows logo, Nerd Font `U+F05B3` | The text shown for Super. Empty leaves the Super cap blank. The page offers a choice of Nerd Font symbols; any text works in the config file. |
| Use macOS-like symbols for mods keys | `useMacSymbol` | off | Symbols for Ctrl, Alt, Shift, Space, Tab, Backspace, Delete, Enter, Escape and a few more. |
| Use symbols for function keys | `useFnSymbol` | off | Symbols for F1 to F12. |
| Use symbols for mouse | `useMouseSymbol` | off | Symbols for the scroll wheel and the left and right buttons. |
| Split buttons | `splitButtons` | off | Each modifier on its own cap, instead of all of them on one. |
| Keybind font size | `fontSize.key` | 12 | Size of the text on the caps. |
| Description font size | `fontSize.comment` | 12 | Size of the descriptions. |

When several symbol sets apply to one name, mouse symbols win over function-key symbols, and those
over macOS-like symbols. The caps use the monospace font (`appearance.fonts.monospace`).
