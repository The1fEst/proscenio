---
title: Screenshots and recording
sidebar_label: Screenshots and recording
description: The region selector that takes screenshots of the screen, a window, a region or a drawn shape and copies, annotates or saves them, and the screen recorder behind proscenio record.
---

# Screenshots and recording

The region selector freezes every monitor and lets you pick what to capture: the whole screen, a
window, a rectangle or a shape drawn freehand, or a region that you scroll through to get one tall
image. The capture goes to the clipboard, to an annotation tool or into a screen recording.

It needs `grim`, `magick` (ImageMagick), `wl-copy` and `satty` for screenshots, and `wf-recorder`
and `slurp` for recordings. The settings page for screenshots names whichever of them is missing.

![The region selector over two windows, with a rectangle being drawn and the toolbar at the bottom](/img/screenshots/region-selector.webp)

## Opening it

| Global shortcut | `proscenio ipc call region …` | Opens the selector in |
|---|---|---|
| `regionScreenshot` | `screenshot` | the "As it is on screen" mode |
| `regionRecord` | `record` | the "Record a region" mode, without sound |
| `regionRecordWithSound` | `recordWithSound` | the "Record a region" mode, with sound |
| `recordStop` | — | Stops a running recording |

The scrolling capture has no shortcut of its own: open the selector and pick it on the toolbar.

The bar's screen snip button (`bar.utilButtons.showScreenSnip`, on by default) and the **Screen
snip** [quick toggle](quick-toggles.md) open it the way `regionScreenshot` does. While a recording
runs, the two record shortcuts stop it instead of opening the selector.

Opening the selector again closes the one that is open and starts over. Escape and the close button
dismiss it.

## Modes

The toolbar at the bottom of each monitor switches between seven modes:

| Mode | Takes |
|---|---|
| Whole screen | The whole monitor, on any click |
| With rounded corners and a shadow | A dragged rectangle or the window clicked, cut to the window's rounded corners and set on a drop shadow |
| As it is on screen | A dragged rectangle, or the window or surface clicked with `regionSelector.targetRegions.selectionPadding` pixels (5) around it |
| A drawn shape | The bounding box of a line drawn by dragging |
| Scrolling capture | A dragged rectangle, or the window or surface clicked, that you then scroll through; see [Scrolling capture](#scrolling-capture) |
| Record the screen | A recording of the whole monitor, on any click |
| Record a region | A recording of a dragged rectangle, or of the window or surface clicked with the padding around it |

Space switches between "With rounded corners and a shadow" and "As it is on screen", and from any
other mode to the first of them. A release counts as a click only when the pointer did not move at
all. A click where there is nothing to take closes the selector.

In the two whole-screen modes, a camera (or record) button next to the toolbar takes the screen as
well.

### Picking a window

A click takes the surface on the top layer under the pointer, such as an open panel, or else the
window under it. The candidates are:

- the windows on the monitor's active workspace, floating ones first;
- the monitor's surfaces on the top layer, except the bar and the dock. A window that overlaps one
  of them is not a candidate.

A floating window with a title bar from the `hyprbars` plugin is taken together with its bar. The
bar counts unless the window is fullscreen, has neither a class nor a title, or is one of
proscenio's own windows.

In the modes that take a window or a rectangle, the selector outlines the candidates: windows
with `regionSelector.targetRegions.windows` on (the default), surfaces with
`regionSelector.targetRegions.layers` on. The outlines are drawn at
`regionSelector.targetRegions.opacity` (30 %) and fade out as soon as a drag begins. With
`regionSelector.targetRegions.showLabel`, each one carries a tag with its app icon and window class,
or its layer namespace.

### Drawing a shape

A drawn shape is a line `regionSelector.circle.strokeWidth` pixels wide (6). The capture is the
line's bounding box, grown by `regionSelector.circle.padding` pixels (10) and half the stroke.

### What the selector shows

Outside the selection the screen is dimmed. A dashed line runs around the selection, its size is
written under its bottom-right corner, and with `regionSelector.rect.showAimLines` (on by default) a
faint horizontal and vertical line cross at the pointer. A pill next to the pointer shows what the
next release does, with a description for a second after the selector opens and whenever the action
changes.

## What a screenshot does

| Button | Result |
|---|---|
| Left | Copies the image to the clipboard |
| Right | Opens the image in `satty` to annotate it |

With **Also save to a file** on (`screenSnip.save`), a copied screenshot is also written to
`screenSnip.savePath` as `screenshot-YYYY-MM-DD_HH.MM.SS.png`; the clipboard gets it all the same.

### The notification

A copied screenshot sends a notification from "Screenshot" with the summary "Screenshot taken". The
body says that the image was copied to the clipboard, or copied and saved as the file's name, and
the image is the preview: the saved file, or the copied image scaled to fit 480 pixels. When a file
was saved, the notification has two buttons, **Open**, which opens the file in the default image
viewer, and **Open folder**, which opens the folder in the default file manager. It is not kept in
the notification history once it expires.

`screenSnip.notify` (on by default) turns it off. Screenshots opened in `satty` and recordings send
no such notification.

## Scrolling capture

The mode takes a region the way "As it is on screen" does, except that a clicked window is taken
without padding, and a region smaller than 64 pixels on a side closes the selector. The selector
then leaves the frozen picture: the dimming and the toolbar disappear, the other monitors' selectors
close, and the region keeps its dashed outline over the live screen. Clicks and wheel notches go to
the window under the pointer, so scroll its content as usual. The mouse button of the selection
decides the result, as in a normal screenshot: left copies, right opens `satty`.

A panel sits 16 pixels from the region, on the side with more room. It shows the end of the stitched
image, up to 200 by 420 pixels, and a line of status: "Scroll the content" until the content has
moved, then the image's height in pixels, "Lost track of the scrolling" or "Height limit reached".
**Done** finishes and **Cancel** discards. When the room beside the region is under 190 pixels, the
panel is not shown, and the keys are the only way to finish.

| Key | Does |
|---|---|
| Return, keypad Enter | Finishes, as **Done** does |
| Escape | Cancels |

The selector keeps the keyboard while the capture runs, so no other key reaches the window under the
region, and Space does nothing.

### How the image is built

1. Every 120 ms, and again right away after a capture in which the content moved, `grim` takes the
   region as it is on screen, at the monitor's scale, without the pointer and without the outline.
   The countdown option does not apply.
2. Each capture is compared with the previous accepted one. Every row is hashed, leaving out the
   outer 8 % of the width on each side so that scrollbars do not count. The rows that differ from
   the previous capture vote for the distance the content moved. The distance is accepted when at
   least 6 rows agree and at least 60 % of the changed rows do.
3. Only the rows beyond what is already captured are appended. A header that stays in place is
   kept once, at the top of the image; a footer that stays in place is left out. Scrolling back up
   adds nothing and never repeats rows.
4. A capture with no changed rows, or with changes in at most one sixteenth of the rows and no
   distance that explains them, adds nothing.
5. After 8 captures in a row that match nothing, or in which `grim` fails, the capture stops and the
   panel says so. **Done** still takes what was stitched.

The stitched image is at most 16,384 pixels tall and 128 MiB, and the stitcher holds that image and
one capture. Hashing, matching and the preview run on a worker thread. When the limit is reached
the capture stops. On finishing, the image goes through the same copy, save and annotate path as
any other screenshot, and the same [notification](#the-notification) follows.

Limits:

- Only downward scrolling adds rows.
- Consecutive captures must overlap: scrolling by more than the region's height between two
  captures loses the match.
- The content must scroll in whole pixels without blending; a smooth scroll that is captured
  mid-glide is fine, an animation that rescales or fades the content is not.
- Content made of identical rows, such as a long blank page, cannot say how far it moved.

## Options

The **Options** button on the toolbar opens a card above it. Its choices are saved in `config.toml`
and kept for the next time.

| Option | Key | Does |
|---|---|---|
| Wait before capturing: None, 5s, 10s | `regionSelector.countdownSeconds` | After the release, waits, takes the screen again and crops the new picture; a recording starts after the wait |
| Also save to a file | `screenSnip.save` | Keeps a file next to the clipboard copy |
| Record the microphone | `regionSelector.recordSound` | Records with sound; see below |
| Include the pointer | `regionSelector.showPointer` | Keeps the pointer in the frozen screen and in the screenshot |
| Start from the last region | `regionSelector.rememberRegion` | Shows the last region taken on that monitor when the selector opens |

## Recording

The recording modes hand the region to `proscenio record`. Once a recording starts, the selector
closes on the other monitors. On the recorded one it keeps only the dashed outline of the region,
which slowly pulses, and lets every click through. It closes when the recording ends.

`proscenio record` runs `wf-recorder` and waits for it:

| Command | Records |
|---|---|
| `proscenio record --region "X,Y WxH"` | That area, in Hyprland's layout coordinates, as `slurp` prints them |
| `proscenio record --fullscreen` | The focused monitor |
| `proscenio record` | An area picked with `slurp` |

- `--sound` adds sound: the monitor of the default output, which `pactl get-default-sink` names. This
  is what the selector's sound option and `regionRecordWithSound` pass.
- Run while `wf-recorder` is running, `proscenio record` stops the recording instead, whatever its
  arguments. The `recordStop` shortcut and the bar's record button use this.
- Videos go to `screenRecord.savePath`, or to the XDG videos folder when that is empty, as
  `recording_YYYY-MM-DD_HH.MM.SS.mp4` in `yuv420p`. The folder is created when it is missing.
- A notification from "Recorder" tells when a recording starts, stops, or is cancelled because
  `slurp` returned nothing.

The bar's record button (`bar.utilButtons.showScreenRecord`) runs `proscenio record` with no
arguments, so it asks `slurp` for the area. See the [command line](command-line.md) for the other
subcommands.

## Settings

The save paths and the selector's look are under **Privacy & Security › Screenshots & Recording**
in the [settings](settings.md).

| Key in `config.toml` | Default | Setting |
|---|---|---|
| `screenSnip.save` | `false` | Also save to a file |
| `screenSnip.savePath` | empty | The screenshot folder; `~` is expanded |
| `screenSnip.notify` | `true` | Notify after a screenshot |
| `screenRecord.savePath` | the XDG videos folder | The recording folder |
| `regionSelector.showPointer` | `false` | Include the pointer |
| `regionSelector.targetRegions.windows` | `true` | Hint target regions: Windows |
| `regionSelector.targetRegions.layers` | `false` | Hint target regions: Layers |
| `regionSelector.targetRegions.showLabel` | `false` | Show region labels |
| `regionSelector.targetRegions.opacity` | `0.3` | Hint opacity (%) |
| `regionSelector.targetRegions.selectionPadding` | `5` | Selection padding |
| `regionSelector.rect.showAimLines` | `true` | Show aim lines |
| `regionSelector.circle.strokeWidth` | `6` | Stroke width |
| `regionSelector.circle.padding` | `10` | Padding |

## How it works

`src/panels/regionselector/` is the selector, and `screenshot.rs` in it builds the shell commands it
runs.

1. On opening, the selector runs `grim -o <output>` for every monitor (`grim -c` with the pointer
   included), writing to `$XDG_RUNTIME_DIR/proscenio/screenshot/image-<output>`. Each monitor gets a
   surface on the `overlay` layer, namespace `proscenio:regionSelector`, that shows this frozen
   picture with the selection drawn over it.
2. On release, the region is clamped to the monitor and converted to physical pixels. `magick` crops
   it out of the frozen picture; in the window mode it also cuts the corners to the window's
   rounding and adds the shadow, and the result is a PNG with transparency.
3. The crop is piped to `wl-copy`, to `tee` and `wl-copy` when saving, or to `satty -f -`, and the
   temporary picture is removed. A selector dismissed without a capture deletes its picture too.
4. A copy then runs `proscenio screenshot-notify`, which sends the notification. Without a saved
   file the crop is also written to `image-<output>.preview.png` next to the temporary picture, for
   the notification's preview, and removed afterwards.

The scrolling capture deletes the frozen picture when the scrolling starts and captures the live
region with `grim -t ppm` into memory. When it finishes it writes the stitched image to the
temporary picture's path as a PPM file, and steps 3 and 4 follow with `magick` taking the whole
image.

With a countdown, the command first sleeps, then runs `grim` again and crops the fresh picture. With
the pointer included, the selector also turns off Hyprland's `cursor:hide_on_key_press` while it
takes the picture, so that the pointer is there to capture.

The last region taken is stored, with its monitor, in `~/.local/state/proscenio/states.json`.
