# Foundations

`modules/common` holds the values and the widgets that the rest of the shell is
built from. The numbers in [bar.md](bar.md) come from here.

## 1. Appearance

`modules/common/Appearance.qml` is a singleton with six groups: `m3colors`,
`colors`, `rounding`, `font`, `sizes`, `animation`/`animationCurves`.

### 1.1 Where the palette comes from

`m3colors` carries a full Material 3 scheme — 60-odd named colors plus 16
terminal colors. The values written in the file are only the dark-mode
fallback; at runtime `MaterialThemeLoader` overwrites them from the generated
palette. The same palette is written to
`~/.local/state/quickshell/user/generated/colors.json`; `proscenio` reads its
own copy from `~/.local/state/proscenio/generated` ([files.md](files.md)).

`darkmode` is a property of `m3colors`, not of the palette file. The shell
flips it together with the colors. `proscenio` reads it from
`material_colors.scss` in the same state directory and watches that file.

### 1.2 Transparency

Two factors modulate every layer color:

```
backgroundTransparency = transparency.enable
    ? (transparency.automatic ? autoBackgroundTransparency : configured)
    : 0
contentTransparency = transparency.automatic ? 0.9 : configured
```

`autoBackgroundTransparency` is derived from the wallpaper: a `ColorQuantizer`
reduces the wallpaper to one color, and

```
vibrancy = (hslSaturation + hslLightness) / 2
y        = 0.5768·vibrancy² − 0.759·vibrancy + 0.2896
result   = clamp(y, 0, 0.22) − (darkmode ? 0 : 0.12)
```

With the shipped default `transparency.enable` is `false`, so
`backgroundTransparency` is 0 and `contentTransparency` is 0.9. The formulas
below keep both factors named; with other settings the layer colors are
translucent.

### 1.3 Color algebra

Three helpers from `modules/common/functions/ColorUtils.qml`:

```
mix(a, b, p)      = p·a + (1−p)·b          componentwise, alpha included
transparentize(c, p) = rgba(c.rgb, c.a·(1−p))
solveOverlayColor(base, target, alpha)
                  = rgba(clamp01((target − base·(1−alpha)) / alpha), alpha)
```

`mix` weights the **first** argument by `p`:
`mix(colLayer1, colOnLayer1, 0.92)` is 92 % layer, 8 % ink, a hover tint.

`solveOverlayColor` returns the color that, drawn at `alpha` over `base`,
looks like `target`. Layers 1–4 use it so that a translucent pane over the
layer below shows its scheme color.

### 1.4 The layer colors

```
colSubtext      = m3outline
colLayer0Base   = mix(m3background, m3primary, extraBackgroundTint ? 0.99 : 1)
colLayer0       = transparentize(colLayer0Base, backgroundTransparency)
colOnLayer0     = m3onBackground
colLayer0Hover  = transparentize(mix(colLayer0, colOnLayer0, 0.9), contentTransparency)
colLayer0Active = transparentize(mix(colLayer0, colOnLayer0, 0.8), contentTransparency)
colLayer0Border = mix(m3outlineVariant, colLayer0, 0.4)

colLayer1Base   = m3surfaceContainerLow
colLayer1       = solveOverlayColor(colLayer0Base, colLayer1Base, 1 − contentTransparency)
colOnLayer1     = m3onSurfaceVariant
colOnLayer1Inactive = mix(colOnLayer1, colLayer1, 0.45)
colLayer1Hover  = transparentize(mix(colLayer1, colOnLayer1, 0.92), contentTransparency)
colLayer1Active = transparentize(mix(colLayer1, colOnLayer1, 0.85), contentTransparency)

colLayer2Base   = m3surfaceContainer
colLayer2       = solveOverlayColor(colLayer1Base, colLayer2Base, 1 − contentTransparency)
colLayer2Hover  = solveOverlayColor(colLayer1Base, mix(colLayer2Base, colOnLayer2, 0.90), …)
colLayer2Active = solveOverlayColor(colLayer1Base, mix(colLayer2Base, colOnLayer2, 0.80), …)
colOnLayer2     = m3onSurface

colLayer3Base   = m3surfaceContainerHigh      (same shape as layer 2)
colLayer4Base   = m3surfaceContainerHighest   (same shape again)
```

**Layer 1 hovers at 0.92/0.85; layers 0 and 2–4 at 0.90/0.80.** The bar's own
hover states use the layer-1 numbers.

Accent colors used by the bar:

```
colPrimary                 = m3primary
colOnPrimary               = m3onPrimary
colSecondaryContainer      = m3secondaryContainer
colSecondaryContainerHover = mix(m3secondaryContainer, m3onSecondaryContainer, 0.90)
colSecondaryContainerActive= mix(m3secondaryContainer, m3onSecondaryContainer, 0.54)
colOnSecondaryContainer    = m3onSecondaryContainer
colError                   = m3error
colErrorHover              = mix(m3error, colLayer1Hover, 0.85)
colErrorActive             = mix(m3error, colLayer1Active, 0.7)
colOutlineVariant          = m3outlineVariant
colTooltip                 = m3inverseSurface
colOnTooltip               = m3inverseOnSurface
```

`colSecondaryContainerActive` mixes at **0.54**. It is the ripple color of a
toggled button.

### 1.5 Rounding

```
unsharpen 2   unsharpenmore 6   verysmall 8   small 12   normal 17
large 23      verylarge 30      full 9999
screenRounding = large = 23      windowRounding = 18
```

`screenRounding` is only the fallback: the bar reads Hyprland's own
`decoration:rounding` first, via
`HyprlandOptions.numberOr("decoration:rounding", Appearance.rounding.screenRounding)`.

### 1.6 Fonts

Families come from the config: `main` and `title` default to "Google Sans",
`iconNerd`/`monospace` to "JetBrains Mono NF", `reading` to "Readex Pro",
`expressive` to "Space Grotesk". The icon font is hard-coded:
**"Material Symbols Rounded"**.

Pixel sizes:

```
smallest 10   smaller 12   smallie 13   small 15   normal 16
large 17      larger 19    huge 22      hugeass 23   title = huge
```

Variable axes: body text is `wght 450`, titles `wght 550`.

### 1.7 Sizes

```
baseBarHeight 40
barHeight     = cornerStyle == 1 ? baseBarHeight + hyprlandGapsOut·2 : baseBarHeight
barCenterSideModuleWidth          = verbose ? 360 : 140
barCenterSideModuleWidthShortened = 280
barCenterSideModuleWidthHellaShortened = 190
barShortenScreenWidthThreshold      = 1200
barHellaShortenScreenWidthThreshold = 1000
elevationMargin 10        hyprlandGapsOut 5
sidebarWidth 460          sidebarWidthExtended 750
osdWidth 180              notificationPopupWidth 410
mediaControlsWidth 440    mediaControlsHeight 160
baseVerticalBarWidth 46
```

`hyprlandGapsOut` is a constant 5 here, not read back from Hyprland.

## 2. Motion

`animationCurves` holds the curves, `animation` pairs each with a duration and
hands out a `Component` that instantiates the animation.

| Name | Duration | Curve | Control points |
| --- | --- | --- | --- |
| `elementMove` | 500 ms | expressiveDefaultSpatial | 0.38, 1.21, 0.22, 1.00 |
| `elementMoveSmall` | 350 ms | expressiveFastSpatial | 0.42, 1.67, 0.21, 0.90 |
| `elementMoveFast` | 200 ms | expressiveEffects | 0.34, 0.80, 0.34, 1.00 |
| `elementMoveEnter` | 400 ms | emphasizedDecel | 0.05, 0.7, 0.1, 1 |
| `elementMoveExit` | 200 ms | emphasizedAccel | 0.3, 0, 0.8, 0.15 |
| `elementResize` | 300 ms | emphasized | multi-segment, below |
| `clickBounce` | 400 ms | expressiveDefaultSpatial | 0.38, 1.21, 0.22, 1.00 |
| `scroll` | 200 ms | standardDecel | 0, 0, 0, 1 |
| `menuDecel` | 350 ms | `Easing.OutExpo` | — |

Two of these overshoot: the y control points 1.67 and 1.21 are above 1, so
`elementMoveSmall` and `elementMove` go past the target and come back.

`emphasized` is a three-segment spline, not one cubic:

```
0.05, 0,  2/15, 0.06,  1/6, 0.4,   5/24, 0.82,  0.25, 1,  1, 1
```

In Qt's `bezierCurve` encoding each segment contributes two control points and
an endpoint, so this reads as: segment 1 to (1/6, 0.4), segment 2 to (0.25, 1),
segment 3 to (1, 1).

`elementMoveEnter`, `elementMoveExit`, `elementMoveFast`, `elementResize` and
`clickBounce` set `alwaysRunToEnd: true` — retargeting mid-flight does not cut
the animation short, it finishes and then the new one starts. `elementMove` and
`elementMoveSmall` do not, so they retarget from wherever they are.

## 3. Interaction states

`widgets/StateLayer.qml` is the Material 3 state-layer opacity table:

```
Hover 0.08    Focus 0.1    Press 0.1    Drag 0.16
```

`widgets/StateOverlay.qml` stacks four of them, one per state, each behind a
`FadeLoader`. They **add**: an element that is hovered and pressed shows
0.08 + 0.1 of its content color, and one that also forces `drag` shows 0.24.
The corner radii of all four follow the overlay's own
`topLeftRadius`/`topRightRadius`/`bottomLeftRadius`/`bottomRightRadius`.

The focus layer's `shown: root.focus` reads the `Item.focus` property of the
overlay itself, which is false for a decorative overlay, so the focus layer
never shows.

## 4. The reusable widgets

### Revealer (`widgets/Revealer.qml`)

A GTK revealer in Qt. One child; `clip: true`. When `reveal` is false the
implicit size along the revealing axis collapses to 0, and the size animates
with **`elementMoveEnter`** (400 ms, emphasizedDecel) in both directions.
`visible` stays true while the size is non-zero, so the child slides out.

`vertical` chooses the axis: horizontal is the default, and then the height
tracks `childrenRect.height` unconditionally while the width collapses.

### FadeLoader (`widgets/FadeLoader.qml`)

A `Loader` whose `opacity` follows `shown` through **`elementMoveFast`**
(200 ms) and whose `active`/`visible` follow `opacity > 0`: the content is
destroyed when fully faded out and created on the way in.

### RippleButton (`widgets/RippleButton.qml`)

The shell's standard button.

*Color.* The background is

```
buttonColor = transparentize(
    toggled ? (hovered ? colBackgroundToggledHover : colBackgroundToggled)
            : (hovered ? colBackgroundHover        : colBackground),
    enabled ? 0 : 1)
```

animated with `elementMoveFast.colorAnimation` (200 ms). Defaults:
`colBackground` fully transparent `colLayer1Hover`, `colBackgroundHover`
`colLayer1Hover`, `colBackgroundToggled` `colPrimary`,
`colBackgroundToggledHover` `colPrimaryHover`. `opacity` drops to 0.4 when
disabled.

*Ripple.* On press a radial gradient starts at the cursor and grows to
`radius·2`, where the radius is the distance from the press point to the
furthest corner of the background. `rippleDuration` is 1200 ms with the
`standardDecel` curve; the fade-out on release runs at double that, 2400 ms.
The gradient is `rippleColor` solid to stop 0.3, transparent by stop 0.5 —
a soft-edged disc, not a hard one. `rippleColor` is `colRippleToggled` when
toggled, else `colRipple` (defaults `colPrimaryActive` / `colLayer1Active`).

*Buttons.* The internal `MouseArea` accepts left, right and middle. Right
calls `altAction(event)` and returns without pressing; middle calls
`middleClickAction()` and returns. Left sets `down`, calls `downAction()`,
starts the ripple, and on release calls `releaseAction()` and then `click()`,
which re-emits the click the `MouseArea` consumed. The cursor is a pointing
hand unless
`pointingHandCursor` is cleared.

### Pill, Circle, Box

`Pill` is `Rectangle { radius: min(width, height) / 2 }`.
`Circle` is a `Rectangle` sized by a `diameter` property, radius half of it.
`Box` is a `Grid` with one row (or one column when `vertical`), `spacing`
aliased onto both axes — a GTK box, which Qt lacks. `BoxLayout` is the same
thing over `GridLayout`.

### StyledRectangle (`widgets/StyledRectangle.qml`)

Its `switch` over the `contentLayer` enum has no `break` and no `return`, so
every branch falls through to the default: the color is `colLayer1` unless
the caller sets `color`, which every caller in the bar does. `contentLayer`
has no effect.

### RoundCorner (`widgets/RoundCorner.qml`)

The concave corner used where the bar meets the screen edge. A `Shape` with
one `PathAngleArc` of 90°, centered at the corner **opposite** the one being
drawn, with radius `implicitSize`, closed back to the start point. So it fills
the square minus a quarter disc, and the disc's center is at the inner corner.
Rendered with `Shape.CurveRenderer` and `layer.smooth`.

### MaterialSymbol (`widgets/MaterialSymbol.qml`)

`StyledText` in "Material Symbols Rounded" with

```
pixelSize = iconSize
weight    = Font.Normal + (Font.DemiBold − Font.Normal)·fill
variableAxes = { FILL: fill, opsz: iconSize }
renderType = NativeRendering, hintingPreference = PreferNoHinting
```

`fill` is animated (`elementMoveFast`, 200 ms) but the value fed to the font is
`fill.toFixed(1)`, rounded to one decimal, so the animation passes the font
at most 11 distinct `FILL` values.

The `opsz` axis of the real font clamps at a minimum of 20: setting `opsz 19`
for `pixelSize.larger` gets clamped by the font engine, it does not fail.

### StyledText (`widgets/StyledText.qml`)

`Text` with family `Appearance.font.family.main`, `pixelSize.small` (15),
`variableAxes { wght: 450 }`, `verticalAlignment: AlignVCenter`,
`renderType: NativeRendering`, color `m3onBackground`.

With `animateChange: true` a text change runs a slide-and-fade: 150 ms out
(up by `animationDistanceY` = 6 px, opacity to 0, `InSine`), the text swaps at
the turn, then 150 ms in from 6 px below with `OutSine`. Used by the keyboard
layout indicator.

### AnimatedTabIndexPair (`models/AnimatedTabIndexPair.qml`)

Two numbers that both chase the same `index` at different speeds: `idx1` over
100 ms, `idx2` over 300 ms, both `Easing.OutSine`. Drawing a shape from
`min(idx1, idx2)` to `max(idx1, idx2)` gives the stretch-and-catch-up
indicator the workspaces use. The leading edge arrives in 100 ms, the trailing
edge 200 ms later.

### ClippedFilledCircularProgress (`widgets/ClippedFilledCircularProgress.qml`)

The small ring behind the media and resource icons. A filled circle of
`colSecondary` (which defaults to `transparentize(colPrimary, 0.5)`), with a
pie wedge of `colPrimary` swept from −90° by `value·360`, and then the whole
thing is drawn through an **inverted** `OpacityMask` whose mask is the child
item. The child is therefore punched out of the ring rather than drawn on top
— the icon is a hole.

`arcRadius = implicitSize/2 − lineWidth/2 − (accountForLightBleeding ? 0.5 : 0)`.
`lineWidth` is set to `rounding.unsharpen` = 2 by both callers.
`enableAnimation: false` at both call sites, so the 800 ms `OutCubic` ease on
`degree` is off in the bar.

### FocusedScrollMouseArea (`widgets/FocusedScrollMouseArea.qml`)

A `MouseArea` with `hoverEnabled`, its own `hovered` flag, and scroll
bookkeeping: `onWheel` emits `scrollUp`/`scrollDown` and records the pointer
position; once the pointer then moves more than 20 px from that spot, or
leaves, it emits `movedAway()` once. The bar uses that to close the volume and
brightness OSDs.

### PopupToolTip (`widgets/PopupToolTip.qml`)

A tooltip in a real `PopupWindow` anchored to the parent item, shown when
`extraVisibleCondition && parent.hovered`, or when
`alternativeVisibleCondition`. The content is `StyledToolTipContent`: a
`colTooltip` rectangle, radius `verysmall` (8), padding 10×5, text at
`pixelSize.smaller` (12) in `colOnTooltip`, no hinting. It animates open by
growing from zero width and height with `elementMoveFast` while fading in.

### MaterialShape morphing (`widgets/shapes/ShapeCanvas.qml`)

A `MaterialShape` that changes shape does not jump: `ShapeCanvas` morphs
from the old polygon to the new one, a port of androidx graphics-shapes'
`Morph`. Both outlines are measured, their corners matched by nearest
position (convex to convex), the second outline cut and shifted to line up
with the first, and the cubics paired and interpolated. The default animation
is 350 ms expressive fast spatial; a caller can swap it.

In proscenio `src/ui/morph.rs` ports it, with `src/ui/shapes.rs` keeping each
feature's kind (edge, convex or concave corner). proscenio follows the Kotlin
original where the JS port differs: `measurePolygon` picks a corner's middle
cubic with `length / 2`, a fraction for the common three-cubic corner, so it
matches almost no corners and falls back to the identity mapping; and
`addMapping` inserts every match at the front rather than in order.

### MaterialLoadingIndicator (`widgets/MaterialLoadingIndicator.qml`)

A `colPrimaryContainer` circle (48 px by default) holding a
`colOnPrimaryContainer` shape 70 % of its size. While loading, the whole thing
turns once every 12 s, and every 800 ms it leaps: the shape morphs to the next
of SoftBurst, Cookie9Sided, Pentagon, Pill, Sunny, Cookie4Sided and Oval over
200 ms (expressive effects), turns a further 90° over 350 ms (`InOutQuad`),
and swells to 120 % and back over 750 ms (standard). proscenio has it as
`src/ui/widgets/loading.rs`.

## 5. Focus grabbing

`services/GlobalFocusGrab.qml` owns one shared `HyprlandFocusGrab` for the
whole shell. Windows register as either:

- **persistent** — always inside the grab, never closed by it: the bar and the
  on-screen keyboard. Clicking the bar while the sidebar is open does not
  dismiss the sidebar, and the bar keeps receiving pointer events.
- **dismissible** — the sidebars and panels. The grab is active while at least
  one exists, and `onCleared` empties the list and emits `dismissed()`.

The `windows` binding includes the persistent windows only when none of the
dismissible ones is focusable, or when one of them holds focus. While a
dismissible window wants keyboard input but lacks it, the grab covers the
dismissible windows alone.

`proscenio` implements the same protocol directly in `src/platform/grab.rs`
(`hyprland_focus_grab_v1`), with the sidebar as the only grabbed surface.
There are no persistent members: a click on the bar or a corner closes an
open panel.

## 6. Unloading hidden surfaces

qs unloads some hidden panels with `Loader`s: the cheatsheet, the media
controls, the session screen, the settings window and the welcome window. The
sidebar and calendar stay loaded under `keepRightSidebarLoaded`, and the dock
always does. `proscenio` releases every hidden surface.

`src/ui/unload.rs` has `when_hidden(widget)`. A second after the window or
popover hides, if it is still hidden, it is unrealized and `malloc_trim(0)`
runs. The trim runs once more 1.5 s later, for memory freed after that
moment. Unrealizing drops the surface's renderer, buffers and caches, and the
trim hands the freed heap back to the system. The next show realizes it
again.

Between hides, the `trim` task of the background loop (section 7) checks the
resident size on every tick and trims whenever it has grown more than 1 MB
since the last trim. glibc keeps memory freed in the middle of its heap until
a trim; an idle bar's redraws alone leave about 50 MB there per hour. With
the check, what a trim could still return stays under 4 MB.

The second of delay is for GTK's tooltips. A pending tooltip popup, up to
500 ms, looks up the surface of the window the pointer was last over. Only a
leave event cancels it, and a window that hides under a still pointer gets
none. Unrealizing sooner hands that lookup a missing surface, and GTK prints
`gdk_surface_get_device_position` and `_gtk_widget_find_at_coords`
criticals.

It is attached to every per-monitor window in `src/main.rs`, to the sidebar
and to the dock preview popover. The widget trees stay; only the rendering
side goes.

Memory kept after one open and close, with llvmpipe, where GPU memory counts
as process memory:

| Surface | Kept while realized | Kept after unrealizing |
| --- | --- | --- |
| calendar | +30 MB | +6 MB |
| session screen | +18 MB | about 0 |
| overview | +16 MB | about 0 |
| cheatsheet | +66 MB | +21 MB, its large widget tree |

Some of what stays is one-time cost shared by every surface: icon-theme and
font caches, and compiled shaders.

## 7. The background loop

All periodic work runs from one loop, `src/services/background.rs`. It ticks
once at start and then every `resources.updateInterval` (3000 ms), read anew
on each tick, so a change in the settings takes effect on the next one. A task
is a name and a function returning `Result`; an `Err` is logged as
`background: <name> failed: …` and the loop carries on. A panic ends the
process: the release profile aborts on panic.

A task may carry a period. The loop then runs it on the first tick at least
that long after its last run, so weather (`fetchInterval`, 10 min) and
`checkupdates` (`checkInterval`, 120 min) share the loop without running
every 3 s. Their periods read the config each tick too, and are zero while
the feature is off, so switching it on fetches on the next tick.

`add_scoped(…)` returns a `Subscription` instead of keeping the task for the
whole run. Dropping it removes the task, together with everything the task
holds; a monitor's panels keep theirs in the monitor's `Scope` (section 8).

| Task | Does |
| --- | --- |
| `resources` | samples `/proc/meminfo` and `/proc/stat` once for every bar |
| `media position` | re-reads the players while one plays |
| `recording` | looks for a `wf-recorder` in `/proc/*/comm`, without spawning anything, while no recording is known |
| `night light schedule` | re-evaluates the schedule when the minute changes |
| `warp` | reads `warp-cli status` once WARP is found |
| `weather`, `updates` | with their periods, as above |
| `trim` | section 6 |
| `clock details`, `uptime` | per monitor, through `add_scoped` |

Outside the loop stay the timers that only run while something is under way
(the stopwatch and pomodoro, the recording's seconds, the capture pump) and
one-shot delays. The clock's own tick stays aligned to the minute.

## 8. Monitors come and go

Each monitor gets its own bar, sidebar, calendar, media controls, session
screen, cheatsheet, overview, notification popup, dock, background and
corners. When the monitor is unplugged all of them have to go, or every
reconnect leaves a full set behind, still subscribed to the services.

Every service hands out a `Subscription` (`src/core/listeners.rs`) for each
listener; dropping it takes the listener off the list. What a monitor's panels
subscribe to, the config files they watch, the background tasks they run and
the teardown they need all go into one `Scope` (`src/core/scope.rs`) that
lives as long as the monitor does. Panels that live for the whole run (OSD,
polkit, region selector, the launcher's clipboard feed) mark theirs with
`forever()`, and a lock surface keeps its own for as long as it is locked.

`gtk_window_destroy` only hides a window and lets go of GTK's own reference.
The window is disposed, and its signal handlers and controllers released,
only once nothing else holds it, yet the handlers inside its tree routinely
hold the window or one of its widgets. `unload::discard` therefore disposes
the window, which lets go of its child, and then disposes each widget of the
old tree that is still alive once it has no parent: a widget kept only by a
closure of its own subtree. A widget still in a parent is left to that
parent's dispose, as GTK expects. A hotplug cycle leaves the layer count
unchanged.

Disposing drops signal handlers, but not event controllers, which GTK keeps
until the widget is finalized, nor closures a widget of ours keeps in its own
fields. A closure stored in either place must therefore not hold the widget,
one of its ancestors, or an `Rc` that owns them: it takes a weak reference or
asks the gesture for its widget. The controllers cannot be removed from
outside either, since GTK widgets remove their own internal ones later.

## 9. Fullscreen windows

qs goes on drawing its bar, dock and corners under a fullscreen window, and a
top-layer surface mapped after the window went fullscreen, as on a shell
restart, even shows above it. proscenio draws none of its permanent surfaces
on a monitor whose active workspace holds a real fullscreen window
(`fullscreen == 2`). One service, `src/services/fullscreen.rs`, keeps the set
of covered monitors from the Hyprland event stream, reading `hyprctl monitors`
and `clients` once per burst of events. The bar, the dock with its trigger and
the corners hide there; so does the background unless
`background.hideWhenFullscreen` is off. Notification popups, on the Overlay
layer, and panels opened on demand (the sidebar, the overview, the OSD and so
on) show over the window.

Hiding the bar would drop its exclusive zone, so the tiled windows of the next
workspace would lay out without it and then jump when the bar comes back. While
the bar, or a pinned dock, is hidden this way, a 1×1 layer surface
(`src/ui/reserve.rs`) on the same edge holds the zone. Its one pixel has an
alpha of 1/255, since a surface that paints nothing never maps.
