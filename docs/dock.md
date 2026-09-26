# The dock

Source: `modules/ii/dock/`, five files, over `services/TaskbarApps.qml`.

A bottom bar of app buttons that hides itself unless pinned, hovered, or the
workspace is empty.

## 1. The window

One per screen, bottom-anchored across the full width, namespace
`quickshell:dock`, transparent, hidden while the screen is locked.

```
implicitHeight = dock.height (60) + elevationMargin (10) + hyprlandGapsOut (5)
exclusiveZone  = pinned ? implicitHeight − gapsOut − (elevationMargin − gapsOut) : 0
mask           = the dock's MouseArea
```

`reveal` is `pinned || (hoverToReveal && hovered) || dockApps.requestDockShow
|| !activeToplevel.activated` — so an empty workspace shows it too. Hiding
slides the content by the mouse area's top margin, animated with
`elementMoveFast`: down to `implicitHeight − hoverRegionHeight` (2 px left
peeking) when `hoverToReveal`, and fully past the edge otherwise.

The plate is `colLayer0` with a 1 px border, radius `large` (23), inset by
the elevation margin on top and the gap on the bottom, with a shadow.

## 2. The row

Spacing 3, padding 5: a pin button, a separator, the apps, a separator, and
an `apps` button that toggles the overview.

The pin is a 35×35 `GroupButton` of radius `normal` (17) carrying `keep`,
toggled on `pinned`. Each separator is a 1 px `colOutlineVariant` line inset
top and bottom by `elevationMargin + padding + rounding.normal`.

## 3. The app list

`TaskbarApps.apps` builds a map keyed by lower-cased app id: first every
pinned id with no windows, then the literal key `SEPARATOR` if anything is
pinned, then every toplevel whose app id does not match one of
`dock.ignoredAppRegexes`, appended to its key's window list.

## 4. An app button

Square, as tall as the dock minus its insets. Inside, a 35 px icon from
`AppSearch.guessIcon`, desaturated 0.8 and washed with `colPrimary` at 10 %
when `monochromeIcons`; under it up to three dots, `10×4` each, or `4×4`
circles when there are more than three windows, `colPrimary` when one of the
app's windows is activated and `colOnLayer0` at 40 % otherwise.

- **Click** launches the desktop entry when there are no windows, otherwise
  cycles to the next window and activates it.
- **Middle-click** always launches a new instance.
- **Right-click** toggles the pin, rewriting `dock.pinnedApps`.
- **Dragging** reorders, and dropping writes the new pinned order — the drop
  index comes from walking the other buttons' widths and stopping at the
  first whose midpoint is past the pointer.
- **Hovering** a button with windows opens a preview popup after 100 ms,
  showing live window thumbnails with their own close controls.

---

**Status (proscenio).** `src/panels/dock/mod.rs` follows the three QML files.

The window is a full-width strip of the same height. Hiding slides the content
by the mouse area's top margin, 200 ms with `elementMoveFast`, and the input
region follows it, so only `hoverRegionHeight` stays hoverable. The pinned flag
is shared by every monitor's dock, as the QML `Scope` property is.

Unlike qs, proscenio unloads the hidden dock: once the slide-out ends, the
window unmaps and is unrealized. A separate
strip surface, `proscenio:dockTrigger`, takes over the hover region. It is as wide
as the dock and `hoverRegionHeight` tall, and any motion over it brings the
dock back. The strip is painted at alpha 1/255, because GTK commits no buffer
for a fully transparent surface, and an unmapped layer gets no pointer.

The dock maps under a pointer that has not moved, so it gets its first enter
only on the next motion. A real mouse leaving upwards passes through the dock
and hides it. A pointer that jumps away in one event leaves it shown until the
next hover.

The app row places its buttons at computed positions, the way `DockApps` does:

- other buttons slide 200 ms when the order changes;
- the row width animates;
- a drag follows the pointer at 0.85 opacity past a 10 px threshold;
- dropping writes the pinned order to `dock.pinnedApps`.

App buttons follow `DockAppButton`:

- the 50 px background inside 10 px insets;
- the 35 px icon stretched to its 34×35 slot, as the QML `IconImage` does;
- the dots 2 px under it;
- the in-list separator at 0.4 opacity.

Desktop entries are found and launched the way `DesktopEntries.heuristicLookup`
and `AppLaunch.entry` do it: by id, then by `StartupWMClass`, and started
inside `systemd-run --user --scope`. `ignoredAppRegexes` are real regular
expressions through `GRegex`. Every process proscenio starts and detaches
(apps, `xdg-open` and similar) begins its own session with `setsid()`, so a
`^C` or hang-up sent to proscenio's terminal does not reach it. The scope
separates the cgroup, not the process group.

Windows come from `hyprctl clients` rather than the foreign-toplevel protocol.
Previews use `hyprland_toplevel_export_v1` (vendored in `protocols/`, trimmed
to version 1) through `src/platform/capture.rs`. The popup in `src/panels/dock/preview.rs` is a
GTK popover centered on the hovered button. It carries the 100 ms settle timer,
the 200 ms fade, the title row with its close button, and the live capture
fitted to 300×200 with radius 12. A card is as wide as its capture, as in qs;
the title fills that width and ends in an ellipsis. Click focuses a window;
middle-click and the
close button close it. Both dispatch in the Lua syntax,
`hl.dsp.focus({ window = "address:…" })` and `hl.dsp.window.close(…)`, through
`hypr::focus_window` and `hypr::close_window`.

The first frame is taken at once; after that a card waits for its window to
change before asking again, as qs's live `ScreencopyView` does, but no more
than 30 times a second per window. The overview's thumbnails share that cap.

Frames are copied on the GPU when it can be done (see `src/platform/capture.rs`):
Hyprland renders the window into a GBM buffer proscenio allocates on the
compositor's device, GTK imports it, and the window's own renderer scales it
down to the card's size with trilinear filtering before the full-size buffer
is released. Only the small texture is kept. A modifier is used only when
Hyprland lists it, GTK takes it and Vulkan can sample it: Hyprland answers an
unsupported one with a fatal protocol error, and on NVIDIA the compressed
modifiers that EGL accepts make GTK's Vulkan renderer download the frame.
NVIDIA's driver holds about a frame of system memory for every imported buffer
while it lives, so the full-size frame is not kept.

Without a usable modifier, or after the GPU path fails once, frames come over
`wl_shm` and are shrunk on the CPU by area averaging.

The popup opens only once every card has its first frame. Moving to another
app closes it until the new pictures arrive. A card whose capture fails three
times counts as ready, so a window that cannot be captured still gets a card;
qs opens the popup after a second instead. Cards are kept per window: a title
change updates the label in place, and a closed window drops only its own
card. The captures stop with the fade-out, as in qs.

The popover is unrealized whenever it is hidden.

The QML popup stays open when the pointer moves to another app and grows or
shrinks to it over 200 ms. proscenio closes it and reopens it once the new
cards have their pictures, with no size animation. The overview button runs
the `overviewWorkspacesToggle` action, as qs flips
`GlobalStates.overviewOpen`.
