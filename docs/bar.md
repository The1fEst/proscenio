# The bar

Source: `modules/ii/bar/`. The colors and curves named below are defined in
[foundations.md](foundations.md).

The bar is one layer-shell window per monitor, holding a transparent
background rectangle, three groups pinned to the horizontal center, and a
right-hand region that runs from the center group to the screen edge.

---

## 1. The window — `Bar.qml`

### 1.1 One per screen

`Variants` over a model that is `Quickshell.screens` filtered by
`bar.screenList`; an empty or missing list means every screen. Inside each,
a `LazyLoader` with

```qml
active: GlobalStates.barOpen && !GlobalStates.screenLocked
```

so the bar is destroyed, not hidden, while locked or toggled off.

Three global shortcuts drive `GlobalStates.barOpen`: `barToggle`, `barOpen`,
`barClose`.

### 1.2 Window properties

```qml
WlrLayershell.namespace: "quickshell:bar"
exclusionMode: ExclusionMode.Ignore
color: "transparent"
implicitHeight: Appearance.sizes.barHeight + barRoot.hugRounding
anchors { top: !bar.bottom; bottom: bar.bottom; left: true; right: true }
```

The layer is the default (`Top`).

`hugRounding` is
`HyprlandOptions.numberOr("decoration:rounding", Appearance.rounding.screenRounding)`
— Hyprland's own corner radius, falling back to 23.

**The window is taller than the bar.** `barHeight` is 40 in Hug style, 50 in
Float style (40 + 5·2), and the hug rounding is added on top in *both* styles
even though the corner decorations are only drawn in Hug. So a Hug bar with
`decoration:rounding 20` gets a 60 px tall window with a 40 px bar and a 20 px
strip below it.

### 1.3 Exclusive zone

```qml
exclusiveZone: (autoHide.enable && (!mustShow || !autoHide.pushWindows)) ? 0 :
    Appearance.sizes.baseBarHeight
    + (Config.options.bar.cornerStyle === 1 ? Appearance.sizes.hyprlandGapsOut : 0)
```

So **40** in Hug and Plain, **45** in Float: `baseBarHeight` plus *one* gap,
not the full `barHeight` of 50 and not the window height. The window reserves
less than it occupies.

`exclusionMode: Ignore` means the bar does not itself respect other surfaces'
exclusive zones — it is the topmost thing on the screen edge.

`hyprlandGapsOut` here is the constant 5 from `Appearance.sizes`, not a value
read back from Hyprland, and `proscenio` uses the same constant.

### 1.4 The input region

```qml
mask: Region { item: hoverMaskRegion }
```

and `hoverMaskRegion` is anchored to fill `barContent` with

```qml
topMargin: -Config.options.bar.autoHide.hoverRegionWidth
bottomMargin: -Config.options.bar.autoHide.hoverRegionWidth
```

Negative margins grow the region, so the clickable area is the bar content
grown by `hoverRegionWidth` (default **2 px**) above and below — and nothing
else. **The round corner decorations are outside the mask and are
click-through.** Clicks land on whatever is behind them.

### 1.5 Auto-hide

Off by default (`bar.autoHide.enable: false`). When on:

- `mustShow = hoverRegion.containsMouse || superShow`.
- `superShow` is set by a timer started when `GlobalStates.superDown` goes
  true, with interval `autoHide.showWhenPressingSuper.delay` (default 140 ms),
  and cleared immediately when Super is released.
- Hiding is a margin, not a visibility change: `barContent.anchors.topMargin`
  goes to `-barHeight` (or `bottomMargin` when the bar is at the bottom),
  animated with **`elementMoveFast`** (200 ms).
- With `pushWindows` false the exclusive zone stays 0 even while shown, so
  revealing the bar overlaps windows instead of moving them.

### 1.6 Focus grab

```qml
Component.onCompleted:  GlobalFocusGrab.addPersistent(barRoot)
Component.onDestruction:GlobalFocusGrab.removePersistent(barRoot)
```

The bar is a **persistent** member of the shared focus grab. It stays
hoverable while a sidebar holds the grab, and a second click on the sidebar
button closes the sidebar. A sidebar grab that leaves the bar out swallows
that click, and the button ignores input until the pointer moves.

### 1.7 Round corner decorations

A `Loader`, `active: showBarBackground && cornerStyle === 0`, anchored to
`barContent.bottom` (or `.top` when the bar is at the bottom), `height:
hugRounding`. It holds two `RoundCorner`s of `implicitSize: hugRounding`,
colored `colLayer0`, cornered TopLeft/TopRight, or BottomLeft/BottomRight when
the bar is at the bottom.

### 1.8 Dead-pixel workaround

`interactions.deadPixelWorkaround.enable` (default false) shifts the window
1 px right and down by negative margins, then compensates inside. Hyprland
leaves the rightmost pixel column without input.

**Status (proscenio).** Window, anchors, exclusive zone, hug corners and the input
strip grown by `hoverRegionWidth` are implemented in `src/main.rs` and
`src/panels/bar/mod.rs`. The namespace is `proscenio:bar`; the region selector
skips it as it skips `quickshell:bar`. The float style's shadow is a CSS
`box-shadow` of `0 1px 9px 1px` in `colShadow`, the blur, offset and spread
of `StyledRectangularShadow`. Auto-hide moves the
content by a `text::Shift` offset over 200 ms of `expressiveEffects`, and the
input strip follows the offset; Super-to-reveal reads `States.super_down`,
published by the `workspaceNumber` shortcut's press and release. `barToggle`,
`barOpen` and `barClose` map and unmap the window. With
`interactions.deadPixelWorkaround.enable` the window reaches one pixel past
the right edge, and past the bottom for a bottom bar, so the last column
Hyprland keeps from input lies inside it. The QML destroys the bar while the
screen is locked; proscenio hides it, and `src/ui/unload.rs` unrealizes it
one second later. Unrealizing frees the surface and its GPU buffers; the
widget tree and its subscriptions stay.

---

## 2. The content region — `BarContent.qml`

### 2.1 Shortening by screen width

```qml
useShortenedForm = (1000 >= screen.width) ? 2 : (1200 >= screen.width) ? 1 : 0
centerSideModuleWidth = [360 or 140, 280, 190][useShortenedForm]
```

The unshortened width is `verbose ? 360 : 140`. The form gates several
widgets, listed with each below.

### 2.2 Background

Two items filling the whole content:

```qml
Rectangle {
    anchors.fill: parent
    anchors.margins: cornerStyle === 1 ? hyprlandGapsOut : 0
    color: showBackground ? colLayer0 : "transparent"
    radius: cornerStyle === 1 ? Appearance.rounding.windowRounding : 0   // 18
    border.width: cornerStyle === 1 ? 1 : 0
    border.color: colLayer0Border
}
```

plus, when `showBackground && cornerStyle === 1 && floatStyleShadow`, a
`StyledRectangularShadow` behind it.

### 2.3 Three regions

The bar is not a three-column layout. It is a centered `Row` with two
`MouseArea`s filling whatever is left:

- `barLeftSideMouseArea` — from the left edge to `middleSection.left`
- `middleSection` — a `Row` anchored to `parent.horizontalCenter`, spacing 4
- `barRightSideMouseArea` — from `middleSection.right` to the right edge

Both side areas are `FocusedScrollMouseArea` and both have
`implicitHeight: baseBarHeight` (40).

The centered `Row` holds, in order: the left center group, a separator, the
workspaces group, a separator, the right center group. Both separators are
`visible: bar.borderless` — they only appear when the group backgrounds are
switched off. Each is a 1 px `Rectangle` of `colOutlineVariant`, filling the
height minus `baseBarHeight / 3` (≈13.3 px) top and bottom.

**Status (proscenio).** The center row, the separators and the three shortened
forms match — `Config::shortened(screen_width)` and
`Config::center_side_width(screen_width)` reproduce the thresholds, and every
widget gated on the form is gated the same way. The three regions sit in a
`Strip`, which keeps the center row centered and lets the sides take the rest
and overflow like the QML anchors do. The background rectangle is an overlay
underneath, so the bar keeps its height whatever the regions ask for. Each
side region carries a scroll controller and a hover controller instead of
being a `FocusedScrollMouseArea`. A wheel notch followed by a pointer move of
more than 20 px, or by leaving the region, closes that region's OSD, as
`movedAway` does.

---

## 3. `BarGroup.qml`

The pill behind each group of widgets.

```
implicitWidth  = gridLayout.implicitWidth + padding·2     (padding default 5)
implicitHeight = Appearance.sizes.baseBarHeight           (40, always)
```

Inside, a `Rectangle` filling the group with **4 px top and bottom margins**
and no horizontal margin, colored `colLayer1` (or transparent when
`borderless`), radius `Appearance.rounding.small` = **12**.

The contents are a `GridLayout` with `columnSpacing: 4`, `rowSpacing: 12`,
anchored to the group's `verticalCenter`, `left` and `right`, with
`margins: padding`.

- The group's **outer height is always 40**, the painted pill is **32**
  (40 − 4 − 4), and the layout inside is centered vertically, not filled.
  `margins: padding` takes effect only horizontally.
- Raising `padding` does not make the group taller.

**Status (proscenio).** `Group` in `src/panels/bar/mod.rs` is an overlay: the `.group`
background (radius 12, `colLayer1`) with 4 px top and bottom margins, and on
top a `Row` with 5 px padding, centered vertically in a 40 px holder. `Row`
follows `RowLayout`: children keep their natural height and are centered with
Qt's round-half-up, not GTK's floor. Matches.

---

## 4. Center-left group — media and resources

`implicitWidth: centerSideModuleWidth` (360 verbose / 140 / 280 / 190).
Contents: `Media` (hidden when `useShortenedForm == 2`) and `Resources`
(`Layout.fillWidth: true`).

### 4.1 `Media.qml`

Visually it is a single 20 px ring with an icon punched out of it — no track
title in the bar.

```
ClippedFilledCircularProgress {
    implicitSize: 20
    lineWidth: Appearance.rounding.unsharpen        // 2
    value: hasTrackLength ? position / trackLength : 0
    colPrimary: colOnSecondaryContainer
    enableAnimation: false
}
MaterialSymbol {
    fill: 1
    text: isPlaying ? "pause" : "music_note"
    iconSize: Appearance.font.pixelSize.normal      // 16
    color: m3onSecondaryContainer
}
```

`implicitWidth` is `rowLayout.implicitWidth + rowLayout.spacing·2` with
spacing 4, i.e. the ring plus 8 px.

Mouse, over the whole item:

| Button | Action |
| --- | --- |
| Left | toggle `GlobalStates.mediaControlsOpen` |
| Middle | `activePlayer.togglePlaying()` |
| Right | `activePlayer.next()` |
| Forward (button 9) | `activePlayer.next()` |
| Back (button 8) | `activePlayer.previous()` |

The title shown elsewhere is `StringUtils.cleanMusicTitle(trackTitle)`,
falling back to "No media".

**Status (proscenio).** Ring, icon and the four transport buttons are implemented
in `src/panels/bar/media.rs`; left click opens the media controls. `src/services/mpris.rs`
ports `MprisController`: the tracked player follows `PlaybackStatus` changes,
is adopted and dropped by the same rules, and keeps the known track lengths.

### 4.2 `Resources.qml` and `Resource.qml`

Three `Resource`s in a row with spacing 0 and `Layout.leftMargin: 6` on the
second and third:

| Icon | Value | Warning threshold |
| --- | --- | --- |
| `memory` | `ResourceUsage.memoryUsedPercentage` | `bar.resources.memoryWarningThreshold` = 95 |
| `swap_horiz` | `ResourceUsage.swapUsedPercentage` | 85 |
| `planner_review` | `ResourceUsage.cpuUsage` | 90 |

Each `Resource` is a 20 px ring exactly like the media one — `lineWidth` 2,
`enableAnimation: false`, icon at `pixelSize.normal` (16) with `fill: 1` and
`font.weight: Font.DemiBold` — followed by a number.

```
warning   = percentage·100 >= warningThreshold
colPrimary = warning ? colError : colOnSecondaryContainer
accountForLightBleeding = !warning
```

The number is drawn inside a fixed-width box: a `TextMetrics` of the string
`"100"` at `pixelSize.small` (15) sets the width, and the text is centered in
it, so the row keeps its width as the digits change. Its color is
`colOnLayer1`, spacing to the ring is 2.

`shown: false` slides the whole row out to `x = -width` with **`elementMove`**
(500 ms) and the item collapses to zero width; `implicitWidth` has the same
animation. Nothing in the bar sets `shown` false, but the animation is on
`implicitWidth` too, so a resource appearing or disappearing animates.

The reading comes from `services/ResourceUsage.qml`: `/proc/meminfo`
(`MemTotal`, `MemAvailable`, `SwapTotal`, `SwapFree`) and `/proc/stat` cpu
line deltas, polled at `resources.updateInterval` = **3000 ms**. The first
tick fires after 1 ms, then the timer reschedules itself to the interval.

Hovering the row opens `ResourcesPopup` (§10.2).

**Status (proscenio).** Rings, colors, thresholds, the fixed-width number and the
hover popup are implemented. The "100" slot is measured in the application
font from `kdeglobals`, as in Qt. The CPU reading uses
the first seven `/proc/stat` fields with `idle` alone as idle time, as the
QML does. `src/ui/widgets/popup.rs` is the `StyledPopup` equivalent: a
non-autohiding `GtkPopover` shown on enter and hidden on leave, pointing at
the bar's bottom edge, styled `.bar-popup`.

---

## 5. Center group — `Workspaces.qml`

Six stacked layers.

### 5.1 Geometry

```
workspaceButtonWidth      26
activeWorkspaceMargin     2
activeWorkspaceSize       26 − 2·2 = 22
workspaceIconSize         26 · 0.69  = 17.94 → rounded to even
workspaceIconSizeShrinked 26 · 0.55  = 14.3
workspaceIconMarginShrinked −4
specialTextSize           26 · 0.5   = 13
widgetPadding             5          (fed back to the enclosing BarGroup)
```

The widget's implicit width is the occupied-indicator layout's width, i.e.
`shown · 26`; its implicit height is `Appearance.sizes.barHeight` — 40 or 50,
not 26. The row of cells is centered in that.

### 5.2 The model — `models/WorkspaceModel.qml`

```
shownCount = bar.workspaces.shown                         (default 10)
activeWorkspace = monitor.activeWorkspace.id
group      = floor((activeWorkspace − 1) / shownCount)
getWorkspaceIdAt(i) = group · shownCount + i + 1
occupied[i] = Hyprland.workspaces.values.some(ws => ws.id === idAt(i))
```

**`occupied` means the workspace exists, not that it has windows.** Hyprland
keeps the focused workspace in its list even when empty. The model excludes
it through a second value:

```
currentWorkspaceNotFake = ToplevelManager.activeToplevel?.activated ?? false
fakeWorkspace = currentWorkspaceNotFake ? −9999 : activeWorkspace
```

and everywhere occupancy is consumed it is guarded with
`&& wsId != fakeWorkspace`. So: a workspace counts as occupied when it exists,
*except* the active one, which counts only when a window is actually
activated on it.

`biggestWindow[i]` is `HyprlandData.biggestWindowForWorkspace(idAt(i))`.

The model recomputes occupancy on `Hyprland.workspaces.valuesChanged`, on
`Hyprland.focusedWorkspaceChanged`, when `group` changes, and at completion.

The monitor binding:

```qml
readonly property HyprlandMonitor monitor:
    Hyprland.monitorFor(root.QsWindow.window?.screen) ?? Hyprland.focusedMonitor
```

`monitorFor()` is a plain function call, so the binding only tracks the window;
touching `focusedMonitor` re-runs it once Hyprland's IPC data lands, otherwise
a window with no screen at creation time keeps `monitor` null forever.

### 5.3 Layer 1 — the occupancy pills

For each cell a `Pill` whose three geometry properties are each animated with
**`elementMoveSmall`** (350 ms, expressiveFastSpatial, overshooting):

```
undirectionalWidth  = 26 · currentOccupied
undirectionalLength = 26 · (1 + 0.5·previousOccupied + 0.5·nextOccupied) · currentOccupied
undirectionalOffset = (!currentOccupied ? 0.5 : −0.5·previousOccupied) · 26
```

with `previousOccupied`/`nextOccupied` being the neighbors' occupancy, each
also guarded against `fakeWorkspace`, and clamped at the ends of the row.

So an occupied cell whose left neighbor is occupied starts half a cell to the
left and is one and a half cells long — adjacent pills overlap exactly, and the
run reads as one continuous capsule. An unoccupied cell collapses to zero size
positioned at the cell's center, so it grows out of the middle.

**The pills are unioned, not drawn one by one.** The layout has
`layer.enabled: true` and `visible: false`; a `MaskMultiEffect` uses it as a
mask over `occupiedIndicatorsBg`, a rectangle of

```
ColorUtils.transparentize(Appearance.m3colors.m3secondaryContainer, 0.4)
```

i.e. `m3secondaryContainer` at alpha 0.6. One translucent rectangle is masked,
so the overlaps do not darken at the seams. The cairo equivalent is a group:
draw the pills opaque into a group, then `paint_with_alpha(0.6)`.

### 5.4 Layer 2 — the active indicator

A `TrailingIndicator` at `z: 2` with `index: (activeWorkspace − 1) % shownCount`.

```
indicatorPosition  = min(idx1, idx2) · 26 + 2
indicatorLength    = |idx1 − idx2| · 26 + 22
indicatorThickness = 22
radius             = 11
color              = colPrimary
```

`idx1`/`idx2` come from `AnimatedTabIndexPair`: 100 ms and 300 ms, both
`OutSine`. The pill therefore stretches toward the new workspace, then the
tail catches up 200 ms later.

### 5.5 Layer 3 — the hover indicator

A second `TrailingIndicator` at `z: 3`, `color: "transparent"`, with

```qml
index: root.containsMouse ? root.hoverIndex : root.workspaceIndexInGroup
```

so when the pointer leaves it slides back under the active workspace rather
than vanishing. `hoverIndex = floor(mouseX / 26)`.

Inside it a `StateOverlay` fills the indicator rectangle, radius 11, with

```qml
hover: root.containsMouse
press: root.containsPress
drag:  true                      // always on
contentColor: Appearance.colors.colPrimary
```

Total opacity of `colPrimary` over the cell: **0.16 idle, 0.24 hovered, 0.34
hovered and pressed.** The `drag` layer is on unconditionally, so the trailing
indicator is faintly visible even when the pointer is elsewhere in the bar.

### 5.6 Layer 4 — numbers and dots

`WorkspaceLayout` at `z: 4` with `layer.enabled: true`, one
`NumberWorkspaceItem` per cell. Each shows **either** a dot or a number,
cross-fading through two `FadeLoader`s (200 ms):

```
showingNumbers =
    superPressAndHeld                                   ? true
  : GlobalStates.screenLocked                           ? false
  : alwaysShowNumbers && (!showAppIcons || !hasBiggestWindow) ? true
  : false
```

The dot is a `Circle` of diameter `26 · 0.18` = 4.68 px.

The number's font:

```
pixelSize = pixelSize.small − (text.length − 1)·(text !== "10")·2
family    = useNerdFont ? font.family.iconNerd : font.family.main
text      = bar.workspaces.numberMap[wsId − 1] || wsId
```

so two-character labels shrink by 2 px — except literally `"10"`, which keeps
the full 15 px.

Color for both:

```
contentColor = (occupied[i] && wsId !== fakeWorkspace)
             ? colOnSecondaryContainer
             : colOnLayer1Inactive
```

There is no active-workspace case here; layer 5 handles it.

### 5.7 Layer 5 — the recoloring pass

```qml
Colorizer {
    z: 5
    anchors.fill: numbersGrid
    colorizationColor: Appearance.colors.colOnPrimary
    sourceColor: Appearance.colors.colOnSecondaryContainer
    source: activeIndicator
    maskEnabled: true
    maskSource: numbersGrid
    maskThresholdMin: 0.5
    maskSpreadAtMin: 1
}
```

This draws the **active indicator** through the numbers layer as a mask, tinted
`colOnPrimary`. The effect: whatever part of the numbers/dots falls inside the
active pill is repainted in `colOnPrimary`.

During the 100/300 ms stretch the pill covers *several* cells, and every dot
under it is `colOnPrimary` for the duration. A per-index rule ("the dot at the
active index is `colOnPrimary`") flips a single dot instead.

### 5.8 Layer 6 — application icons

One per cell at `z: 6`, anchored to the cell's bottom-right corner with

```
cornerMargin = (!superPressAndHeld && showAppIcons && biggestWindow)
             ? (26 − workspaceIconSize) / 2      // ≈ 4
             : −4                                // workspaceIconMarginShrinked
```

animated with `elementMoveSmall` (350 ms); plus
`(parent.implicitHeight − 26) / 2` on the bottom and the same shape on the
right, which recenters the icon inside the 40 px tall cell.

The icon itself is invisible (`visible: false`) — what is drawn is a
`Colorizer` copy of it, so the mask and the tint apply cleanly:

```
colorizationColor = darkmode ? colOnSecondaryContainer : colOnPrimary
colorization      = monochromeIcons ? 0.8 : 0.5
opacity = !showAppIcons ? 0
        : (biggestWindow && !superPressAndHeld) ? 1
        : biggestWindow ? workspaceIconOpacityShrinked (1) : 0
scale   = ((!superPressAndHeld && showAppIcons) ? 17.94 : 14.3) / 17.94
```

`opacity` animates with `elementMoveFast` (200 ms), `scale` with
`elementMoveSmall` (350 ms). The mask is a `Circle` of the icon's size, so
icons are clipped round. `implicitSize` is `NumberUtils.roundToEven(17.94)`
= 18. `animated: !biggestWindow` is set only to stop the "image-missing"
placeholder animating.

The icon source is `Quickshell.iconPath(AppSearch.guessIcon(biggestWindow.class), "image-missing")`.

### 5.9 The special workspace

When `specialWorkspaceActive`, the whole regular stack blurs out and a pill
takes its place.

`specialBlur` is 1 when the special workspace is active **and the pointer is
not over the widget**, 0 otherwise, animated with `elementMoveSmall` (350 ms).
The regular layer then gets

```
scale: 1 − 0.08·specialBlur
layer.effect: MultiEffect { brightness: −0.1·specialBlur; blurEnabled: true; blur: specialBlur; blurMax: 32 }
```

so hovering the widget while the special workspace is up brings the normal
workspaces back into focus.

Over it, a `FadeLoader` shown while the special workspace is active, with
`scale: 0.8 + 0.2·specialBlur` and `opacity: specialBlur`, with an empty
`Behavior on opacity {}`: only `specialBlur` animates. It holds a `Pill` of
`colPrimary`:

```
undirectionalWidth  = 22                          (activeWorkspaceSize)
undirectionalLength = specialWsText.implicitWidth + 22
```

animated with `elementMoveEnter` (400 ms), with the workspace name (special
prefix stripped) in `colOnPrimary` at `pixelSize` 13.

### 5.10 Input

`ButtonMouseArea` — hover enabled, pointing-hand cursor. Accepted buttons are
left, right and **back**.

| Input | Action |
| --- | --- |
| Left | `hl.dsp.focus({workspace = <id at hoverIndex>})` |
| Right | toggle `GlobalStates.overviewOpen` |
| Back (button 8) | `hl.dsp.workspace.toggle_special("special")` |
| Wheel down | `hl.dsp.focus({workspace = "r+1"})` |
| Wheel up | `hl.dsp.focus({workspace = "r-1"})` |

`BarContent.qml` layers a second `MouseArea` over the widget that also toggles
the overview on right-click; the two agree.

The dispatches use Hyprland's Lua dispatcher names (`hl.dsp.*`), not the
classic `workspace 3` syntax.

### 5.11 Super-hold

A timer with interval `autoHide.showWhenPressingSuper.delay` (140 ms) starts
when `GlobalStates.superDown` becomes true and sets `superPressAndHeld`; the
release clears it, and `onSuperReleaseMightTriggerChanged` stops the timer.
While held: numbers replace dots, and the app icons shrink to
`workspaceIconSizeShrinked` at margin −4.

**Status (proscenio).** `src/panels/bar/workspaces.rs` draws the occupancy pills with
the same three animated properties and the same 350 ms curve, the trailing
active indicator at 100/300 ms `OutSine`, the hover indicator with the same
drag+hover+press state layers, and the dot/number pair cross-fading over
200 ms under the same `showingNumbers` rule, with the number map, the nerd
font option and the two-character size drop.

Layer 5 is reproduced as a real mask, not a per-index rule: the marks are
drawn once in their own colors, then the active capsule is set as a clip and
they are drawn again in `colOnPrimary`. Dots under the stretching pill change
color as it passes over them, as in §5.7.

App icons are implemented in `src/platform/appicon.rs`: the class is resolved through
the substitution table, the desktop entries and the icon theme, the icon is
rasterized through the widget's own GSK renderer, colorized in HSL exactly as
`MultiEffect.colorization` does, cached, and drawn under a circular clip with
the 350 ms corner margin and the 200 ms opacity.

The special workspace is implemented: the regular layers are rendered into an
image surface padded by three blur radii, blurred by three box passes of radius
`blur·32/5` that treat everything outside as transparent, darkened by 0.1 and
scaled by `1 − 0.08·blur`, with the name pill over it at `0.8 + 0.2·blur`
scale. The widget is a `Paint` whose cairo node reaches 32 px past its
allocation on every side, so the blur spreads past the 26 px strip as
`MultiEffect`'s padding lets it in qs.
Back-button toggles the special workspace.

Super-hold follows `States.super_down` through the same delay timer, and
right-click toggles proscenio's overview. Occupancy is the QML's rule: a workspace
is occupied when it exists and is not the fake one, the active workspace while
no window is active. Dots and numbers are centered with Qt's rounding relative
to the 40 px bar, and the active mark takes the `colorized` color the QML
computes for it.

---

## 6. Center-right group — clock and utilities

The group is wrapped in a `MouseArea` whose `onPressed` toggles
`GlobalStates.sidebarRightOpen`. So **pressing anywhere on the clock group —
the clock, the util buttons, the battery — opens the right sidebar**, unless a
child consumes the press first (the clock does; see below).

`implicitWidth: centerSideModuleWidth`, same as the left group.

### 6.1 `ClockWidget.qml`

```
StyledText { pixelSize: large (17); color: colOnLayer1; text: DateTime.time }
StyledText { pixelSize: small (15); color: colOnLayer1; text: "•" }     visible: showDate
StyledText { pixelSize: small (15); color: colOnLayer1; text: DateTime.longDate }  visible: showDate
```

Row spacing 4, centered in an item of height `barHeight`.
`showDate: bar.verbose && useShortenedForm < 2`.

Formats come from `services/DateTime.qml` via the config `time` section:
`time.format` (default `"hh:mm"`), `time.dateFormat` (default
`"dddd, dd/MM"`), in Qt locale format strings. Precision is minutes unless
`time.secondPrecision` or the screen is locked.

Its own `MouseArea` toggles `GlobalStates.calendarOpen` on press —
consuming the press, so a click on the clock text opens the calendar rather
than the sidebar. `hoverEnabled: !GlobalStates.calendarOpen`. It hosts
`ClockWidgetPopup` (hover) and `CalendarPopup` (click).

### 6.2 `RecordingIndicator.qml`

`visible: ScreenRecording.active`. A `RippleButton`, height 26, horizontal
padding 8, vertical padding 0, radius `full`, colored
`colError`/`colErrorHover`/`colErrorActive`, content a filled `stop_circle` at
`pixelSize.large` (17) plus the elapsed time at `pixelSize.smaller` (12)
nudged up 1 px, both in `colOnError`. Clicking stops the recording. Tooltip
"Stop the recording", anchored below the bar (or above when the bar is at the
bottom).

### 6.3 `UtilButtons.qml`

`visible: bar.verbose && useShortenedForm === 0`. A row, spacing 4, of
`CircleUtilButton`s — a `RippleButton` whose width equals its height, at least
26 px. Every icon is at `pixelSize.large` (17) in `colOnLayer2`.

| Config flag | Default | Icon | `fill` | Action |
| --- | --- | --- | --- | --- |
| `showUpdates` | true | `deployed_code_update` | 1 | `AppLaunch.shell(apps.update)`; shown only when `Updates.updateAdvised`, colored `colError` when `updateStronglyAdvised`, tooltip "%1 packages can be updated" |
| `showScreenSnip` | true | `screenshot_region` | 1 | `hl.dsp.global("quickshell:regionScreenshot")` |
| `showScreenRecord` | false | `videocam` | 1 | runs `Directories.recordScriptPath`, then `ScreenRecording.watch()` |
| `showColorPicker` | false | `colorize` | 1 | `hyprpicker -a` |
| `showKeyboardToggle` | true | `keyboard` | 0 | toggle `GlobalStates.oskOpen` |
| `showMicToggle` | false | `mic` / `mic_off` | 0 | `wpctl set-mute @DEFAULT_SOURCE@ toggle` |
| `showDarkModeToggle` | true | `light_mode` / `dark_mode` | 0 | `wallpaperSwitchScriptPath --mode light|dark --noswitch` |
| `showPerformanceProfileToggle` | false | `energy_savings_leaf` / `airwave` / `local_fire_department` | 0 | cycles the UPower power profile |

The power profile cycles PowerSaver → Balanced → Performance → PowerSaver when
a performance profile exists, otherwise toggles Balanced ↔ PowerSaver.

### 6.4 `BatteryIndicator.qml`

`visible: useShortenedForm < 2 && Battery.available`. A `ClippedProgressBar`
whose highlight is `m3error` when `percentage <= battery.low/100` (20 %) and
not charging, else `colOnSecondaryContainer`. Inside the bar's value area, a
filled `bolt` at `pixelSize.smaller` (12) shown while charging below 100 %,
and the percentage text. Hover opens `BatteryPopup` (§10.3).

**Status (proscenio).** The clock, its Qt→strftime format translation, the
group-wide press-to-open-sidebar, the hover popup (date, uptime, the first
five unfinished to-dos read from the same `todo.json`), the eight util
buttons and the battery indicator are implemented. The keyboard toggle runs
the `oskToggle` action ([osk.md](osk.md)). The clock follows `time.format`, `time.dateFormat` and
`time.secondPrecision` as they change in `config.toml`, and re-arms its tick
when the precision changes. `src/services/battery.rs` reads UPower's
`DisplayDevice` and takes health from the first real battery's `Capacity`.
When a level is crossed it acts as qs's `Battery.qml` does: "Low battery" and
"Critically low battery" notifications while discharging, "Battery full"
while charging, a suspend at `battery.suspend` when automatic suspend is on,
and, with `sounds.battery`, the `dialog-warning`, `suspend-error`, `complete`
and plug or unplug sounds. The first reading only records the charger state,
so no plug sound plays at start. `src/services/updates.rs` polls
`checkupdates` on the configured interval.

The util buttons are 26 px `RippleButton`s with the tooltip, record and dark
mode wired as in the QML. The screen snip button runs proscenio's own
`regionScreenshot` action, which opens the region selector.
`src/services/recording.rs` ports `ScreenRecording` (the running
`wf-recorder`, found in `/proc/*/comm` rather than through `pgrep`, the 250 ms
confirmation checks, the elapsed-seconds ticker) and `src/panels/bar/recording.rs` is the indicator with its tooltip.

The calendar panel is `src/panels/calendar/mod.rs`: one overlay layer surface per
monitor, namespace `proscenio:calendarPanel`, 420 wide inside a 10 px elevation
margin, anchored top-left with the top margin at the bar height and the left
margin computed on open to center the panel under the clock, as
`CalendarPopup.reposition` does. `colLayer0` with its 1 px
border and radius 17 outside, `colLayer1` radius 17 inside with 10 px of
padding. It takes a dismissible focus grab while open and closes on Escape.

`src/panels/calendar/month.rs` is the month grid: the header button carrying
`• MMMM yyyy` and jumping back to this month, the two round chevrons, the
`Mo…Su` row and six rows of seven 38 px cells with radius 12, today filled
`colPrimary`, the neighboring months in `colOutlineVariant`. The layout is
`calendar_layout.js` ported function for function, including its
`getMonthDays` parity calculation. Scrolling over the grid moves a month.

Beside the grid is the collapsed navigation rail from `NavigationRailButton`:
56 px cells, the icon at 24 filling and turning `m3onSecondaryContainer` when
selected, the 14 px label under it. The panel carries the minimum height of
350 the QML gives `CalendarPanelContent`.

The To Do tab is `src/panels/calendar/todo.rs` over `src/services/todo.rs`, which reads and
writes the same `todo.json` the QML service does. Two secondary tabs —
Unfinished and Done — with the 3 px `colPrimary` underline, the task cards in
`colLayer2` at radius 12 with their check and delete buttons, the 55 px
placeholder when a tab is empty, the 48 px `add` floating button, and the add
dialog over a scrim with its outlined entry and Cancel/Add pair. Adding a task
returns to the Unfinished tab, as the QML does.

The Timer tab, the tab-switch slide and Ctrl+PageUp/PageDown are in
[calendar-panel.md](calendar-panel.md).

---

## 7. The left region — brightness

`barLeftSideMouseArea`, a `FocusedScrollMouseArea` spanning the left edge to
the center group.

```
onScrollDown: Brightness.decreaseBrightness()
onScrollUp:   Brightness.increaseBrightness()
onMovedAway:  GlobalStates.osdBrightnessOpen = false
```

Brightness and gamma are one continuum. `increaseBrightness` raises
`Hyprsunset.gamma` by 5 first if it is below 100, and only then raises the
focused monitor's backlight by 0.05. `decreaseBrightness` lowers the backlight
by 0.05 while it is above 0, and below that lowers the gamma by 5.

A `ScrollHint` sits at the left edge, revealed on hover: a `Revealer`
containing a column (spacing −5, so the glyphs overlap) of
`keyboard_arrow_up`, the state icon, `keyboard_arrow_down`, all at
`iconSize: 14` in `colSubtext`. The state icon is `light_mode` when
`Hyprsunset.gamma === 100`, `wb_twilight` otherwise. After 500 ms of hover a
tooltip "Scroll to change brightness" appears.

**Status (proscenio).** `left_section` in `src/panels/bar/mod.rs` is the `Strip`'s start
child and covers the space left of the middle section, as the QML anchor to
`middleSection.left` does. It carries the wheel and the hint. `Light::raise` and `Light::lower` are the two QML functions verbatim:
gamma first while it is below 100, backlight after; backlight first on the way
down, gamma once the backlight is at zero. The hint is
`src/ui/widgets/scrollhint.rs` — a revealer over a column spaced −5 with the
three 14 px glyphs in `colSubtext`, its middle one following the gamma.

The hint is an overlay and does not change the bar's height. Its tooltip
appears below it after 500 ms of hover. The hint stays shown while the region
is hovered.

---

## 8. The right region

`barRightSideMouseArea`, a `FocusedScrollMouseArea` from the center group to
the right edge.

```
onScrollDown: Audio.decrementVolume()
onScrollUp:   Audio.incrementVolume()
onMovedAway:  GlobalStates.osdVolumeOpen = false
onPressed:    if left button → toggle GlobalStates.sidebarRightOpen
```

The volume step is 0.01 below 10 % volume and 0.02 above; increase clamps at
1.0, decrease does not clamp at 0.

Its `ScrollHint` is at the right edge with the icon `volume_up` and the
tooltip "Scroll to change volume".

**Status (proscenio).** `right_section` carries the wheel with the same two steps
— 0.01 below 10 %, 0.02 above, clamped at 1.0 going up and unclamped going
down — and the hint at its right end, overlaid in the 23 px right margin of
the sidebar button as in the QML.

The wheel notch after the volume OSD maps or unmaps is lost; qs keeps it.
Hyprland 0.56 sends `wl_pointer.motion` without the required
`wl_pointer.frame` from `CInputManager::simulateMouseMovement`, which runs
whenever a layer surface or popup maps or unmaps. GTK holds that motion as
the pending frame event; at the next frame it delivers the motion and drops
the wheel notch that arrived with it. Qt dispatches the motion at once.

**The whole region is the hover target.** The sidebar button's background
follows `barRightSideMouseArea.hovered`: pointing anywhere in the right half
of the bar, including over the tray, lights the button up.

Inside it a `RowLayout` with `layoutDirection: Qt.RightToLeft`, spacing 5,
holding in visual right-to-left order: the sidebar button, the tray, a
`fillWidth` spacer, and the weather group.

### 8.1 The sidebar button — `rightSidebarButton`

A `RippleButton` with

```
Layout.rightMargin: Appearance.rounding.screenRounding      // 23
implicitWidth:  indicatorsRowLayout.implicitWidth + 10·2
implicitHeight: indicatorsRowLayout.implicitHeight + 5·2
buttonRadius:   Appearance.rounding.full
toggled:        GlobalStates.sidebarRightOpen
```

so padding 10 horizontal, 5 vertical, fully rounded. The right margin is the
*screen* rounding, not a layout constant.

The colors are the four-state set from §foundations 4:

```
colBackground             = barRightSideMouseArea.hovered
                            ? colLayer1Hover
                            : transparentize(colLayer1Hover, 1)
colBackgroundHover        = colLayer1Hover
colRipple                 = colLayer1Active
colBackgroundToggled      = colSecondaryContainer
colBackgroundToggledHover = colSecondaryContainerHover
colRippleToggled          = colSecondaryContainerActive
```

The resting background is already `colLayer1Hover` when the *region* is
hovered, independent of the button's own `hovered`. `colBackground` and
`colBackgroundHover` are therefore the same color whenever the button is
hovered, and the visible transition is region-wide.

The content color is a property with its own animation:

```qml
property color colText: toggled ? Appearance.m3colors.m3onSecondaryContainer
                                : Appearance.colors.colOnLayer0
Behavior on colText { elementMoveFast.colorAnimation }      // 200 ms
```

Every icon inside binds to `rightSidebarButton.colText`, so they all cross-fade
together when the sidebar opens.

Pressing toggles `GlobalStates.sidebarRightOpen` — and so does pressing
anywhere else in the region, via the enclosing `FocusedScrollMouseArea`.

### 8.2 The indicators

A `RowLayout` centered in the button, `spacing: 0`, with a
`property real realSpacing: 15` applied as explicit per-item margins. Reading
left to right as drawn:

| Item | Shown when | Spacing |
| --- | --- | --- |
| `volume_off` | `Audio.sink.audio.muted` | `rightMargin: reveal ? 15 : 0` |
| `mic_off` | `Audio.source.audio.muted` | `rightMargin: reveal ? 15 : 0` |
| `HyprlandXkbIndicator` | more than one xkb layout | `rightMargin: 15` always |
| notification count | `Notifications.silent \|\| Notifications.unread > 0` | `rightMargin: reveal ? 15 : 0` |
| network symbol | always | — |
| WireGuard icon | a WireGuard connection is up | `leftMargin: reveal ? 15 : 0` |
| bluetooth symbol | `BluetoothStatus.available` | `leftMargin: 15` |

The first four and the WireGuard one are wrapped in `Revealer`s, so they slide
in and out (400 ms `elementMoveEnter`), and the margin animates separately with
**`elementMoveFast`** (200 ms): the width opens over 400 ms, the gap over 200.

All the icons are `MaterialSymbol` at `pixelSize.larger` (**19**) in
`colText`.

**Network** (`services/Network.qml`): `lan` when ethernet, else by Wi-Fi state —
connected and enabled picks from signal strength (>83 `signal_wifi_4_bar`,
>67 `network_wifi`, >50 `network_wifi_3_bar`, >33 `network_wifi_2_bar`,
>17 `network_wifi_1_bar`, else `signal_wifi_0_bar`); connecting
`signal_wifi_statusbar_not_connected`; disconnected `wifi_find`; disabled
`signal_wifi_off`; anything else `signal_wifi_bad`.

**Bluetooth** (`services/BluetoothStatus.qml`): `bluetooth_connected` when any
device is connected, `bluetooth_disabled` when the default adapter is off,
`bluetooth` otherwise. `available` is "at least one adapter exists".

**Keyboard layout** (`HyprlandXkbIndicator.qml`): active only when there is
more than one layout. The text is the layout code split on `:`, each part cut
to its first four characters before any `-`, upper-cased, joined with a
newline; the font drops from `pixelSize.small` (15) to `smallie` (13) when it
contains a newline. `animateChange: true`, so a layout switch slides the text.

**Notifications** (`NotificationUnreadCount.qml`): the glyph is
`notifications_paused` when silent, else `notifications`. Over its top-right
corner sits a badge, visible when not silent and `unread > 0`, colored
`colOnLayer0`, fully rounded. With `bar.indicators.notifications.showUnreadCount`
(default **false**) it is 8×8 px at margins right 1, top 3; with the count
shown it grows to fit the number, sits flush in the corner, and carries the
count at `pixelSize.smallest` (10) in `colLayer0`.

**WireGuard**: a `CustomIcon` `wireguard-symbolic`, colorized to `colText`,
sized 19×19. Its state comes from polling
`nmcli connection show --active | grep -q WireGuard` every
`resources.updateInterval` (3000 ms).

**Status (proscenio).** `src/panels/bar/mod.rs` builds the pill as a `RippleButton` with the
same padding (5 px × 10 px), full rounding and the 23 px end margin, and the
four background states with the same color expressions, including the
`contentTransparency` alpha the hover colors carry. Hover is taken from the
whole right region, matching `barRightSideMouseArea`; a press on the button
itself is not passed on to the region, so it toggles the sidebar once. Mute, mic, xkb, network
and bluetooth indicators exist at 19 px with the 15 px spacing, and
`src/ui/widgets/reveal.rs` animates width over 400 ms `emphasizedDecel` and the
margin over 200 ms `expressiveEffects`.

The WireGuard indicator is implemented on the same two-speed reveal. It runs
`nmcli connection show --active` once at start and again whenever the network
service reports a NetworkManager change, instead of every 3 s, and draws the shell's own
`wireguard-symbolic.svg` as a symbolic `GFileIcon`, which GTK recolors. The
`colText` cross-fade is a 200 ms `transition: color` on `.indicator-pill
label` with the `expressiveEffects` curve.

The notification indicator and its badge are implemented against `proscenio`'s own
daemon ([notifications.md](notifications.md)), including the two badge shapes,
the reveal rule `silent || unread > 0`, and the reset of the unread count when
the sidebar opens. It is a drawn widget that reads its ink color from CSS
(`widget.color()` on the `.notify-icon` node), so it cross-fades over the same
200 ms as its neighbors.

### 8.3 Weather

A `Loader` active on `bar.weather.enable` (default false), left margin 4,
holding a `BarGroup` with a `WeatherBar`: a row of the weather symbol at
`pixelSize.large` (17) and the temperature at `small` (15), both
`colOnLayer1`, inset 10 on each side. Right-click refetches and sends a
"Refreshing (manually triggered)" notification. Hovering opens a
`StyledPopup`: the city behind a `location_on`, the temperature and "Feels
like", a two-column grid of eight cards — UV index, wind, precipitation,
humidity, visibility, pressure, sunrise, sunset — each a `colSurfaceContainer
High` rectangle of radius 12 with 14 px padding and its value pulled 10 px up
under the title, and the last-refresh line at the bottom.

`services/Weather.qml` builds `curl -s wttr.in/<city>?format=j1 | jq '…'`,
refetches every `fetchInterval` minutes (10), and picks metric or US units
from `useUSCS`. The weather code maps to a Material symbol through
`Icons.weatherIconMap`.

**Status (proscenio).** `src/services/weather.rs` fetches the same URL and reads the same
three objects — `current_condition[0]`, `nearest_area[0]` and
`weather[0].astronomy[0]` — directly from the JSON, without `jq`, with the
same unit split and the same fallbacks. `src/panels/bar/weather.rs` draws the
pill and the popup, and `weather::symbol` is the icon map verbatim. The cards
are 4 px wider than in qs: Pango advances the 12 px text further than Qt.

With `bar.weather.enableGPS`, the location comes from GeoClue2 over the
system bus (`src/platform/geoclue.rs`). The client asks for exact accuracy
under the desktop id `proscenio`, and each `LocationUpdated` stores the
coordinates and fetches `wttr.in/<lat>,<long>`; the periodic fetch keeps
using them. When GeoClue is missing or refuses, the "Cannot find a GPS
service" notice is sent and the configured city is used, as in the QML.
GeoClue runs only while the weather is enabled.

---

## 9. The tray — `SysTray.qml`

`visible: useShortenedForm === 0`. A `GridLayout`, one row, **`columnSpacing:
15`** (`rowSpacing: 8` for the vertical bar).

### 9.1 Pinned and unpinned

`services/TrayService.qml`:

```
itemsInUserList    = items where tray.pinnedItems includes id  (and status ≠ Passive if filterPassive)
itemsNotInUserList = the complement, same passive filter
pinnedItems   = invertPinnedItems ? itemsNotInUserList : itemsInUserList
unpinnedItems = invertPinnedItems ? itemsInUserList    : itemsNotInUserList
```

With the shipped defaults — `invertPinnedItems: true`, `pinnedItems: ["Fcitx"]`
— the list is a **blacklist**: everything shows in the bar except Fcitx, which
goes into the overflow. `filterPassive: true` hides items whose status is
Passive.

`getTooltipForItem` is `tooltipTitle`, else `title`, else `id`; plus
`" • " + tooltipDescription` when there is one; plus `"\n[id]"` when
`tray.showItemId`.

### 9.2 The overflow button

Visible when `showOverflowMenu && unpinnedItems.length > 0`. A `RippleButton`
with a fixed 24×24 background centered on itself, toggled colors
`colSecondaryContainer` / `…Hover` / `…Active`, content the `expand_more`
symbol at `pixelSize.larger` (19), colored `colOnSecondaryContainer` when
open else `colOnLayer2`.

Its rotation encodes the state and the bar's orientation:

```
rotation = (open ? 180 : 0) − (90 · vertical) + (180 · invertSide)
```

animated with `elementMoveFast` (200 ms). `invertSide` is
`Config.options.bar.bottom`.

Opening it shows a `StyledPopup` containing a grid of the unpinned items with
`columns: ceil(sqrt(count))` and 10 px spacing.

### 9.3 A tray item — `SysTrayItem.qml`

A 20×20 `MouseArea`, hover enabled, accepting left and right.

| Button | Action |
| --- | --- |
| Left | `item.activate()` |
| Right | open the item's menu, or close it if already open |

The event is always accepted.

The icon is an `IconImage` filling the 20×20. With `tray.monochromeIcons`
(default **true**) a second copy is drawn instead: `Desaturate` at 0.8 followed
by a `ColorOverlay` of `transparentize(colOnLayer0, 0.9)`: mostly gray icons
with a 10 %-opacity tint of the foreground color.

The tooltip text is refreshed on `onEntered`, and the tooltip anchors below the
bar unless the bar is at the bottom or vertical.

### 9.4 The separator

At the end of the grid, a `StyledText` `"•"` at `pixelSize.larger` (19) in
`colSubtext`, visible when `showSeparator && SystemTray.items.values.length > 0`.

### 9.5 Focus grab

The tray owns a `HyprlandFocusGrab` of its own, separate from
`GlobalFocusGrab`, over `[the overflow popup window, the currently open menu]`.
Opening a second item's menu closes the first (`setExtraWindowAndGrabFocus`).
`onCleared` closes both the overflow and the menu. When the unpinned list
empties the overflow closes itself.

### 9.6 The menu — `SysTrayMenu.qml`

A `PopupWindow`, transparent, `padding: Appearance.sizes.elevationMargin` (10),
anchored under the item (gravity and edges flip to `Top` when the bar is at the
bottom, and to `Left`/`Right` when vertical).

The background is a `Rectangle` of `colLayer0`, radius
`Appearance.rounding.windowRounding` (18), 1 px `colLayer0Border` border,
`clip: true`, inner padding 4. It fades in from `opacity: 0` on completion with
`elementMoveFast` (200 ms) and resizes with **`elementResize`** (300 ms,
emphasized) on both axes. A `StyledRectangularShadow` sits behind it.

Submenus live in a `StackView` with **all four transitions set to zero-duration**.
Each `SubMenu` fades its own opacity (`elementMoveFast`), driven by
`StackView.onActivating` / `onDeactivating`.

Right-click or the back button inside the menu pops one level.

Every menu, at depth 1, gets a **pin entry** prepended: a row with a `push_pin`
icon reading "Pin"/"Unpin" that calls `TrayService.togglePin(id)`, followed by
a 1 px `colSubtext` separator with 4 px margins. A submenu gets a "Back" row
with `chevron_left` instead.

All rows are `RippleButton`s of height 36, horizontal padding 12, radius
`popupBackground.radius − popupBackground.padding` = 18 − 4 = **14**.

### 9.7 A menu row — `SysTrayMenuEntry.qml`

Three columns, spacing 8:

1. A 20×20 interaction slot — a disabled `StyledRadioButton` for
   `QsMenuButtonType.RadioButton`, or a `check` / `check_indeterminate_small`
   symbol at 20 px for a checked or partially-checked `CheckBox`. Present on
   every row of the menu if *any* row needs it (`forceSpecialInteractionColumn`).
2. A 20×20 icon slot, `IconImage`, mipmapped, asynchronous. Same
   all-or-nothing rule (`forceIconColumn`).
3. The label at `pixelSize.smallie` (13), `Layout.fillWidth`.
4. A `chevron_right` at 20 px when the entry has children.

A separator entry becomes a 1 px row colored `m3outlineVariant` with 4 px
margins above and below, `enabled: false`.

Activating a row with children pushes the submenu; otherwise it calls
`triggered()` and closes the whole menu. Right-click is *not* consumed
(`altAction` sets `event.accepted = false`).

**Status (proscenio).** `src/panels/bar/tray.rs` and `src/panels/bar/traymenu.rs`
implement the item list with the pinned/unpinned split, the 15 px spacing, the
`•` separator, right-click menus over DBusMenu (`src/platform/dbusmenu.rs`) with
submenus, checkboxes, radio buttons, icons and separators; the overflow
button with its `ceil(sqrt(n))` grid popup; per-item tooltips built from
`ToolTip`, `Title` and `Id` by the same precedence; monochrome icons
(desaturate 0.8 then a 10 % wash of `colOnLayer0`, computed on the rasterized
icon); and the pin/unpin entry, which rewrites `tray.pinnedItems` in the
config file and rebuilds the row.

The overflow chevron rotates 180° over 200 ms inside a `Centred`, which draws
its child through a transform. The menu follows `SysTrayMenu`: a 10 px margin
around a `colLayer0` background of radius 18, the pages stacked in a
`Viewport` whose width is the widest page and whose height animates over
300 ms `emphasized`, a 200 ms fade, 36 px rows of radius 14, and the pin
entry hidden below the first level. Tray icons are looked up in the Qt icon
theme from `kdeglobals`, and Breeze's `current-color-scheme` stylesheet is
filled with the `kdeglobals` colors, as KDE's platform theme does for Qt.

As in the QML, the menu ignores a DBusMenu entry's `enabled` flag: disabled
entries look and act like the rest. Left-click on a tray item does nothing;
proscenio does not call `activate()`.

The shell is the session's `org.kde.StatusNotifierWatcher` itself
(`src/services/statusnotifierwatcher.rs`, Plasma 6.7's kded module): it owns
the name with `REPLACE`, takes `RegisterStatusNotifierItem` with a bus name
(path `/StatusNotifierItem`) or with an object path on the caller's unique
name, registers the item only when its bus name has an owner, drops every
item of a name once that name leaves the bus, and answers
`IsStatusNotifierHostRegistered` with true and `ProtocolVersion` with 0.

---

## 10. Popups

### 10.1 `StyledPopup.qml`

A `LazyLoader` whose `active` is `hoverTarget.containsMouse`: the popup appears
on hover and is destroyed on leave, with no delay and no close animation.

The window is a `PanelWindow` on `WlrLayer.Overlay`, namespace
`quickshell:popup`, `exclusiveZone: 0`, `exclusionMode: Ignore`, masked to the
background rectangle only. It is anchored to the bar's edge and positioned by

```qml
margins.left: QsWindow.mapFromItem(hoverTarget,
                  (hoverTarget.width − popupBackground.implicitWidth) / 2, 0).x
margins.top:  Appearance.sizes.barHeight
```

— centered under the hovered item.

The background is `m3surfaceContainer`, radius `Appearance.rounding.small`
(12), 1 px `colLayer0Border` border, inner margin 10, with `elevationMargin`
(10) of space around it for the shadow.

Rows come in two shapes: `StyledPopupHeaderRow` (icon at `pixelSize.large`
(17) + DemiBold label at `pixelSize.normal` (16), both `colOnSurfaceVariant`,
spacing 5) and `StyledPopupValueRow` (icon, label, and a right-aligned value,
spacing 4, same color).

### 10.2 `ResourcesPopup.qml`

Three columns side by side, spacing 12: RAM, Swap (hidden when
`swapTotal === 0`) and CPU. RAM and Swap each show Used / Free / Total
formatted as `(kb / 1024²).toFixed(1) + " GB"`; CPU shows Load as a rounded
percentage.

### 10.3 `BatteryPopup.qml`

Header "Battery", then time-to-full or time-to-empty (hidden when the state is
4 — fully charged — or the time or power is zero), then the charge/discharge
rate in watts, then health to one decimal.

### 10.4 `ClockWidgetPopup.qml`

The full date, the uptime from `/proc/uptime`, and up to five unfinished
to-dos from `Todo.list`, with "… and N more" when there are more.

**Status (proscenio).** The resources, battery and clock popups are implemented
through `src/ui/widgets/popup.rs`, with the same rows, the same icons and the
same wording, and they update live while open. They are `GtkPopover`s, not
layer surfaces: GTK positions them relative to the hovered widget, pointed at
the bar's bottom edge, with no explicit margin. The 9 px CSS padding plus the
1 px border equal the QML's 10 px inner margin, since Qt draws the border
inside the rectangle. There is no open or close animation, as in the QML.

---

## 11. Parity checklist

| Area | State |
| --- | --- |
| Window, anchors, exclusive zone, hug corners | done |
| Input region grown by `hoverRegionWidth` | done |
| Auto-hide, Super-to-reveal, global shortcuts | done |
| `hyprlandGapsOut` | done |
| Float style: shadow | done |
| Shortened forms at 1200 / 1000 px | done |
| Group pill geometry and color | done |
| Media ring and transport | done |
| Resources rings, thresholds, fixed-width number | done |
| Resources / battery / clock hover popups | done |
| Workspaces: occupancy pills, active + hover indicators | done |
| Workspaces: dots, numbers, cross-fade, number map, nerd font | done |
| Workspaces: mask-based recoloring of the covered marks | done |
| Workspaces: app icons, colorization, circular mask | done |
| Workspaces: special workspace pill and blur | done |
| Workspaces: back-button special toggle | done |
| Workspaces: super-hold | done |
| Workspaces: right-click overview | done |
| Clock text and formats | done |
| Calendar on click | done |
| Util buttons | done |
| Battery indicator | done |
| Recording indicator | done |
| Left region: brightness scroll + hint | done |
| Right region: volume scroll + hint | done, but a notch after an OSD map is lost (Hyprland frame bug) |
| Sidebar button: geometry, four states, region-wide hover | done |
| Sidebar button: `colText` cross-fade | done |
| Indicators: mute, mic, xkb, network, bluetooth | done |
| Indicators: WireGuard | done |
| Indicators: notifications + unread badge | done |
| Reveal animation, two-speed | done |
| Tray items, pinned/unpinned split, separator | done |
| Tray: overflow button and popup | done |
| Tray: monochrome icons, tooltips, pin entry | done |
| Tray menus, submenus, checkboxes, icons | done |
| Tray menu: fade-in and resize animation | done |
| Weather, GPS location | done |

## 12. The vertical bar — `verticalBar/`

`bar.vertical` puts the bar on the left edge, or on the right with
`bar.bottom` (`src/panels/bar/vertical.rs`, `open_bar` in `main.rs`). It is
`VerticalBar.qml` and `VerticalBarContent.qml`: 46 px wide (51 px of exclusive
zone in Float), the Hug corners on the side away from the edge, the top half
scrolling brightness and the bottom block scrolling volume and opening the
sidebar. The middle holds three groups — resources and media, the workspaces,
the clock and battery — and the bottom the tray and the sidebar button with
its indicators in a column. The clock stacks the hours and minutes at −4 px
spacing (a smaller AM/PM line in 12-hour time) over the short date from
`time.shortDateFormat`. The widgets shared with the horizontal bar take a
vertical mode, like the QML's `vertical` property. The calendar and media
controls open beside the bar.

Differences from the QML: the right bar auto-hides by its own width, not the
horizontal bar's height; popups center on their item; the borderless
separators between groups are drawn; the mute and notification indicators are
centered; the Float shadow follows its setting.
