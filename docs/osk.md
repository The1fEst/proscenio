# The on-screen keyboard

Source: `modules/ii/onScreenKeyboard/`, three QML files and `layouts.js`, plus
`services/Ydotool.qml`. Read [foundations.md](foundations.md) first.

A keyboard along the bottom edge that types into the focused window through
`ydotool`, so it never takes keyboard focus itself.

## 1. The window

A `Scope` holds `pinned` (bound to `osk.pinnedOnStartup` until the pin button
is used) and a `Loader` on `GlobalStates.oskOpen`. Deactivating the loader
releases every key. Inside, a `PanelWindow`:

```qml
WlrLayershell.namespace: "quickshell:osk"
WlrLayershell.layer: WlrLayer.Overlay
anchors { bottom: true; left: true; right: true }
exclusiveZone: pinned ? implicitHeight - hyprlandGapsOut : 0
mask: Region { item: oskBackground }
visible: loader.active && !GlobalStates.screenLocked
```

No output is set, so the compositor picks one. The window is added to
`GlobalFocusGrab` as persistent, so pressing a key does not dismiss the
sidebar or overview that holds the grab.

The background is `colLayer0`, radius `windowRounding`, a rectangular shadow,
padding 10, centered with `elevationMargin` around it. It holds a row, spacing
5, of:

- a `VerticalButtonGroup` of two 40×40 `GroupButton`s, radius `normal`, that
  grow to 50 high while pressed: the pin (`keep`, toggled with `pinned`,
  switched on press) and hide (`keyboard_hide`);
- a 1 px `colOutlineVariant` line, 20 in from top and bottom;
- the keys: a column of rows, spacing 5 both ways.

## 2. Layouts

`layouts.js` holds three layouts by name — English (US), German, Russian —
chosen by `osk.layout`, falling back to English (US). A key is `{ keytype,
label, labelShift, labelCaps, shape, keycode }`. `keytype` is `normal`,
`modkey` or `spacer`; `keycode` is a Linux input event code. Only the names
and `keys` are read; `name_short`, `description`, `comment` and `labelAlt` are
unused.

## 3. A key

A `RippleButton`, `colLayer1` (transparent for `empty`), radius `small`, 45×45
scaled by shape: `fn` 1×0.7, `tab` 1.6, `shift` 2.5, `control` 1.3. `space`
and `expand` fill the row. The label is the main font at `large` (`small` for
`fn`); Backspace and Enter show the Material glyphs `backspace` and
`subdirectory_arrow_left` at `huge`. Toggled keys are `colPrimary` with an
`m3onPrimary` label.

Pressing sends `code:1`, releasing a normal key sends `code:0`. `Ydotool`
keeps a shift mode — off, on, locked — and a Shift key is a key whose code is
42 or 54:

- pressing Shift while off turns it on;
- releasing a normal key while on releases both Shift keys, so Shift lasts one
  key;
- a second Shift release within 300 ms of the first locks it; after that, the
  next Shift release unlocks;
- other modkeys latch on release and send `code:0` when released again.

Labels follow the mode: `labelCaps`, then `labelShift`, then `label` when
locked; `labelShift` when on.

## 4. Commands

IPC target `osk` with `toggle`, `open`, `close`; global shortcuts `oskToggle`,
`oskOpen`, `oskClose`; the Virtual Keyboard quick toggle.

---

**Status (proscenio).** Done. `src/panels/osk/` follows the QML: `mod.rs` is
the window, `key.rs` the key and `layouts.rs` the three layouts, and
`src/platform/ydotool.rs` is the service. `States::osk_open` stands in for
`GlobalStates.oskOpen`. The window is built on open and discarded on close,
like the `Loader`. The two controls are a `ButtonGroup` with
`set_vertical(true)` of `GroupButton`s with `set_clicked_height`.
`src/platform/grab.rs` keeps the persistent surfaces and adds them to every
grab, including one that is already held. Changing `osk.layout` while open
rebuilds the keys. The `fn` row is 32 px high where Qt makes it 31.5.
