# Media controls

Source: `modules/ii/mediaControls/`, two files, over
`services/MprisController.qml`.

A panel of one card per player, under the bar and left of center.

## 1. The window

```qml
WlrLayershell.namespace: "quickshell:mediaControls"
exclusionMode: ExclusionMode.Ignore
exclusiveZone: 0
implicitWidth: Appearance.sizes.mediaControlsWidth      // 440
anchors { top: !bar.bottom; left: true }
margins {
    top:  Appearance.sizes.barHeight
    left: screen.width / 2 − osdWidth / 2 − widgetWidth
}
mask: Region { item: playerColumnLayout }
```

It sits just left of the OSD's position. It is a **dismissible** member of
the shared focus grab. The column's spacing is `−elevationMargin`, so
neighboring cards' shadows overlap.

An `IpcHandler` on `mediaControls` — whose `open` and `toggle` also call
`Notifications.timeoutAll()` — and three global shortcuts.

## 2. Which players are shown

`filterDuplicatePlayers` groups players whose titles contain one another
**and** whose position and length are both within 2, then keeps the one with
a non-empty `trackArtUrl`, falling back to the first of the group.

## 3. A card — `PlayerControl.qml`

440×160 with a 10 px elevation margin, radius
`screenRounding − hyprlandGapsOut + 1` (19).

The cover art is downloaded to `Directories.coverArt` under the md5 of its
URL by `[ -f … ] || curl -4 -sSL …`, then quantized to a single color.
That color, mixed 0.8 toward `colPrimaryContainer`, seeds an
`AdaptedMaterialScheme` — **the whole card is themed from the album art**,
not from the global palette.

Behind everything: the art again, cropped to fill and blurred, under a
`colLayer0` wash at 0.3, and over that a `WaveVisualizer` driven by `cava`
running with the shell's `raw_output_config.txt` — a single process for the
panel, parsed from `;`-separated numbers.

In front, a row inset 13 with spacing 15:

- the art thumbnail, square, radius `verysmall` (8);
- a column of spacing 2: the title at `large` (17), the artist at `smaller`
  (12) in the scheme's subtext, both eliding and animating a 6 px slide on
  change, a spacer, then the time and the controls.

The time reads `position / length` when there is a length, else just the
position. The play button is 44 px, radius `normal` (17) while playing and
22 when not, `colPrimary` against `colSecondaryContainer`. Around the seek
bar sit two 24 px `skip_previous` / `skip_next` buttons.

The seek bar is a `StyledSlider` in the **Wavy** configuration when the
player can seek and has a length, writing `position` back on release; when it
cannot, a `StyledProgressBar` that is wavy while playing.

## 4. The placeholder

When no player survives the filter, a `colLayer0` box of the same radius,
padded 20: "No active player" at `large`, and under it "Make sure your player
has MPRIS support / or try turning off duplicate player filtering" at
`small` in `colSubtext`.

---

**Status (proscenio).** `src/services/mpris.rs` is the service. It does the following:

- lists `org.mpris.MediaPlayer2.*` and reads each player's `Identity`,
  `DesktopEntry`, track id, title, artist, art, position, length, `CanSeek`
  and `PlaybackStatus`;
- follows `PropertiesChanged` and `NameOwnerChanged`, and polls only while
  something is playing;
- carries `meaningful()`, the duplicate filter, with the 2-second window in
  microseconds;
- seeks with `SetPosition(trackid, …)`, or with a relative `Seek` when there
  is no track id, the way Quickshell does.

`src/panels/mediacontrols.rs` follows the two QML files. It keeps one card per player
across updates and has:

- the art downloaded into the same `coverart` cache and averaged to one color
  (the quantizer at depth 0), which seeds `Theme::adapted`, the port of
  `AdaptedMaterialScheme`;
- the blurred art with its 0.2 saturation boost and the 0.3 `colLayer0` wash;
- the `cava` wave, blurred;
- the art fading in and out;
- the title and artist sliding on change;
- the wavy `StyledSlider` or `StyledProgressBar` in the scheme's colors;
- the play and transport buttons pressed on down;
- the placeholder;
- the three global shortcuts.

The art backdrop is blurred with GTK's blur at radius 70 (`ART_BLUR`), which
differs from `MultiEffect`'s; the result is within about 1.3 levels of qs on
average.

A player that gives neither a title nor an artist shows as its app: the
`Identity` as the title, the app's icon at 60 % of the art square, no time and
no progress row, and the play button at the bottom, as in qs.

The IPC target `mediaControls` has `toggle`, `open` and `close`. As in the
QML, opening it over IPC times out every notification popup, and the global
shortcuts do not.
