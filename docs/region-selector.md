# The region selector

Source: `modules/ii/regionSelector/`, nine files, over
`modules/common/utils/ScreenshotAction.qml`, `TempScreenshotProcess.qml` and
the `ScreenRecording` service.

## 1. The window

One `WlrLayer.Overlay` surface per screen, anchored to all edges, keyboard
`OnDemand`, namespace `quickshell:regionSelector`. It is created when a
shortcut opens the selector and destroyed on dismissal; pressing a shortcut
while it is open recreates it. Before it shows, `grim -o <screen>` writes the
screen to a temporary file, and the frozen copy is drawn under everything.

Shortcuts: `regionScreenshot` (region mode), `regionRecord` and
`regionRecordWithSound` (record-a-region mode, which also set
`regionSelector.recordSound`), and `recordStop`. A record shortcut while
`wf-recorder` runs stops the recording instead of opening.

## 2. Modes

Six, chosen from the toolbar: the whole screen, a window with rounded corners
and a shadow, a region as it is on screen, a drawn shape, recording the
screen and recording a region. Space toggles the two region modes.

- **Region and window**: drag a rectangle, or click to take the window or
  top-layer surface under the pointer. Targets are the windows of the active
  workspace, floating first, minus any window overlapping a top-layer surface
  other than a bar or a dock; they are outlined at `targetRegions.opacity`
  with a `windowRounding` radius and fade out once a drag starts. The region
  mode pads a clicked target by `selectionPadding`; the window mode does not,
  and its crop gets the rounded corners and a drop shadow through `magick`.
- **Shape**: a free polyline in the selection color, `circle.strokeWidth`
  wide; the capture is its bounding box plus `circle.padding` and half the
  stroke.
- **Whole screen** modes: any click, or the camera button, takes the screen.

Around the region: a 60% black overlay, a 1 px dashed border (8 on, 4 off)
one pixel outside it, the `W x H` label below its right edge, and 1 px aim
lines at 20% opacity. The cursor guide follows the pointer (the region's
corner while dragging): a `colPrimary` pill with a sharp top-left corner, the
action's symbol and, for a second after opening or after the action changes,
its description.

## 3. The toolbar

At the bottom center, sliding up 8 px from below the screen as it opens: an
M3 toolbar holding the mode tabs with the sliding `colSecondaryContainer`
indicator, a separator and the Options button; then a tertiary FAB that takes
the whole screen (shown in the whole-screen modes) and one that closes.
Options opens a card above, right-aligned with the close button: the
countdown (none, 5 s, 10 s), saving to `screenSnip.savePath`, recording the
microphone, including the pointer, and starting from the last region.

## 4. The result

Left button copies, right button opens `satty`. The crop is taken from the
temporary file in physical pixels, piped to `wl-copy`, and also written to
`screenSnip.savePath` when saving is on. A countdown waits, grabs the screen
again and crops that. Recording runs `scripts/videos/record.sh --region`
(`proscenio record --region` in `proscenio`, see [files.md](files.md));
the selector then keeps only a breathing border on that screen and closes
when the recording ends.

---

**Status (proscenio).** Done. `src/panels/regionselector/mod.rs` is the selector,
`src/panels/regionselector/screenshot.rs` holds the commands, and
`src/ui/widgets/toolbar.rs` the M3 toolbar, tab bar and paired FAB. The
overlay, border, label, aim lines, cursor guide, window targets, toolbar,
options card and every capture and recording mode match the QML. A clicked
1270 × 750 window gives a 1382 × 862 image with its shadow.

As in the QML: `wf-recorder` gets the logical layout coordinates `--geometry`
expects, a release counts as a click only when the pointer never moved, the
two modes that take a click or a drag are named by their result ("With
rounded corners and a shadow", "As it is on screen"), the surface takes no
keyboard focus while recording, the screenshots a dismissal leaves unused
are deleted, and the target filter drops layers whose namespace contains
`:bar`, `:verticalBar` or `:dock`, which covers `proscenio:bar`.

Differences:

- **Temporary screenshots** live in `$XDG_RUNTIME_DIR/proscenio/screenshot`.
- The toolbar sits on whole pixels; qs draws it at a half-pixel offset, which
  blurs its icons across two pixels.
