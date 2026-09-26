# Screen corners

Source: `modules/ii/screenCorners/ScreenCorners.qml`, one file. Read
[foundations.md](foundations.md) for `RoundCorner`.

Four layer surfaces per screen, one per corner, on `WlrLayer.Overlay` with
namespace `quickshell:screenCorners`, `exclusionMode: Ignore`, transparent.
Each is anchored to its own corner and is as large as its content.

## 1. When they exist

```qml
visible: fakeScreenRounding === 1 || (fakeScreenRounding === 2 && !fullscreen)
```

`appearance.fakeScreenRounding` is 0 none, 1 always, 2 when not fullscreen —
the default. `fullscreen` is computed per monitor:

```qml
monitorData = HyprlandData.monitors.find(m => m.name === monitor.name)
fullscreen  = HyprlandData.windowList.some(win =>
                  win.fullscreen === 2 && win.workspace.id === monitorData?.activeWorkspace?.id)
```

`visible` covers the whole window, so `fakeScreenRounding` 0 also removes the
corner-open regions below.

## 2. The corner itself

A `RoundCorner` of `implicitSize: Appearance.rounding.screenRounding` (23),
whose `color` is left at its default **black**: it is a mask that squares off
the display's rounded corner, not a themed decoration. The window's mask is
only the corner-open loader, so the painted corner is click-through.

## 3. The corner-open region

A `FocusedScrollMouseArea` under `sidebar.cornerOpen`:

```
enable   true      bottom  false     valueScroll true    clickless false
cornerRegionWidth  250     cornerRegionHeight 5
visualize false    clicklessCornerEnd true     clicklessCornerVerticalOffset 1
```

It is loaded on the two corners that match `bottom` — the top pair by
default — and grows the window to 250×23, since the corner keeps its 23.

- **Entering** opens the sidebar when `clickless`, otherwise runs the corner
  check.
- **The corner check** (`clicklessCornerEnd`) fires when the pointer is
  within 2 px of the outer edge horizontally *and* past
  `clicklessCornerVerticalOffset` vertically, and only on the transition into
  that state.
- **Clicking** toggles.
- **Scrolling** changes brightness on the left corners and volume on the
  right, with the same 0.01 / 0.02 volume step the bar uses, and closes the
  matching OSD on `onMovedAway`.
- **`visualize`** paints the region `colPrimary`.

**Only the right corners act on the sidebar.** `actionForCorner` and
`stateForCorner` are keyed on `TopRight` and `BottomRight` only, so entering
or clicking a left corner does nothing — the left ones exist for the
brightness scroll.

---

**Status (proscenio).** `src/panels/corners.rs` builds the four surfaces per monitor with
the same anchors, `exclusive_zone(-1)` for `ExclusionMode.Ignore`, an input
region set to the corner-open rectangle alone and empty when there is none,
the black corner drawn with cairo, the 250×5 region on the pair `cornerOpen.
bottom` selects, the 2 px edge test with its vertical offset and its
transition-only trigger, click to toggle, the brightness and volume wheels,
and the `visualize` rectangle. Only the right corners reach the sidebar.
The corners hide under a fullscreen window whatever `fakeScreenRounding`
says, as every persistent surface does ([foundations.md](foundations.md) §9).
With `interactions.deadPixelWorkaround.enable` the right and bottom corners
reach one pixel past their edge and draw one pixel in from it, so the last
pixel column takes input. A wheel turn in a corner region closes its OSD once
the pointer leaves or moves 20 px away, as `FocusedScrollMouseArea.movedAway`
does, through the bar's `watch_moved_away`.

The corners are not persistent members of the focus grab: a click on one
closes an open panel ([foundations.md](foundations.md) §5).
