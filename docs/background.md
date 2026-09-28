# The wallpaper background

Source: `modules/ii/background/Background.qml` and
`modules/ii/background/widgets/`. Read [foundations.md](foundations.md) first.

One full-screen layer-shell window per monitor, on the bottom layer, holding
the wallpaper and a canvas of desktop widgets that parallax with it.

---

## 1. The window

```qml
Variants { model: Quickshell.screens }        // every screen, no filter
PanelWindow {
    screen: modelData
    exclusionMode: ExclusionMode.Ignore
    WlrLayershell.namespace: "quickshell:background"
    WlrLayershell.layer: (screenLocked && !scaleAnim.running) ? WlrLayer.Overlay : WlrLayer.Bottom
    anchors { top: true; bottom: true; left: true; right: true }
}
```

The layer is `Overlay` while the screen is locked, from the moment the zoom
animation finishes. The same window is the lock screen's backdrop.

### 1.1 Hiding under fullscreen windows

```qml
monitorData = HyprlandData.monitors.find(m => m.name === monitor?.name)
fullscreen  = HyprlandData.windowList.some(win =>
                  win.fullscreen === 2 && win.workspace.id === monitorData?.activeWorkspace?.id)
visible = GlobalStates.screenLocked || !fullscreen || !background.hideWhenFullscreen
```

`fullscreen === 2` is Hyprland's *real* fullscreen, not the maximized state
(1). The check is per monitor, by the workspace the monitor is showing.

### 1.2 Window color and the work-safety screen

The window's own `color` is transparent unless the work-safety trigger fires,
in which case it becomes `mix(colLayer0, colPrimary, 0.75)` and the wallpaper
source is cleared — the screen goes to a flat tinted color.

The trigger is `workSafety.enable.wallpaper` **and** the wallpaper path
containing one of `triggerCondition.fileKeywords` **and** the current network
name containing one of `triggerCondition.networkNameKeywords`.

---

## 2. Sizing the wallpaper

A video wallpaper (`.mp4 .webm .mkv .avi .mov`) is represented by
`background.thumbnailPath`; anything else uses `background.wallpaperPath`.

The image's own pixel size is read by shelling out:

```
magick identify -format "%w %h" <path>
```

and then

```
minSuitableScale       = max(screenWidth / imageWidth, screenHeight / imageHeight)
effectiveWallpaperScale = minSuitableScale · parallax.workspaceZoom      // default 1.07
scaledWallpaperWidth   = imageWidth  · effectiveWallpaperScale
scaledWallpaperHeight  = imageHeight · effectiveWallpaperScale
parallaxTotalPixelsX   = max(0, scaledWallpaperWidth  − screenWidth)
parallaxTotalPixelsY   = max(0, scaledWallpaperHeight − screenHeight)
```

`minSuitableScale` is a cover fit: the larger of the two ratios, so neither
axis leaves a gap. `workspaceZoom` overscans by 7 %, and that overscan is the
parallax travel. The zoom is relative to the screen, not to the wallpaper: a
2560 px wide screen gets about 180 px of horizontal travel whatever the size
of the source image.

The image is `fillMode: PreserveAspectCrop` at exactly
`scaledWallpaperWidth × scaledWallpaperHeight`, with `cache: false` and
`smooth: false`.

---

## 3. Parallax

### 3.1 Which axis

```qml
verticalParallax = (parallax.autoVertical && imageHeight > imageWidth)
                 || parallax.vertical
```

Defaults are both false, so parallax is horizontal unless configured
otherwise.

### 3.2 The fraction

```qml
workspaceIndex = (monitor.activeWorkspace.id ?? 1) − 1
fraction = totalWorkspaces <= 1 ? 0.5
         : clamp(workspaceIndex / (totalWorkspaces − 1), 0, 1)
```

`totalWorkspaces` is not the configured count. It is derived from the windows
actually on this monitor:

```qml
relevantWindows = HyprlandData.windowList
      .filter(win => win.monitor == monitor.id && win.workspace.id >= 0)
      .sort((a, b) => a.workspace.id − b.workspace.id)
lastWorkspaceId  = relevantWindows[last]?.workspace.id || 10
workspaceChunkSize = bar.workspaces.shown                     // 10
totalWorkspaces  = ceil(lastWorkspaceId / workspaceChunkSize) · workspaceChunkSize
```

The travel is quantized to whole groups of ten: with windows only up to
workspace 4 the wallpaper spans workspaces 1…10, and it spans 1…20 once a
window exists on workspace 11 or beyond. Negative workspace ids (special
workspaces) are excluded.

`firstWorkspaceId` is computed and never used.

### 3.3 Combining the offsets

```qml
usedFractionX = 0.5
  (if parallax.enableWorkspace && !verticalParallax) → fraction
  (if parallax.enableSidebar)  → += (workspaceZoom / workspaceChunkSize / 2) · sidebarRightOpen
  clamped to 0…1

usedFractionY = 0.5
  (if parallax.enableWorkspace && verticalParallax)  → fraction
  clamped to 0…1
```

The sidebar nudge with the defaults is `1.07 / 10 / 2` = **0.0535** of the
total travel, added while the right sidebar is open. There is no vertical
equivalent.

### 3.4 Position

```qml
x = screenWidth  > width  ? (screenWidth  − width)  / 2 : −parallaxTotalPixelsX · usedFractionX
y = screenHeight > height ? (screenHeight − height) / 2 : −parallaxTotalPixelsY · usedFractionY
```

Both animate with **600 ms `Easing.OutCubic`**, not an `Appearance.animation`
preset.

---

## 4. The blur on lock

```qml
Loader {
    active: lock.blur.enable && (screenLocked || scaleAnim.running)
    anchors.fill: wallpaper
    scale: screenLocked ? lock.blur.extraZoom : 1
    Behavior on scale { 400 ms, expressiveDefaultSpatial }
    sourceComponent: GaussianBlur {
        source: wallpaper
        radius: screenLocked ? lock.blur.radius : 0
        samples: radius · 2 + 1
        Rectangle { anchors.fill: parent
                    opacity: screenLocked ? 1 : 0
                    color: transparentize(colLayer0, 0.7) }
    }
}
```

The wallpaper itself hides (`visible: opacity > 0 && !blurLoader.active`)
while the blur copy is up. The loader stays active through the un-zoom:
`scaleAnim.running` in the `active` binding, and in the layer binding in §1.

---

## 5. The widget canvas

`WidgetCanvas` fills the screen and carries the desktop clock and weather
widgets, each behind a `FadeLoader` keyed on
`background.widgets.clock.enable` / `…weather.enable`.

It parallaxes with the wallpaper, faster:

```qml
parallaxFactor = parallax.widgetsFactor / parallax.workspaceZoom     // 1.2 / 1.07 ≈ 1.12
baseWallpaperOffsetX = (screenWidth − wallpaper.width) / 2
wallpaperTotalOffsetX = wallpaper.x − baseWallpaperOffsetX
x = wallpaperTotalOffsetX · parallaxFactor · !locked
```

The widgets move about 12 % further than the wallpaper, and snap to zero
offset while the screen is locked. The `x` follows the wallpaper's animated
`x`, so it moves with the same 600 ms `OutCubic` and has no animation of its
own.

Size and anchor changes on the canvas animate with `elementMove` (500 ms).

The clock widget takes `wallpaperSafetyTriggered` and recolors itself over
the flat safety background.

### 5.1 Text color over the wallpaper

```qml
dominantColor       = (sampled; defaults to colPrimary)
dominantColorIsDark = dominantColor.hslLightness < 0.5
colText = safetyTriggered ? mix(colOnLayer0, colPrimary, 0.75)
        : (screenLocked && shouldBlur) ? colOnLayer0
        : colorWithLightness(colPrimary, dominantColorIsDark ? 0.8 : 0.12)
```

animated with `elementMoveFast` (200 ms). Over a dark wallpaper the widget
text is the primary hue at 80 % lightness, over a light one the same hue at
12 %.

---

## 6. Status (proscenio)

`src/panels/background/mod.rs` implements: one window per monitor on `Layer::Bottom`,
anchored to all four edges, `exclusive_zone(-1)`, an empty input region, the
cover-fit plus `workspaceZoom` sizing, the workspace fraction with the
`ceil(last / chunk) · chunk` quantization, the vertical/horizontal axis
choice including `autoVertical`, the sidebar nudge (`parallax.enableSidebar`,
`workspaceZoom / chunk / 2` of the travel while any right sidebar is open),
the 600 ms `OutCubic` slide, and hiding under a real fullscreen window on the
active workspace. A video wallpaper leaves the surface transparent so
`mpvpaper` shows through, as the QML does. A change of
`background.wallpaperPath` in `config.toml` reloads the wallpaper.

A new wallpaper, at start or after a change, appears at a random point of its
parallax travel and glides to its place over a second (`OutCubic`); qs places
it there directly. The glide also frees memory: GTK's GPU renderer releases a
texture upload's staging buffer only when the frame that did the upload is
reused for a real redraw. A still wallpaper never redraws and holds tens of
megabytes of driver memory per monitor (mapped from `/dev/nvidiactl` on
NVIDIA). A `queue_draw` alone does not release it: an unchanged picture diffs
to an empty region and GTK skips the frame.

The window is exactly the monitor's size and draws the wallpaper itself, as
one texture offset into it. A picture laid out as a child widget would size
the surface by the picture, up to 8 % larger than the screen, and resize it on
every frame of the slide. The image size is read with `Pixbuf::file_info`, not
`magick`, and the image is decoded off the main thread straight to the target
size.

**The lock blur** is a second copy of the wallpaper, blurred once off the main
thread, not a GSK blur node. The copy is decoded at up to 1/8 of the wallpaper's
size (the shrink is `sigma / 3`, where `sigma` is half the GSK radius of
`lock.blur.radius`), blurred with three box passes whose sum has the Gaussian's
variance, and drawn stretched over the wallpaper while locked. It is rebuilt when
the wallpaper or `lock.blur` changes. A blur node is recomputed on every frame of
the zoom, and on the `cairo` renderer that runs on the CPU in the main thread.

**`last`** is the higher of the active workspace and the highest workspace
holding a window, so the first window opened on an empty monitor leaves the
wallpaper where it is.

**The clock** is an overlay on that window (`src/panels/background/clock.rs`
for the widget, `cookie.rs` for the dial). It has both styles and every
cookie option: the dials, hands and dates of `CookieClock.qml`, the sine cookie,
the 30 s turn, and the digital column with its font axes. It has the quote,
the "Locked" and safety badge, the random or free placement with dragging
saved to `config.toml`, and the `widgetsFactor` over-parallax. The window takes
input only over the clock, and only while it can be dragged. Locking centers
the clock over 500 ms (`elementMove`). Its text follows the `colText` rule
without the safety tint, as `AbstractBackgroundWidget` does. The dial's text
uses Qt's line height (`ceil` of the 1/64 px ascent plus descent), which puts
the center digits on the same pixels as qs.

The weather widget (`src/panels/background/weather.rs`, `WeatherWidget.qml`) is
a 200 px `colPrimaryContainer` pill with a shadow, the temperature without its
unit letter at 80 px in `colPrimary` top right and the weather symbol at 80 px
bottom left, both 16 px in from the side and 20 px from the edge. It shares the
clock's placement: free (400, 100 by default) or random, dragging saved to the
config, a 1.05 scale while held, and it fades out on the lock screen. The
window's input region covers whichever of the two can be dragged.

Missing:

- the work-safety screen
