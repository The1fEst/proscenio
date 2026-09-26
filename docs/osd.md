# The on-screen display

Source: `modules/ii/onScreenDisplay/`, five files. Read
[foundations.md](foundations.md) first.

One overlay surface that appears under the bar whenever a value changes, shows
it for a second and goes away.

## 1. The window

A `Loader` on `GlobalStates.osdVolumeOpen` — one flag for all three
indicators, despite the name. Inside, a `PanelWindow`:

```qml
WlrLayershell.namespace: "quickshell:onScreenDisplay"
WlrLayershell.layer: WlrLayer.Overlay
anchors { top: !bar.bottom; bottom: bar.bottom }
margins { top: Appearance.sizes.barHeight; bottom: Appearance.sizes.barHeight }
exclusionMode: ExclusionMode.Ignore
exclusiveZone: 0
mask: Region { item: osdValuesWrapper }
```

Its `screen` follows `Hyprland.focusedMonitor`, and the content is centered
horizontally. **Hovering it closes it** — `onEntered: osdVolumeOpen = false`.

`triggerOsd()` opens it and restarts a timer of `osd.timeout`, **1000 ms**.

## 2. What triggers it

| Source | Indicator |
| --- | --- |
| `Brightness.onBrightnessChanged` | `brightness` |
| `Hyprsunset.onGammaChangeAttempt` | `gamma` |
| the sink's `onVolumeChanged` / `onMutedChanged`, once `Audio.ready` | `volume` |
| `Audio.onSinkProtectionTriggered(reason)` | `volume`, plus the message |

An `IpcHandler` on `osdVolume` with `trigger` / `hide` / `toggle`, and the
global shortcuts `osdVolumeTrigger` and `osdVolumeHide`.

## 3. `OsdValueIndicator`

```
implicitWidth  = Appearance.sizes.osdWidth + 2 · elevationMargin   // 180 + 20
radius         = full,  color colLayer0,  with a shadow
padding        = left 10, right 20, vertical 9
```

A row of spacing 10: a 30×30 icon slot, then a column of spacing 5 holding the
name and the rounded value side by side — both `pixelSize.small` (15) in
`colOnLayer0`, inset by half the progress bar's height so the text lines up
with the bar's end curve — and the progress bar under them.

The icon is `iconSize: 20 + 10 · (scaleIcon ? value : 1)` and
`rotation: 180 · (rotateIcon ? value : 0)`, both eased with
`elementMoveEnter`. Only the brightness indicator sets `scaleIcon` and
`rotateIcon`.

| Indicator | Icon | Name | Value | From |
| --- | --- | --- | --- | --- |
| volume | `volume_off` when muted else `volume_up` | Volume | the sink volume | 0 |
| brightness | `routine` when the night light is on else `light_mode` | Brightness | the monitor's backlight | 0 |
| gamma | `wb_twilight` | Gamma | `Hyprsunset.gamma / 100` | `gammaLowerLimit / 100` |

## 4. `StyledProgressBar`

Height 4, width 120, gap 4. The filled part runs from the left to
`width · visualPosition`, radius half its height, `colPrimary`. The remainder
is drawn from the **right** edge with width `(1 − pos)·width − gap` in
`m3secondaryContainer`, and a 4×4 dot sits at the very right end in
`colPrimary` — so the track always ends in a stop point separated from the
fill by the gap.

## 5. The protection message

Under the indicator, an error-colored rounded box with a `dangerous` symbol
and the reason, shown when `Audio.sinkProtectionTriggered` fires and cleared
by the next trigger or the timeout.

---

**Status (proscenio).** Done. `src/panels/osd.rs` follows the QML
file for file. It keeps one `Osd` for all monitors, like the `Scope`, and
builds the window when the OSD opens and destroys it when it closes, like
the `Loader`. So each opening starts at the current value, and only changes
while it is open are eased. Switching indicators while open swaps the
indicator the same way the `Loader`'s `source` does. The window goes to the
focused monitor when it opens and follows focus while open. The global
shortcuts `osdVolumeTrigger` and `osdVolumeHide` are registered, and so is
the IPC target `osdVolume` with `trigger`, `hide` and `toggle`.

The icon is drawn as text, not set on a label, and follows Qt's placement:
the `Text` box sits centered on the 30 px slot at a fractional x and rotates
about the slot's center. The box is rounded to the pixel only when the icon
does not rotate (`alignWhenCentered: !rotateIcon`); the rotated brightness
icon differs from qs's only in antialiasing. Size and rotation both ease with
`elementMoveEnter`. The progress bar is the shared `ProgressBar`,
filled to the 110 px the row leaves it, as `Layout.fillWidth` does.

`src/services/audio.rs` has `on_sink_change` and the volume guard from
`Audio.qml`, with the `audio.protection` options. `on_sink_change` fires only
on sink volume or mute changes after the first read, like `Audio.ready`. The
guard reverts an increase larger than `maxAllowedIncrease` ("Illegal
increment") and caps the volume at `maxAllowed` ("Exceeded max allowed").

The empty protection box takes no room, so the space under the pill does not
close the OSD. The first opening lands on the focused monitor, as it does in
qs, where Hyprland places a layer surface without an output there.
