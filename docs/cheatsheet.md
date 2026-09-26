# The cheatsheet

Source: `modules/ii/cheatsheet/`, five files plus `periodic_table.js`, over
`services/HyprlandKeybinds.qml`.

A centered card with two tabs: every Hyprland keybind that carries a
description, and a periodic table.

## 1. The window

Anchored to all four edges, transparent, namespace `quickshell:cheatsheet`,
masked to the card so the rest of the screen stays click-through. It is a
dismissible member of the shared focus grab, closes on Escape, and has a
40 px round close button 20 px in from the top right. Ctrl+PageUp/PageDown
and Ctrl+Tab move between the tabs.

The card is `colLayer0` with a 1 px border, radius `windowRounding` (18) and
padding 20. Inside it a `Toolbar` with the two tabs and a `SwipeView`, whose
current index is remembered in `Persistent.states.cheatsheet.tabIndex`.

Reached only through the `cheatsheet` IPC target or the shortcuts
`cheatsheetToggle` / `cheatsheetOpen` / `cheatsheetClose`.

## 2. The keybinds page

70 % of the screen in both axes. A horizontally-scrolling `Flow` laid out
`TopToBottom`, so categories stack into columns and wrap to the next column
when they run out of height, spacing 10.

Keybinds come from `hyprctl binds -j`, re-read on `configreloaded`. The
category is the part of `description` before the first `:`, and the binds
with no `:` land in a final "Uncategorized" column.

Two filters drop the repetitive binds, so `Super+1…9` shows once:

- `containsNonFirstRepetitive` hides a bind whose key has a digit that is not
  1, or starts with `right`, `up` or `down`;
- what remains is rewritten — the first `1` in the key becomes `<Number>`,
  the first `Left` becomes `<Direction>`, and the same happens in the
  description when the key was one of those.

Each row is the modifier caps, a `+`, the key cap, then the description.
The caps column is as wide as the widest in its category. Modifiers come from
the mod mask, bit 2 Ctrl, 6 Super, 0 Shift, 3 Alt, 1 Caps, 4 Mod2, 5 Mod3,
7 Mod5, in that order rather than by bit number.
`splitButtons` decides whether they are separate caps or one joined cap.
`SUPER_L` and `SUPER_R` hide the key cap entirely.

Names are substituted before display: Super becomes `cheatsheet.superKey` or
nothing, `mouse_up` becomes "Scroll ↓" and `mouse_down` "Scroll ↑", plus
the LMB/RMB/MouseBack, Slash,
Hash and Return names, and then optionally the mac, function-key and mouse
symbol maps.

A `KeyboardKey` is a rounded rectangle in `colOnLayer0` with a 1 px border
and 2 extra pixels along the bottom, holding a `m3surfaceContainerLow` face
with the name in the monospace font.

## 3. The elements page

`periodic_table.js` holds two arrays of rows — the main table and the two
series — of `{ name, symbol, number, weight, type }`, with `type: 'empty'`
padding the gaps. Each tile is 70×70, `colLayer2`, radius `small`, with the
atomic number badged top-left and the weight top-right; an empty tile is laid
out but fully transparent.

---

**Status (proscenio).** `src/panels/cheatsheet.rs` builds the window on all four edges
with the dismissible grab, Escape and the close button, around the keybinds
page alone. The elements page and the tab toolbar are left out. The keybinds
page reads `hyprctl binds`, groups by the same prefix rule with Uncategorized
last, applies both repetition filters, the substitutions and the mouse,
function-key and mac symbol maps, and lays the categories out row-major in a
`Flow` (`src/ui/widgets/flow.rs`, spacing 10) inside a vertical scroller at
70 % of the monitor. The QML's columns run off to the right instead. A
`GtkSizeGroup` per category gives the caps column its shared width while the
caps keep their own width, left-aligned as in the QML `Row`. The key caps
carry the two-layer border with its heavier bottom edge.
