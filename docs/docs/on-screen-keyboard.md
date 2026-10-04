---
title: On-screen keyboard
sidebar_label: On-screen keyboard
description: The keyboard along the bottom of the screen that types into the focused window through ydotool, its layouts, its Shift and modifier keys, and pinning it in place.
---

# On-screen keyboard

A full keyboard along the bottom edge of the screen. It never takes keyboard focus itself: each key
is sent as a key event through `ydotool`, so it lands in whatever window has focus, as if typed on a
real keyboard. The source is in `src/panels/osk/`, with the key sender in
`src/platform/ydotool.rs`.

## Opening and closing

- The keyboard button among the bar's utility buttons, shown while
  `bar.utilButtons.showKeyboardToggle` is on (the default; see [The bar](bar.md)).
- The *Virtual Keyboard* toggle in the [sidebar](sidebar.md) (see [Quick toggles](quick-toggles.md)).
- The global shortcuts `oskToggle`, `oskOpen` and `oskClose` (see [Shortcuts](shortcuts.md)).
- The `osk` target over [IPC](ipc.md): `toggle`, `open` and `close`, for example
  `proscenio ipc call osk toggle`.
- The hide button on the keyboard itself.

While the screen is locked the keyboard is hidden, and it comes back after unlocking if it was
open.

The window exists only while the keyboard is open. Closing it sends a key release for every key
code, so no key or modifier stays held down.

## Pinning

The pin button at the left of the keyboard reserves room for it at the bottom of the screen, so
windows tile above it instead of under it. Unpinned, the keyboard floats over the windows.

`osk.pinnedOnStartup` (default off; *Pinned on startup*) decides whether the keyboard starts pinned.
The pin follows that setting until the pin button is pressed; from then on, until the shell
restarts, the button decides.

## Layouts

| `osk.layout` | Labels |
|---|---|
| `English (US)` (default) | US QWERTY |
| `German` | German QWERTZ |
| `Russian` | Russian ЙЦУКЕН |

An unknown name falls back to English (US). The layout is chosen on **Appearance › Panels**, under
*On-screen keyboard* (see [Appearance settings](settings-appearance.md)); changing it while the
keyboard is open rebuilds the keys.

Every key sends a Linux input event code for a position on the keyboard, not a character. Which
character comes out is decided by the keyboard layout Hyprland uses, so pick the on-screen layout
that matches it: the layout only changes what the keys say.

Each layout has six rows: Esc, F1 to F12, Print Screen and Delete in a short top row; the number
row with Backspace; three letter rows with Tab, Enter and both Shift keys; and a bottom row of Ctrl,
Alt, Space, the right Alt and the right Ctrl, with Menu in the English and Russian layouts and the
left and right arrow keys in the German one.

## Shift and modifiers

**Shift** has three states, shared by both Shift keys:

- Press Shift once and the next key is shifted, after which Shift lets go by itself.
- Press Shift twice within 300 ms and it locks, like Caps Lock; the keys show their capital labels.
- Press Shift again, while locked or after the 300 ms, and it lets go.

**Ctrl and Alt** latch: a tap holds the modifier down, and the next tap of the same key lets it go.
So Ctrl, then C, then Ctrl types Ctrl+C.

The labels follow the Shift state, and a held Shift or a latched modifier shows in the primary
color.

## How keys are sent

Pressing a key runs `ydotool key --key-delay 0 <code>:1`, and releasing it runs the same with
`<code>:0`. The **Appearance › Panels** page says so when `ydotool` is missing, and without it the
keyboard draws but types nothing.

The window sits on the overlay layer, anchored to the bottom edge, on the monitor the compositor
picks. It never asks for keyboard focus, and only the keyboard itself takes pointer input; the
transparent margin around it does not. It belongs to every focus grab the shell holds, so pressing
its keys does not close an open panel such as the sidebar or the overview.
