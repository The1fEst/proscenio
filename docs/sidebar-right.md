# The right sidebar

Source: `modules/ii/sidebarRight/`, plus the toggle models in
`modules/common/models/quickToggles/`. Read [foundations.md](foundations.md)
first.

The sidebar is a full-height layer-shell window on the right edge holding, top
to bottom: a system button row, an optional slider group, the quick toggle
panel, and a notification list. Six modal dialogs can cover the whole thing.

---

## 1. The window — `SidebarRight.qml`

One window, not a `Variants` per monitor. It has no explicit monitor, so the
compositor puts it on the focused output.

```qml
visible: GlobalStates.sidebarRightOpen
exclusiveZone: 0
implicitWidth: Appearance.sizes.sidebarWidth          // 460
WlrLayershell.namespace: "quickshell:sidebarRight"
WlrLayershell.keyboardFocus: open ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None
color: "transparent"
anchors { top: true; right: true; bottom: true }
```

`exclusiveZone: 0` respects other surfaces' exclusive zones and claims none,
which places the sidebar below the bar. A top margin added on top of that
moves it down by the bar height a second time.

`keyboardFocus` is `None` while closed.

### 1.1 Dismissal

```qml
onVisibleChanged: visible ? GlobalFocusGrab.addDismissable(panelWindow)
                          : GlobalFocusGrab.removeDismissable(panelWindow)
Connections { target: GlobalFocusGrab; function onDismissed() { panelWindow.hide() } }
```

The sidebar is a **dismissible** member of the shared grab (§foundations 5),
and the bar is a persistent member. Clicking the bar does not dismiss the
sidebar; clicking a window does.

Escape is handled on the content `Loader`, which holds `focus` while the
sidebar is open.

### 1.2 Content loader

```qml
active: GlobalStates.sidebarRightOpen || Config.options.sidebar.keepRightSidebarLoaded
anchors.fill: parent
anchors.margins: Appearance.sizes.hyprlandGapsOut      // 5
anchors.leftMargin: Appearance.sizes.elevationMargin   // 10
```

`keepRightSidebarLoaded` defaults to true: the content is built once and kept
across opens.

The extra 10 px on the left holds the drop shadow.

### 1.3 Other entry points

An `IpcHandler` on target `sidebarRight` with `toggle()`, `open()`, `close()`,
and three global shortcuts `sidebarRightToggle` / `sidebarRightOpen` /
`sidebarRightClose`.

**Status (proscenio).** `src/panels/sidebar/mod.rs` builds one window per monitor, layer Top,
namespace `proscenio:sidebarRight`, anchored top/right/bottom, `exclusive_zone(0)`,
`KeyboardMode::OnDemand`, Escape handler, and the same 5/5/5/10 margins.
Dismissal uses `hyprland_focus_grab_v1` directly (`src/platform/grab.rs`), with the
sidebar surface as the only grabbed one. The bar is not a persistent member:
a click on the bar clears the grab, which hides the sidebar, and the click
then lands on the bar.

The IPC target `sidebarRight` (`toggle`, `open`, `close`) and the global
shortcuts `sidebarRightToggle` / `sidebarRightOpen` / `sidebarRightClose`
(`src/main.rs`) act on the sidebar of Hyprland's focused monitor.

The keyboard mode stays `OnDemand`; a closed sidebar is unmapped.
`keepRightSidebarLoaded` is not read: the content is built once and kept.

Closing removes whichever dialog is open, the night light dialog included. In
the QML the night light dialog survives a close (§2.3).

---

## 2. The panel — `SidebarRightContent.qml`

### 2.1 Background

```qml
Rectangle {
    implicitWidth:  sidebarWidth − hyprlandGapsOut·2          // 450
    implicitHeight: parent.height − hyprlandGapsOut·2
    color: Appearance.colors.colLayer0
    border.width: 1
    border.color: Appearance.colors.colLayer0Border
    radius: Appearance.rounding.screenRounding − Appearance.sizes.hyprlandGapsOut + 1   // 23 − 5 + 1 = 19
}
```

with a `StyledRectangularShadow` behind it.

Inside, a `ColumnLayout` filling it with `margins: 10` and `spacing: 10`.

### 2.2 The column

1. `SystemButtonRow` — `Layout.topMargin: 5`, not filling height
2. the sliders `Loader`
3. the classic quick panel, loaded when `sidebar.quickToggles.style === "classic"`
4. the android quick panel, loaded when the style is `"android"` (the default)
5. `CenterWidgetGroup` — `Layout.fillHeight: true`, centered horizontally

The two quick panels are mutually exclusive `Loader`s. The slider loader is
active only when `quickSliders.enable` **and** at least one of
`showMic`/`showVolume`/`showBrightness` is on. The shipped default is
`enable: false`.

### 2.3 Closing resets the dialogs

```qml
onSidebarRightOpenChanged: if (!open) { showWifiDialog = showBluetoothDialog =
    showAudioOutputDialog = showAudioInputDialog = showWireGuardDialog = false }
```

`showNightLightDialog` is **not** in that list: the night light dialog
survives a close and reopen.

---

## 3. `SystemButtonRow`

An inline component at the bottom of `SidebarRightContent.qml` (there is no
`SystemButtonRow.qml` file). Height is the taller of its two halves.

**Left — the uptime pill.** A `Rectangle` of `colLayer1`, fully rounded
(`radius: height / 2`), sized `uptimeRow.implicitWidth + 24` by
`implicitHeight + 8`. Inside, spacing 8: a 25×25 `CustomIcon` of
`SystemInfo.distroIcon`, colorized to `colOnLayer0`, and the text
`"Up %1"` with `DateTime.uptime` at `pixelSize.normal` (16) in `colOnLayer0`,
rendered as Markdown.

**Right — four buttons.** A `ButtonGroup` of `colLayer1` with `padding: 4`,
holding `QuickToggleButton`s:

| Icon | Visible | Action | Tooltip |
| --- | --- | --- | --- |
| `edit` | android style only | toggle `editMode` | "Edit quick toggles" (plus drag instructions while on) |
| `restart_alt` | always | `hyprctl reload` then `Quickshell.reload(true)` | "Reload Hyprland & Quickshell" |
| `settings` | always | close the sidebar, `Session.openSettings()` | "Settings" |
| `power_settings_new` | always | `GlobalStates.sessionOpen = true` | "Session" |

`ButtonGroup` (§foundations) derives its own corner radii from its first and
last child plus the padding, and lays the children out in a `RowLayout` with
spacing 5.

**Status (proscenio).** `src/panels/sidebar/systemrow.rs` is the first child of the
sidebar column. On the left, the uptime pill, stretched to the row height,
with the distro icon `assets/icons/<family>-symbolic.svg`, embedded in the
binary by `src/core/assets.rs`, masked with the text color as `CustomIcon` does
(`src/ui/widgets/customicon.rs`), and `Up <uptime>` at 16 px built from
`/proc/uptime` in the `Nd, Nh, Nm` form. On the right, a `ButtonGroup`
(`src/ui/widgets/group.rs`), `colLayer1` with padding 4, of four
`GroupButton`s 40×40 that widen to 60 while pressed and squeeze their
neighbors, radius 20 falling to 12, each with the QML tooltip placed as the
`Popup` places it.

`edit` flips edit mode on the quick panel (§5.7). `restart_alt` runs
`hyprctl reload`, then re-executes `proscenio` in place of
`Quickshell.reload`. `settings` closes the sidebar and opens proscenio's
settings window ([settings.md](settings.md)). `power_settings_new` opens
proscenio's session screen ([session-screen.md](session-screen.md)).

---

## 4. `QuickSliders.qml`

A `Rectangle` of `colLayer1`, radius `Appearance.rounding.normal` (17),
padding 12 horizontal and 4 vertical, holding up to three `StyledSlider`s in
`Configuration.M`.

### 4.1 Brightness — one slider, two devices

The slider's 0…1 range is split at **0.3**:

```
value = gamma === 100
      ? 0.3 + brightness·0.7
      : (gamma − gammaLowerLimit) / (100 − gammaLowerLimit) · 0.3

onMoved:
  value >= 0.3 → setBrightness((value − 0.3) / 0.7); if gamma ≠ 100 setGamma(100)
  value <  0.3 → if brightness ≠ 0 setBrightness(0)
                 setGamma(value / 0.3 · (100 − gammaLowerLimit) + gammaLowerLimit)
```

The lower 30 % of the slider dims the screen through Hyprsunset's gamma and
the upper 70 % is the real backlight.

`stopIndicatorValues` marks where the backlight would resume
(`[0.3 + brightness·0.7]`) while the slider is down in gamma territory and the
backlight is not zero.

`dividerValues` puts a divider at the secondary icon's location, 0.3.

Tooltip: the backlight percentage, or `"Gamma <n>%"` while below 0.3.

### 4.2 The icons

The primary icon (`light_mode`, `volume_up`, `mic`) sits at the right end of
the track, `rightMargin: 8`. When `value >= 0.9` it jumps to
`quickSlider.handle.right` with `rightMargin: 14` and rides the handle. Its
color flips from `colOnSecondaryContainer` to
`colOnPrimary` at the same point. Both the margin and the color animate with
`elementMoveFast` (200 ms).

The secondary icon (`wb_twilight`, brightness only) sits at 0.3 on the track
and moves the same way, with the condition

```
nearIcon = 0.3 − value <= 0.1 && 0.3 − value > (handleWidth + 8 − 14) / effectiveDraggingWidth
```

and it turns `colOnPrimary` once `value >= 0.3 − 0.1`.

Both icons are 20 px.

### 4.3 Volume and microphone

Plain sliders bound straight to `Audio.sink.audio.volume` and
`Audio.source.audio.volume`.

**Status (proscenio).** `src/ui/widgets/slider.rs` draws the three sliders from the
same geometry as `StyledSlider.qml`: `leftPadding = rightPadding =
handleMargins = 4`, so positions run over `effectiveDraggingWidth = width − 8`.
The track and the two fills use the `leftValues` / `rightValues` split, with
dividers dropped when the handle comes within
`handleMargins + handleWidth/2 − dividerMargins` of them, the outer corners at
`trackRadius` (9) and every inner corner at `unsharpen` (2). The handle is
3 px wide, 1.5 while pressed, 39 tall. The stop dots are 3 px, flip from
`m3onSecondaryContainer` to `m3onPrimary` as the fill passes them, and default
to `[1]`. Dragging maps the pointer as `QQuickSlider::positionAt` does, over
`effectiveDraggingWidth − handleWidth`. The `nearFull` / `nearIcon`
handle-riding uses the QML conditions and offsets. The group is
`.sidebar-group`, `colLayer1`, radius 17, padding `4px 12px`.

The tooltip shows while the handle is held, the handle width animates between
3 and 1.5, and the icon animates its color and margin as the fill passes it.
The same widget, at `track: 18` and without an icon, is the slider inside the
dialogs (§6).

The brightness slider drives the monitor its sidebar is on, as
`Brightness.getMonitorForScreen(screen)` does. `src/services/brightness.rs`
ports `Brightness.qml`: one level per screen, DDC displays matched to screens
by DRM connector and read one after another, DDC writes held back 300 ms, and
one shared writer. The slider follows every change to that level and reads
the level again each time the sidebar opens; qs does not re-read. A backlight
fades to the new level over 200 ms on the `expressiveEffects` curve, as in qs,
with `brightnessctl` run whenever the whole percentage changes.

A value set from outside eases into place over `elementMoveFast` (200 ms,
`expressiveEffects`); the pointer moves the handle at once. In qs the value
jumps: `Behavior on value` is a `SmoothedAnimation` at
`elementMoveFast.velocity`, 850 units a second, which crosses the 0…1 range
within a frame.

---

## 5. The quick toggles — `AndroidQuickPanel.qml`

The default style. A `colLayer1` rectangle, radius 17, `padding: 6`,
`spacing: 6`.

### 5.1 The grid

```
columns        = sidebar.quickToggles.android.columns        (default 5)
toggles        = sidebar.quickToggles.android.toggles        (default: network 2,
                 bluetooth 2, idleInhibitor 1, mic 1, audio 2, nightLight 2 —
                 type and size, as in Config.qml)
baseCellHeight = 56
baseCellWidth  = (width − padding·2 − spacing·columns) / columns
```

The width formula subtracts `spacing · columns`, one spacing more than a row
of `columns` cells has gaps (`columns − 1`); the source comment marks it as
wrong.

Toggles are packed into rows greedily by `toggleRowsForList`: walk the list,
add each toggle's `size` to the running total, and start a new row as soon as
the total would exceed `columns`. A size-2 toggle after three size-1s in a
5-column grid starts a new row, leaving a 1-cell hole at the end of the
previous one; the packing does not backfill.

Each row is a `ButtonGroup` with `spacing: 6`. The panel's height animates
with **`elementMove`** (500 ms) when it changes.

The default layout is

```
network 2, bluetooth 2, idleInhibitor 1, mic 1, audio 2, nightLight 2
```

which packs as `[network, bluetooth, idleInhibitor]` then `[mic, audio,
nightLight]`.

### 5.2 A tile — `androidStyle/AndroidQuickToggleButton.qml`

A `GroupButton` (§foundations) with its click-bounce: pressing grows
`implicitWidth` to `baseWidth + (isAtSide ? 10 : 20)` over **`clickBounce`**
(400 ms, expressiveDefaultSpatial). Here the bounce animation is enabled only
while the pointer is over the tile and edit mode is off.

```
baseWidth  = baseCellWidth · cellSize + cellSpacing · (cellSize − 1)
baseHeight = 56
padding    = 6   (both axes)
buttonRadius        = toggled ? Appearance.rounding.large (23) : height / 2 (28)
buttonRadiusPressed = Appearance.rounding.normal (17)
```

An off tile is a **stadium** and an on tile is a 23-radius rounded
rectangle, and pressing squares it further to 17. Both `leftRadius` and
`rightRadius` animate with `elementMoveFast` (200 ms).

The tile fades in on creation: `opacity: 0` then 1 at
`Component.onCompleted`, over `elementMoveFast`.

Colors:

```
colBackground             = colLayer2
colBackgroundToggled      = (altAction && expandedSize) ? colLayer2       : colPrimary
colBackgroundToggledHover = (altAction && expandedSize) ? colLayer2Hover  : colPrimaryHover
colBackgroundToggledActive= (altAction && expandedSize) ? colLayer2Active : colPrimaryActive
```

**A wide tile with a menu never turns primary.** Its lit state is the icon
circle. A narrow tile, or one without a menu, fills with `colPrimary`.

```
colText = (toggled && !(altAction && expandedSize) && enabled)
        ? colOnPrimary
        : transparentize(colOnLayer2, enabled ? 0 : 0.7)
colIcon = expandedSize ? (toggled ? colOnPrimary : colOnLayer3) : colText
```

### 5.3 The icon slot

A square (`implicitWidth: height`) `Rectangle` with
`radius: root.radius − verticalPadding`, animated with `elementMove`
(500 ms), colored

```
transparentize(toggled ? colPrimary : colLayer3,
               (altAction && expandedSize) ? 0 : 1)
```

**Only a wide tile with a menu paints this circle.** On every other tile it
is fully transparent and the icon sits on the tile background.

The symbol is `fill: toggled ? 1 : 0`, size **22 on a wide tile, 24 on a
narrow one**, colored `colIcon`.

On a wide tile with a menu the icon slot is its own `MouseArea` accepting the
left button, with a state layer drawn as

```
transparentize(colIcon, containsPress ? 0.88 : containsMouse ? 0.95 : 1)
```

That is 0.12 and 0.05 opacity, not the Material state-layer values used
elsewhere (0.1 / 0.08).

### 5.4 The label column

Only on a wide tile. A `Column` with `spacing: -2`:

- the name at `pixelSize.smallie` (13), weight 600, elided right
- the status text at `pixelSize.smaller` (12), weight 100, elided right,
  hidden when empty

`statusText` defaults to "On"/"Off" when the model says it has status text but
supplies none.

### 5.5 Click routing

```qml
onClicked: if (expandedSize && altAction) altAction(); else mainAction()
```

On a **wide** tile the body opens the dialog and the icon circle runs the
toggle; on a **narrow** tile the body toggles and right-click (or press-and-
hold, via `GroupButton`'s `onPressAndHold`) opens the dialog.

### 5.6 The toggles

`availableToggleTypes` is

```
network, bluetooth, idleInhibitor, easyEffects, nightLight, darkMode,
cloudflareWarp, wireGuard, screenSnip, colorPicker, onScreenKeyboard,
mic, audio, notifications, powerProfile
```

Each is a `QuickToggleModel` — name, status text, tooltip, icon, `available`,
`toggled`, `mainAction`, `hasMenu`, `altAction`.

| Type | Name | Icon | Toggled when | Status text | Main action | Menu |
| --- | --- | --- | --- | --- | --- | --- |
| `network` | Internet | `Network.materialSymbol` | `wifiStatus !== "disabled"` | `Network.networkName` | `Network.toggleWifi()` | Wi-Fi dialog |
| `bluetooth` | Bluetooth | `bluetooth_connected` / `bluetooth` / `bluetooth_disabled` | adapter enabled | first connected device's name, else "Not connected" | flip `defaultAdapter.enabled` | Bluetooth dialog |
| `audio` | Audio output | `volume_up` / `volume_off` | not muted | "Unmuted" / "Muted" | `Audio.toggleMute()` | volume mixer |
| `mic` | Audio input | `mic` / `mic_off` | not muted | "Enabled" / "Muted" | `Audio.toggleMicMute()` | input mixer |
| `nightLight` | Night Light | `night_sight_auto` when automatic, else `bedtime` | `Hyprsunset.temperatureActive` | "Auto, Active" / "Inactive" | `Hyprsunset.toggleTemperature()` | night light dialog |
| `notifications` | Notifications | `notifications_active` / `notifications_paused` | not silent | "Show" / "Silent" | `Notifications.setSilent(!silent)` | — |
| `idleInhibitor` | Keep awake | `coffee` | `Idle.inhibit` | On/Off | `Idle.toggleInhibit()` | — |
| `easyEffects` | EasyEffects | `graphic_eq` | `EasyEffects.active` | On/Off | `EasyEffects.toggle()` | right-click launches the app and closes the sidebar |
| `darkMode` | Dark Mode | `contrast` | `m3colors.darkmode` | "Dark" / "Light" | run the wallpaper switch script with `--mode light\|dark --noswitch` | — |
| `cloudflareWarp` | Cloudflare WARP | `cloud_lock` | polled from `warp-cli status` | On/Off | `warp-cli connect` / `disconnect` | — |
| `wireGuard` | WireGuard | `vpn_key` | `nmcli connection show --active \| grep -q WireGuard` | On/Off | `nmcli connection up/down WireGuard` | WireGuard dialog |
| `screenSnip` | Screen snip | `screenshot_region` | never | none | close the sidebar, then after **300 ms** dispatch `quickshell:regionScreenshot` | — |
| `colorPicker` | Color picker | `colorize` | never | none | close the sidebar, then after **300 ms** run `hyprpicker -a` | — |
| `onScreenKeyboard` | Virtual Keyboard | `keyboard` / `keyboard_hide` | `GlobalStates.oskOpen` | On/Off | flip `oskOpen` | — |
| `powerProfile` | Power Profile | `energy_savings_leaf` / `airwave` / `local_fire_department` | profile ≠ Balanced | "Power Saver" / "Balanced" / "Performance" | cycle the profile | — |

The 300 ms delay on screen snip and color picker lets the sidebar finish
closing before the capture.

WireGuard and Cloudflare WARP both poll at `resources.updateInterval`
(3000 ms); WireGuard additionally re-checks 500 ms after its own action.

### 5.7 Edit mode

`editMode` is toggled by the `edit` button in the system row. While on:

- the panel grows to `contentItem.implicitHeight`, which includes a divider
  and a second section of unused toggles, both behind `FadeLoader`s. The
  divider is a 1 px `colOutlineVariant` line inset by `baseCellHeight / 2`
  (28) on each side.
- `unusedToggles` is every available type not already placed, each at size 1.
- a full-panel `MouseArea` takes over. Left-press picks the toggle under the
  cursor by hit-testing every cell; dragging past `QuickToggleDrag.threshold`
  starts a drag, and the panel recomputes a drop target on every move.
  Right-click, and press-and-hold, call `toggleSize(type)`, which is
  `size = 3 − size` — a flip between 1 and 2.
- the dragged tile's content drops to `opacity: 0.35`.
- `insertionIndex` sorts the cells by (y, x), finds the row the pointer is in
  by `floor(y / (56 + 6))`, and steps past a cell when the pointer is past its
  horizontal midpoint — or its *vertical* midpoint if that cell fills the row.

**Status (proscenio).** `src/panels/sidebar/toggles.rs` holds the sixteen models — name, status
text, tooltip, icon, `available`, main action and menu — with the `On`/`Off`
fallback `QuickToggleModel` applies when a toggle sets no status text.

Wi-Fi and the wired connection are separate tiles:

| Type | Name | Icon | Toggled when | Status text | Main action | Menu |
| --- | --- | --- | --- | --- | --- | --- |
| `network` | Wi-Fi | signal bars of the wireless device's active access point, `wifi_find` when not connected, `signal_wifi_off` when the radio is off | `wifiStatus !== "disabled"` | the SSID, "Disconnected" or "Off" | `nmcli radio wifi on\|off` | Wi-Fi dialog |
| `ethernet` | Ethernet | `lan` | the wired device is connected | the connection's name or "Disconnected" | `nmcli device connect\|disconnect` on the wired device | right-click opens the settings window at the `network` page and closes the sidebar |

The wired device is the first `ethernet` device in `nmcli device status` that
is connected, else the first that is disconnected, else the first that is
unavailable (no cable). Without one the tile is unavailable.
`onScreenKeyboard` runs proscenio's `oskToggle` action, which toggles its own
on-screen keyboard ([osk.md](osk.md)). `idleInhibitor` holds
`systemd-inhibit --what=idle:sleep --who=proscenio cat` with `cat` reading a
pipe from the shell, so the lock ends with the shell process, including a
restart through `exec`. The state is `idle.inhibit` in `states.json`; within
the same Hyprland instance a restarted shell takes the lock again, as qs's
`Idle` does. `src/panels/sidebar/quickpanel.rs` is
the panel: `colLayer1`, radius 17, padding 6, its height easing over 500 ms,
and each row a `ButtonGroup` spaced 6 whose cells take the QML width. That
includes the extra `spacing · columns` subtraction, so a full row stops one
spacing short of the right edge, and the Repeater that the QML `ButtonGroup`
counts as a child, which costs every row one more spacing.

`src/panels/sidebar/quicktoggle.rs` is the tile, a `GroupButton`:

- the three radii (28 off, 23 on, 17 pressed), with the icon disc at
  `radius − verticalPadding`;
- the click bounce that widens the pressed tile and squeezes its neighbors;
- state layers from `colLayer1Hover`/`Active` for an untoggled tile (the
  `GroupButton` default), `colPrimaryHover`/`Active` when toggled, and
  `colLayer2Hover`/`Active` for a wide tile with a menu, which stays
  `colLayer2`;
- the icon at 24 narrow, 22 wide, in `colOnLayer3` when wide and untoggled;
- the name at 13 px and the status at 12 px pulled up 2 px, both at
  `wght 450`: `StyledText` pins that axis regardless of `font.weight`;
- a disabled tile keeps its background, with its text at 30 % alpha;
- the fade-in and width slide when edit mode adds or moves a tile.

Edit mode follows the QML: the `edit` button in §3 turns it on, the divider and
the unused section fade in, a right click or an 800 ms hold toggles a tile
between one and two cells, and a drag past 6 px carries the tile with the
`insertionIndex` rule above, previewing the row it would land in and dropping
into either section. Every change is written to
`sidebar.quickToggles.android.toggles` in `~/.config/proscenio/config.toml`.
Closing the sidebar turns edit mode off, where the QML keeps it on for the next
opening.

An empty unused section stays one cell tall, so a toggle can always be dropped
there. In the QML it is only as tall as its rows: with every toggle placed it
has no height, and no toggle can be taken out.

The placed toggles end in an empty row as wide as the grid while edit mode is
on, and the whole grid width of every row takes a drop. A toggle dropped there
goes to the end of the list, so it starts a new row when the last one is full.
In the QML the drop area is only as wide and tall as the placed rows, so a new
row can only be started by first moving a toggle out of a full one.

The notifications tile reads and writes the silent flag of proscenio's
notification daemon ([notifications.md](notifications.md)).

`cloudflareWarp` is `src/services/warp.rs`. It reads `warp-cli status` once at
startup, as the QML `Process` does, sets `available` on any output, and runs
the registration and reconnect pair with its two failure notifications when
the status says `Unable`. `easyEffects` is `src/services/easyeffects.rs`, with
the same three shell probes as the QML service. Its right-click launches the
app and closes the sidebar; it is the one `altAction` that is not a dialog.

The classic quick panel (`style: "classic"`) is `src/panels/sidebar/classic.rs`:
a `ButtonGroup` in `colLayer1` with 5 px spacing and padding, holding 40 px
round `GroupButton`s that grow to 60 px when pressed, with a 22 px symbol that
fills when toggled. The order is network, Bluetooth, Night Light, keep awake,
EasyEffects, Cloudflare WARP and WireGuard; Bluetooth, EasyEffects and WARP
show only when available. The network button shows `lan` while the primary
connection is wired and the Wi-Fi symbol otherwise, with the active
connection's name in its tooltip. A toggle with a right-click action rounds to 17 px
while on. Right-click opens the settings window at the `network` (wired) or
`wifi` page and at the `bluetooth` page, flips Night Light's automatic mode,
starts EasyEffects, and opens the WireGuard dialog. Each toggle reads and acts
through `toggles.rs`, as on the Android panel.

---

## 6. The dialogs

Six, all driven the same way. `SidebarRightContent` holds a `bool` per dialog
and a `ToggleDialog` loader per dialog; the panel emits `openXDialog()` and the
loader sets its bool, activates, sets `item.show = true` and calls
`forceActiveFocus()`. `dismiss()` clears `show` and the bool; the loader
deactivates once the item has become invisible.

Two have side effects on open/close:

- **Bluetooth** — opening enables the adapter and starts discovery; closing
  stops discovery.
- **Wi-Fi** — opening calls `Network.enableWifi()` then `Network.rescanWifi()`.

### 6.1 `WindowDialog.qml`

The shared shell. A full-panel `Rectangle` acting as the scrim:

```
color  = show ? colScrim : transparentize(colScrim)    // animated, elementMoveFast
radius = Appearance.rounding.screenRounding − hyprlandGapsOut + 1   // 19
visible = dialogBackground.implicitHeight > 0
```

`colScrim` is `transparentize(m3scrim, 0.5)` — black at 50 %.

A `MouseArea` over the scrim dismisses on any button; a second `MouseArea`
over the dialog body swallows clicks so they do not reach the first. Escape
dismisses.

The body is a `Rectangle` of `m3surfaceContainerHigh` (**opaque**, not
`colLayer3`), radius `Appearance.rounding.large` (23),
`backgroundWidth` 350 by default, horizontally centered, with

```
y = show ? (root.height − backgroundHeight) / 2
         : that − backgroundAnimationMovementDistance          // 60
implicitHeight = show ? backgroundHeight : 0
```

Both animate over `elementMoveFast`'s duration (200 ms) with the curve
**switched by direction**: `emphasizedDecel` opening, `emphasizedAccel`
closing. The dialog grows from zero height while sliding down 60 px, and
collapses upward.

The content is a `ColumnLayout` inset by the body's own radius (23) with
`spacing: 16`, fading with `elementMoveFast`.

### 6.2 The row widgets

- `WindowDialogTitle` — `StyledText` in the title family, `pixelSize.title`
  (22), `wght 550`, `colOnSurface`.
- `WindowDialogSeparator` — 1 px `colOutline`, with **negative margins**
  (−23 left and right, −8 top and bottom) so it bleeds to the dialog edge and
  eats part of the 16 px spacing.
- `WindowDialogButtonRow` — a `RowLayout`, spacing 4, `Layout.margins: -8`
  with `topMargin: 0`.
- `DialogButton` — a `RippleButton`, height 36, padding 14, fully rounded,
  text at `pixelSize.small` (15) in `colPrimary` (`m3outline` when disabled),
  backgrounds `colLayer3` transparent / `colLayer3Hover` / ripple
  `colLayer3Active`.
- `DialogListItem` — a `RippleButton` with `buttonRadius: 0`, horizontal
  padding `Appearance.rounding.large` (23), vertical padding 12, clipped,
  height animated with `elementMove` (500 ms). When `active`, hover does
  nothing and the cursor stays an arrow.

Long lists inside a dialog use the same negative margins (−23 horizontally,
−15/−16 vertically) to reach the dialog edges.

### 6.3 The six

| Dialog | Content |
| --- | --- |
| Wi-Fi | title "Connect to Wi-Fi", an indeterminate progress bar while scanning, a list of `Network.friendlyWifiNetworks` as `WifiNetworkItem`s, buttons "Details" (opens the settings app at the Wi-Fi page and closes the sidebar) and "Done" |
| Bluetooth | adapter and device list, pairing flow through `bluetoothctl --agent NoInputNoOutput` |
| Volume mixer | per-application sliders and a device selector; two instances, `isSink` true and false |
| Night light | temperature and schedule |
| WireGuard | the list of WireGuard connections |

`Network.friendlyWifiNetworks` is the access point list sorted; `active` is
the one flagged active.

**Status (proscenio).** `src/ui/widgets/windowdialog.rs` is `WindowDialog`: the
scrim fading over 19 px corners, the `m3surfaceContainerHigh` body growing
from zero while sliding 60 px, `emphasizedDecel` opening and
`emphasizedAccel` closing, the content fading, and dismissal by a click on the
scrim or by Escape. Its column uses `ColumnLayout` arithmetic, not GTK's box:
a cell is `max(0, implicitHeight + topMargin + bottomMargin)`, so the
separators' negative margins collapse as in the QML, and the item is centered
in the rest of the cell. The row widgets are there too — title, section
header, separator, button row, `DialogButton`, `DialogListItem`, and the
Material indeterminate progress bar — and `src/ui/widgets/controls.rs` has
`StyledSwitch`, `ConfigSwitch`, `StyledComboBox` and `RippleButtonWithIcon`.

`src/panels/sidebar/dialogs.rs` builds the six on top of them: Wi-Fi with its
scan and password prompt, the list being `src/panels/wifinetwork.rs` (shared
with the settings page) over `src/services/wifi.rs`; Bluetooth with discovery,
expandable devices, pair, forget and connect; the two volume dialogs with a
slider per stream, the `Cookie7Sided` placeholder when nothing is playing,
and the device box; night light with its switches and the intensity,
brightness and gamma sliders; and WireGuard.

Divergences:

- The QML Wi-Fi scan never ends when `nmcli` prints nothing, and its progress
  bar runs forever; proscenio ends the scan when the command exits.
- In the QML, pressing a `ConfigSwitch` and releasing outside it flips the
  switch; proscenio acts only on a release inside.
- In the QML, pressing the switch itself, rather than its row, does not grow
  the thumb although `StyledSwitch` sets it; proscenio grows it.
- The password goes to NetworkManager over D-Bus (`AddAndActivateConnection`,
  replacing a saved profile of the same name), never on a command line. The
  QML passes it in `nmcli`'s arguments, readable by any process. For a network
  with no saved profile it runs `nmcli connection modify` on a missing profile
  and then repeats the last connection command, so a first connection to a
  secured network ignores the typed password.
- Controls inside a list item take their own clicks, as in Qt: the item's
  `RippleButton` handles clicks in the bubble phase
  (`set_click_phase(Bubble)`), and a press on a nested button or field neither
  ripples nor clicks the item. With the `RippleButton` default, the capture
  phase, the item takes the click and Cancel in the password prompt reopens it.
- The list keeps each network's item between refreshes and only moves it, so a
  refresh does not clear a password being typed.
- "Details" closes the sidebar and opens proscenio's settings window at the
  `wifi` page (`network` on a wired connection), `bluetooth` or `sound`.
  WireGuard's "New Connection" opens the `network` page.

---

## 7. `CenterWidgetGroup.qml` and the notification list

```qml
Rectangle {
    radius: Appearance.rounding.normal      // 17
    color: Appearance.colors.colLayer1
    implicitHeight: 170
    NotificationList { anchors.fill: parent; anchors.margins: 5 }
}
```

`implicitHeight: 170` is the minimum; the layout gives it `fillHeight` and
`Layout.minimumHeight: implicitHeight`, so it takes whatever is left.

`NotificationList.qml`:

- a `NotificationListView` filling everything above the status row, with 5 px
  of gap, clipped and masked by an `OpacityMask` with radius
  `Appearance.rounding.normal` (17).
- a `PagePlaceholder` over it, shown when the list is empty:
  icon `notifications_active`, description "Nothing", shape
  `MaterialShape.Shape.Ghostish`.
- a `ButtonGroup` pinned to the bottom with three `NotificationStatusButton`s:
  `notifications_paused` toggling silent mode, a disabled middle button
  reading "%1 notifications", and `delete_sweep` calling
  `discardAllNotifications()`.

`NotificationStatusButton` is a `GroupButton` of height 36,
`baseWidth: content.implicitWidth + 46`, `clickedWidth: baseWidth + 6`, radius
18 (half the height) falling to `Appearance.rounding.small` (12) while
pressed, backgrounds `colLayer2` / `colLayer2Hover` / `colLayer2Active`, and
text color `m3onPrimary` when toggled else `colOnLayer1`. Its icon is at
`pixelSize.huge` (22), its text at `pixelSize.small` (15), spacing 5.

The middle button is `enabled: false`, which in `GroupButton` locks the color
to `colBackground`: a label shaped like a button.

**Status (proscenio).** `src/panels/notifications/list.rs` builds the group at the bottom
of the sidebar column: a scrolled list of the same cards the popups use, built
with `popup: false`, clipped to the 17 px radius by `overflow: Hidden` rather
than an opacity mask; the `PagePlaceholder` centered over it while the list is
empty, with the real `Ghostish` shape (`src/ui/shapes.rs` ports androidx
`RoundedPolygon` and all 35 `MaterialShapes`) and
the 400 ms fade, rise and turn; and the three status buttons as a
`ButtonGroup` of `GroupButton`s that widen by 6 px while pressed, the middle
one locked to its background color and carrying the count. The silent button
and the `notifications` quick toggle flip the same daemon flag, stored as
`notifications.silent` in `~/.config/proscenio/config.toml`.

Divergence: the list has no scroll-edge fade.

---

## 8. Parity checklist

| Area | State |
| --- | --- |
| Window, anchors, exclusive zone, margins, Escape | done |
| Focus-grab dismissal | done |
| IPC handler and global shortcuts | done |
| `keyboardFocus: None` while closed | `OnDemand` always; the closed window is unmapped |
| Panel background, radius 19, border, shadow | done except the shadow |
| System button row (uptime pill + four buttons, tooltips, bounce) | done |
| Quick sliders: 0.3 split, stop indicator, riding icons, pressed tooltip | done |
| Android grid: packing, cell size, spans | done |
| Tile radii, icon circle, label column, colors | done |
| Tile click bounce, fade-in, height animation | done |
| 15 toggles, with their tooltips | done |
| Notifications toggle, with its real silent state | done |
| Edit mode: drag, resize, unused section | done |
| Classic quick panel | done |
| Six dialogs, scrim, dismissal | done |
| Dialog open/close animation and curves | done |
| Notification list, placeholder, status row | done |
