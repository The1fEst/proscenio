---
title: The bar
sidebar_label: The bar
description: The bar on each screen, how it reserves space, hides and styles itself, and what each of its groups, indicators and popups shows and does.
---

# The bar

Each screen gets a bar, along its top edge unless set otherwise. Its middle holds three groups: media
and resource meters, the [workspaces](workspaces.md), and the clock with the utility buttons and the
battery. The space on either side is not empty: scrolling over the left half changes the brightness,
scrolling over the right half changes the volume, and the right end carries the [tray](tray.md), the
keyboard layout button and the button that opens the [sidebar](sidebar.md).

```text
⇕ brightness  (privacy)   [weather] (media mixer · RAM swap CPU) (workspaces) (clock · buttons · battery)     tray •  (layout)  (indicators)
```

| Part | Wheel | Left click |
|---|---|---|
| Left region, from the screen edge to the center groups | brightness | nothing; the [privacy indicator](#privacy-indicator) opens its popup |
| Media and resources group | nothing | the media ring opens the media controls, the mixer ring the volume mixer |
| Workspaces group | previous or next workspace | focuses the workspace |
| Clock group | nothing | the clock opens the calendar; anywhere else toggles the sidebar |
| Right region, from the center groups to the screen edge | volume | toggles the sidebar |

Almost every option lives on **Appearance → Bar** in the [settings window](settings-appearance.md),
which writes `~/.config/proscenio/config.toml`. A change under `[bar]` rebuilds the bar on every
screen at once. The last section of this page lists every key.

## One bar per screen

Every connected screen gets a bar unless `bar.screenList` names the screens that should, by connector
name (`DP-1`, `HDMI-A-1`, …). The switches under **Monitors** on the Bar page edit that list and
always leave at least one screen on. An empty list, or one that names no connected screen, puts a bar on
every screen.

:::note

The list decides more than the bar: a screen left out of it gets none of the shell's per-screen
surfaces, so no sidebar, dock or desktop background either.

:::

The bar hides:

- while the screen is locked;
- while a fullscreen window covers the active workspace of its screen. A 1×1 transparent surface on
  the background layer (namespace `proscenio:barReserve`) keeps the bar's exclusive zone in the
  meantime;
- when told to, on every screen at once:

| Shortcut | IPC call | Effect |
|---|---|---|
| `proscenio:barToggle` | `proscenio ipc call bar toggle` | shows or hides the bar |
| `proscenio:barOpen` | `proscenio ipc call bar open` | shows it |
| `proscenio:barClose` | `proscenio ipc call bar close` | hides it |

The bar starts shown; whether it was hidden is not remembered across restarts. One second after it
hides, the bar gives back its surface and buffers. Its widgets stay, so it comes back at once. See
[Shortcuts](shortcuts.md) and [IPC](ipc.md) for binding these.

## The window

The bar is a layer-shell surface on the `top` layer, anchored to its edge and stretched along it. It
never takes keyboard focus, so hovering it does not interrupt typing in another window. A
[tray](tray.md) menu gets its keys from its popup grab.

| Namespace | Surface |
|---|---|
| `proscenio:bar` | a horizontal bar |
| `proscenio:verticalBar` | a vertical bar |
| `proscenio:barReserve` | the placeholder that holds the space while a fullscreen window hides the bar |

The surface is thicker than the bar. It adds Hyprland's `decoration:rounding` (23 px when Hyprland
does not say) to make room for the rounded corners under the bar, in every style. When
Hyprland reloads its config with a different rounding, the bar rebuilds.

Only the bar itself takes input: a strip as thick as the bar, grown by
`bar.autoHide.hoverRegionWidth` pixels (2 by default) on each side. The rest of the surface,
corners included, lets clicks through to the windows below.

### Exclusive zone

| Style | Horizontal bar | Its exclusive zone | Vertical bar | Its exclusive zone |
|---|---|---|---|---|
| Hug, Rect | 40 px | 40 px | 46 px | 46 px |
| Float | 50 px | 45 px | 56 px | 51 px |

A floating bar is drawn 5 px in from every edge of its strip. Its exclusive zone covers the bar and
the gap on the screen side, not the gap on the inside, so windows keep their usual gap from it.

### Clicking the bar while a panel is open

The bar is not part of any panel's focus grab. With the sidebar, the calendar or another panel open,
a click on the bar closes that panel.

### Dead pixel workaround

**System → Advanced → Dead pixel workaround** (`interactions.deadPixelWorkaround.enable`, off by
default) stretches a horizontal bar one pixel past the right edge of the screen, and past the bottom
edge for a bar at the bottom. Turn it on when the last column of pixels does not react to the
pointer.

## Style

| Setting | Key | Values |
|---|---|---|
| Bar position | `bar.bottom`, `bar.vertical` | Top, Bottom, Left, Right; Left and Right are the vertical bar |
| Bar style | `bar.cornerStyle` | `0` Hug (default), `1` Float, `2` Rect |
| Show background | `bar.showBackground` | `true` by default |
| Shadow when floating | `bar.floatStyleShadow` | `true` by default; Float only |

- **Hug** fills the strip edge to edge in `colLayer0` and draws rounded corners under it.
- **Float** draws the bar as a rounded rectangle (18 px radius, 1 px border) inset by 5 px, with a
  soft shadow unless **Shadow when floating** is off.
- **Rect** fills the strip edge to edge with square ends and no corners.
- With **Show background** off the bar is transparent and the pill behind each group is hidden. Text
  and icons, the resource rings, the battery and the workspace indicator sit on the wallpaper with a
  soft halo in `colLayer0`. Hug draws no corners then.

Bar position and Bar style also appear on the **Quick** page of the settings window.

### Rounded corners

In Hug style, two concave corners hang under the bar, one at each end, in the bar's color. Their
radius is Hyprland's `decoration:rounding`, so the bar and the windows below it read as one shape.
A bottom bar has them above it, a vertical bar on its inner side at the top and bottom ends. They
are drawn on the bar's own surface and let clicks through. The rounded corners at the other corners
of the screen are a separate feature: see [Screen corners](screen-corners.md).

## Auto-hide

**Automatically hide** (`bar.autoHide.enable`, off by default) keeps the bar out of sight until the
pointer touches the screen edge. The bar slides in over 200 ms and slides out when the pointer leaves
it.

- The edge that reveals it is `bar.autoHide.hoverRegionWidth` pixels deep (**Hover region thickness**,
  2 by default). The same strip extends a visible bar's input area past its inner edge.
- **Push windows away** (`bar.autoHide.pushWindows`, off by default) reserves the bar's space while it
  is shown, so windows move out of its way. Without it the revealed bar lies over them. A hidden bar
  reserves nothing either way.
- Holding Super also reveals it; see the next section.

## Holding Super

Holding Super for a moment reveals an auto-hidden bar and shows workspace numbers in place of dots
(see [Workspaces](workspaces.md)). This needs the `workspaceNumber` shortcut bound to the Super key
twice in Hyprland's config, once on press and once on release:

```lua
hl.bind("SUPER_L", hl.dsp.global("proscenio:workspaceNumber"), {
    ignore_mods = true,
    transparent = true,
})
hl.bind("SUPER_L", hl.dsp.global("proscenio:workspaceNumber"), {
    ignore_mods = true,
    transparent = true,
    release = true,
})
```

Bind `SUPER_R` the same way to use either Super key.

| Setting | Key | Default |
|---|---|---|
| Reveal bar and workspace numbers | `bar.autoHide.showWhenPressingSuper.enable` | `true` |
| Hold delay (ms) | `bar.autoHide.showWhenPressingSuper.delay` | `140` |

Releasing Super before the delay runs out shows nothing; releasing it later undoes both effects at
once. The switch applies with or without auto-hide.

## Narrow screens

The bar drops widgets on narrow screens, judged by the screen's width in logical pixels. **Verbose**
(`bar.verbose`, on by default) adds the date and the utility buttons on wide screens.

| Screen width | Width of each side group | Left out |
|---|---|---|
| over 1200 px | 360 px, or 140 px with Verbose off | nothing |
| 1001 to 1200 px | 280 px | the utility buttons; every tray item moves into the overflow menu |
| 1000 px or less | 190 px | also the date, the media and mixer rings and the battery |

The side groups are at least this wide and grow if their contents need more. The three center groups
stay centered on the screen until a side region runs out of room; then they shift away from it.

## The center groups

![The three center groups of the bar: media and resource meters, the workspaces, and the clock with the utility buttons](/img/screenshots/bar.webp)

Each group sits on a pill in `colLayer1` with a 12 px radius, 32 px tall inside the 40 px bar, with
5 px of padding at its ends and 4 px between widgets.

### Media

A 20 px ring that fills with the position in the current track, with a note icon in the middle, or a
pause icon while something plays. It follows the player the [media controls](media-controls.md)
follow.

| Button | Action |
|---|---|
| Left | opens or closes the media controls |
| Middle | play or pause |
| Right, or Forward (button 9) | next track |
| Back (button 8) | previous track |

### Mixer

A 20 px ring that fills with the volume of the default output, with a `tune` icon in the middle. It
sits after the media ring, and exists only when a sound server was running as the shell started.

| Button | Action |
|---|---|
| Left | opens or closes the [volume mixer](volume-mixer.md) |

### Resources

Three rings with their reading in percent: memory (`memory` icon), swap (`swap_horiz`) and CPU
(`planner_review`). A ring turns to the error color when its reading reaches its warning threshold.
The numbers sit in boxes as wide as `100`, so the row keeps its width as they change.

| Reading | Source | Warning threshold | Default |
|---|---|---|---|
| Memory | `/proc/meminfo`: `MemTotal` minus `MemAvailable` | `bar.resources.memoryWarningThreshold` | 95 |
| Swap | `/proc/meminfo`: `SwapTotal` minus `SwapFree` | `bar.resources.swapWarningThreshold` | 85 |
| CPU | the first line of `/proc/stat`, busy time over total since the last reading | `bar.resources.cpuWarningThreshold` | 90 |

The thresholds are on the Bar page under **Resources**. The readings refresh every
`resources.updateInterval` milliseconds (3000 by default; **System → Services → Polling interval**),
the interval of the shell's shared background loop.

Hovering the rings opens a popup with three columns: RAM with Used, Available (`MemAvailable`) and
Total in GiB, Swap with Used, Free and Total in GiB, and CPU with its load. The Swap column is left
out on a system without swap.

### Clock

The time, plus a `•` and the date on wide screens with Verbose on. The formats come from the
`[time]` table and follow edits to `config.toml` without a restart:

| Setting | Key | Default |
|---|---|---|
| Time format | `time.format` | `hh:mm` |
| Date (beside the clock) | `time.dateFormat` | `ddd, dd/MM` |
| Short date (vertical bar) | `time.shortDateFormat` | `dd/MM` |
| Seconds | `time.secondPrecision` | `false` |

These are on **System → Date & Time** in the [settings window](settings-system.md). The clock ticks on
the minute, or every second with `time.secondPrecision` or when a format it shows has seconds: the time
format, the date format beside the clock, or the short date on a vertical bar. A format is built from
these letters; any other character is printed as it is:

| Letters | Meaning |
|---|---|
| `h` `hh` | hour, 12-hour when the format has `AP` or `ap`, else 24-hour; `hh` pads to two digits |
| `H` `HH` | hour, always 24-hour |
| `m` `mm`, `s` `ss` | minutes, seconds |
| `AP` `ap` | `AM`/`PM` or `am`/`pm` |
| `d` `dd` | day of the month |
| `ddd` `dddd` | short or full weekday name |
| `M` `MM` | month number |
| `MMM` `MMMM` | short or full month name |
| `yy` `yyyy` | two- or four-digit year |

A left click on the clock opens or closes the [calendar](calendar.md). A left click anywhere else in
the clock group that is not a button, such as the battery or the space between widgets, toggles the
sidebar.

Hovering the clock opens a popup with the full date, the system uptime, and up to five unfinished
tasks from the calendar's to-do list, numbered, with "… and N more" past five, or "No pending tasks".

### Recording indicator

While a screen recording runs, a red pill appears after the clock with a stop icon and the elapsed
time, counted from when the shell sees `wf-recorder` running, not while a region is picked. Clicking
it stops the recording. Recordings start from the region selector or the Record utility button; see
[Region selector](region-selector.md). The pill shows as soon as either of them asks for a recording,
and stays, at 0:00, for as long as the Record button's `slurp` region is being picked. Clicking it
before `wf-recorder` runs ends the pending `proscenio record` and its pick, and no recording starts.

### Utility buttons

A row of round buttons after the clock, shown on wide screens with Verbose on. Each one is switched on
**Appearance → Bar → Utility buttons**, under `[bar.utilButtons]`:

| Button | Key | Default | Action |
|---|---|---|---|
| Screen snip | `showScreenSnip` | on | opens the [region selector](region-selector.md) for a screenshot |
| Record | `showScreenRecord` | off | runs `proscenio record`: pick a region with `slurp`, record it with `wf-recorder` |
| Color picker | `showColorPicker` | off | runs `hyprpicker -a`, which copies the picked color |
| Keyboard toggle | `showKeyboardToggle` | on | shows or hides the [on-screen keyboard](on-screen-keyboard.md) |
| Mic toggle | `showMicToggle` | off | runs `wpctl set-mute @DEFAULT_SOURCE@ toggle`; the icon shows whether the microphone is muted |
| Dark/Light toggle | `showDarkModeToggle` | on | switches the [color scheme](colors.md) between dark and light and keeps the wallpaper |
| Performance profile toggle | `showPerformanceProfileToggle` | off | cycles power saver, balanced and performance; without a performance profile it switches between balanced and power saver |

The performance profile button needs power-profiles-daemon.

### Battery

On a machine with a battery, a small capsule at the end of the clock group fills to the charge level,
with the percentage cut out of the fill and a bolt before it while charging below 100 %. The fill
turns to the error color at or below `battery.low` (20 % by default) while not charging. The reading
comes from UPower's display device, so it covers every battery at once.

Hovering it opens a popup:

| Row | Shows |
|---|---|
| Time to empty / Time to full | hours and minutes; left out when fully charged or unknown |
| Discharging / Charging | the rate in watts, or "Fully charged" |
| Health | the first battery's capacity against its design, in percent |

The low, critical and full warnings and the automatic suspend are set on the
[Power](settings-power.md) page.

## The left region: brightness

The left region runs from the screen edge to the center groups. Scrolling over it changes the
brightness of the focused monitor, and the [on-screen display](osd.md) shows the level.

- Scrolling up first raises the screen gamma by 5 points while it is under 100 %, then the monitor's
  brightness by 5 %.
- Scrolling down lowers the brightness by 5 % until it reaches zero, then the gamma by 5 points, down
  to 25 %.

Past zero brightness the screen keeps dimming through the gamma, which scales the same color matrix
night light sets (see [Night light](settings-displays.md#night-light)); the brightness goes through
`brightnessctl` or `ddcutil`.

Hovering the region reveals a hint at the screen edge: arrows around a sun icon (`light_mode` at full
gamma, `wb_twilight` below it), with the tooltip "Scroll to change brightness" after half a second.

After a scroll, moving the pointer more than 20 px or leaving the region closes the on-screen display
at once instead of waiting for it to time out. The right region does the same for volume.

### Privacy indicator

Pill-shaped chips 23 px from the screen edge, 27 px tall and wider than tall, one per kind in use,
in this order: microphone (`mic`), screen sharing (`screen_share`), camera (`videocam`). Nothing is
shown while nothing is in use. Each icon is centered by the pixel rows its glyph inks. In the
vertical bar they are the same pills, stacked. The chips have fixed colors, whatever the palette or the dark or light mode, with white icons (about
3.5:1 contrast on each fill at rest; hovering darkens the fill):

| Kind | Fill |
|---|---|
| Microphone | `#D46A00` |
| Screen sharing | `#6D7CFD` |
| Camera | `#1F9D44` |

A left click on a chip opens a popup with a section for each active kind, marked with a badge in the
kind's color, listing the applications with their icons and what they use. A click elsewhere closes
the popup, and it closes by itself when nothing is in use any more. It refreshes while open.

While a fullscreen window covers the bar of a screen, a dot in each active kind's color shows in the
corner where the chips sit: top left for a horizontal bar at the top, bottom left for one at the
bottom, top left or top right for a vertical bar on the left or right. The dots are a layer surface
on the `overlay` layer (namespace `proscenio:privacyDots`) with no input region and an exclusive
zone of -1, which places them against the screen edges whatever the bar still reserves. They take
no clicks or focus. They show only on screens whose bar is covered, and appear
and disappear with the chips.

| Kind | Detected by | Popup rows |
|---|---|---|
| Microphone | the sound server's recording streams, followed through PulseAudio events | each application with the source it records from |
| Screen sharing | Hyprland's `screencastv2` event | each application reading the share, with what is shared: `Monitor: DP-2`, `Window: <title>`, `Region: DP-2` or `Workspace: <name>` |
| Camera | a process holding `/dev/video<N>` open, found by scanning `/proc/*/fd` | each application with the camera's name from `/sys/class/video4linux/video<N>/name` |

- A recording stream does not count while it is corked or when it records from a monitor source,
  which the shell's own audio visualizer and the audio of its screen recordings do.
- The shell's own window captures for the dock and overview previews do not count as sharing: a
  window session whose title belongs to a window the shell requested a capture of within the last
  2 s is the shell's own. Monitor and region sessions never are. A session counts as sharing only
  after it has lasted 1 s, so a screenshot, which opens a capture for a frame, never shows; a
  session restarting under a target already shown counts at once. Hyprland ends a session when no
  frame was copied for half a second, which a static screen causes, so a session going off does not
  end the sharing at once. After 3 s the sharing continues while one of these holds, checked once
  at that point and then on each tick of the shared background loop, only while a share is shown
  and static:
  - PipeWire still has a screen `Video/Source` node linked into a `Stream/Input/Video` node,
    which is the case for a portal share (xdg-desktop-portal-hyprland) for its whole session;
  - a process named `wf-recorder` or `wl-screenrec` is running. Hyprland gives no sign when a
    screencopy client stops, so the name is the only signal, and a client not in this list is
    dropped 3 s after its last frame on a static screen. While such a process runs, a finished
    share of another kind stays shown too.

  The applications come from `pw-dump`: the active links from a screen `Video/Source` node into a
  `Stream/Input/Video` node. A screen recorder that does not go through PipeWire, such as
  `wf-recorder`, has no application row, only one row per share; so does a system without
  `pw-dump`.
- The camera scan runs on a worker thread with each tick of the shared background loop (3 s by
  default, see `resources.updateInterval`), and is skipped when `/sys/class/video4linux` is empty.
  A virtual camera made by v4l2loopback is in use only while a producer feeds it (its `state` file
  reads `capture`), and the holder that started first is taken for the producer and left out. PipeWire
  and WirePlumber holding a camera for an application are not listed; the applications streaming
  that camera are, from `pw-dump`. It runs when the shared targets or the cameras PipeWire holds
  change and when the popup opens, and up to three more times on later ticks while no reader has
  shown up.

### Weather

With **Weather → Enable** on the Bar page (`bar.weather.enable`, off by default), a pill with the
condition icon and the temperature sits at the inner end of the left region, beside the media and
resources group. It shows `--°` until the first report arrives.

- A right click fetches the report again and says so in a notification.
- Hovering it opens a popup with the place, the temperature and what it feels like, eight cards (UV
  index, wind and its direction, precipitation, humidity, visibility, pressure, sunrise, sunset), and
  the time of the last refresh.

The report comes from `https://wttr.in/<place>?format=j1`, fetched with `curl`. The rest of the
options are on **System → Services → Weather**:

| Setting | Key | Default |
|---|---|---|
| Enable GPS based location | `bar.weather.enableGPS` | `true` |
| Fahrenheit unit | `bar.weather.useUSCS` | `false` |
| City name | `bar.weather.city` | empty |
| Polling interval (m) | `bar.weather.fetchInterval` | `10` |

With GPS on, the shell creates a GeoClue client on the system bus (`CreateClient`, so GeoClue 2.5 or
newer), under the desktop ID `proscenio`, and fetches the weather for the coordinates it reports.
When GeoClue is missing or refuses, a notification says so, and the city name is used instead.
GeoClue is only asked while the weather is enabled: turning GPS or the weather off stops and deletes
that client, also when it is still starting, and a location that still arrives is ignored. When
fetches overlap, only the report of the newest one is shown.

## The right region

The right region runs from the center groups to the screen edge. It holds, from the edge inward, the
sidebar button, the keyboard layout button, the [tray](tray.md) and empty space.

- Scrolling over it changes the volume of the default output: 1 % per step below 10 %, 2 % per step
  above, up to 100 %. A hint with a `volume_up` icon at the screen edge reads "Scroll to change
  volume".
- A left click anywhere in it that is not a button or a tray icon toggles the sidebar.

### The keyboard layout button

A pill between the tray and the sidebar button, shown when Hyprland has more than one layout and
left out, with no gap, otherwise. It shows the current layout's code, cut to four letters before any
`-` and upper-cased; a two-part code stacks on two lines. When the layout changes, the code slides
out and the new one slides in.

| Input | Action |
|---|---|
| Left click | switches every keyboard to the next layout, as the `xkbLayoutNext` shortcut does |
| Wheel down | next layout |
| Wheel up | previous layout |
| Right click | opens the Keyboard page of the [settings window](settings-input.md) |

Both directions wrap around the list. Hovering the button shows the current layout's full name.
Scrolling over it switches layouts instead of changing the volume.

### The sidebar button

A pill 23 px from the screen edge that holds the indicators and toggles the sidebar. Pointing anywhere
in the right region lights it up, not just pointing at the button. While the sidebar is open the pill
turns to the secondary container color and its icons fade to match over 200 ms.

### Indicators

From left to right; the ones that come and go slide in and out:

| Indicator | Shown | Looks like |
|---|---|---|
| Updates | while the shell is behind its GitHub repository or the package count reaches `updates.adviseUpdateThreshold`; see [Updates](sidebar.md#updates) | `deployed_code_update`, red from `updates.stronglyAdviseUpdateThreshold` packages |
| Speaker muted | while the default output is muted | `volume_off` |
| Microphone muted | while the default input is muted | `mic_off` |
| Notifications | while Do Not Disturb is on, or unread notifications wait | a bell with a dot for unread ones, or `notifications_paused` under Do Not Disturb |
| Network | always | `lan` on a wired connection; Wi-Fi bars in six steps of signal strength; `wifi_find` when not connected; `signal_wifi_off` with Wi-Fi off |
| WireGuard | while a NetworkManager connection named `WireGuard` is active | the WireGuard logo |
| Bluetooth | when a Bluetooth adapter exists | `bluetooth_connected` with a device connected, `bluetooth` when on, `bluetooth_disabled` when off |

**Unread indicator: show count** (`bar.indicators.notifications.showUnreadCount`, off by default)
turns the dot into a badge with the number. Opening the sidebar marks every notification read. See
[Notifications](notifications.md).

## Hover popups

The resources, clock, battery and weather popups open when the pointer enters their widget and close
when it leaves, with no delay and no animation. Each one opens just past the bar's inner edge, lined
up with its widget, on a `m3surfaceContainer` card with a 12 px radius. They are GTK popovers on the
bar's own surface and refresh while open. The privacy popup opens on a click instead.

## The vertical bar

**Bar position → Left** (`bar.vertical = true`) puts the bar on the left edge; **Right** sets
`bar.bottom` as well. The vertical bar is 46 px wide, 56 px in Float style.

From top to bottom:

- **The top part**, above the groups: the [privacy indicator](#privacy-indicator) with its icons
  stacked, and scroll for brightness.
- **Three groups**, centered on the screen's height:
  - the three resource rings without numbers, a line, the media ring and the mixer ring. Hovering
    the media ring shows the track title and artist, or "No media", unless the media controls are
    already open;
  - the workspaces, as a column;
  - the clock as hours over minutes, a smaller AM/PM line for 12-hour formats, and the short date
    (`time.shortDateFormat`), then a line and the battery: an upright capsule that fills from the
    bottom, with a check when full, a bolt while charging, or a level icon, and the number under
    100 %.
- **The bottom part**: the tray as a column, the keyboard layout button and the sidebar button with
  the indicators stacked. Scroll here for volume, except over the layout button; a click toggles the
  sidebar.

The vertical bar has no weather, utility buttons, recording indicator, WireGuard indicator or scroll
hints. Screen width does not shorten it, and its tray never folds into the overflow menu. Hug corners
sit on its inner side at both ends, popups open beside it, and the special workspace pill reads `S`.

## Settings reference

Every key lives in `~/.config/proscenio/config.toml`. A dotted key is a TOML table path:
`bar.autoHide.enable` is `enable` under `[bar.autoHide]`.

| Key | Default | In the settings window |
|---|---|---|
| `bar.bottom`, `bar.vertical` | `false`, `false` | Appearance → Bar → Bar position |
| `bar.cornerStyle` | `0` (Hug) | Appearance → Bar → Bar style |
| `bar.showBackground` | `true` | Appearance → Bar → Show background |
| `bar.floatStyleShadow` | `true` | Appearance → Bar → Shadow when floating |
| `bar.verbose` | `true` | Appearance → Bar → Verbose |
| `bar.screenList` | `[]` | Appearance → Bar → Monitors |
| `bar.autoHide.enable` | `false` | Appearance → Bar → Automatically hide |
| `bar.autoHide.pushWindows` | `false` | Appearance → Bar → Push windows away |
| `bar.autoHide.hoverRegionWidth` | `2` | Appearance → Bar → Hover region thickness |
| `bar.autoHide.showWhenPressingSuper.enable` | `true` | Appearance → Bar → Reveal bar and workspace numbers |
| `bar.autoHide.showWhenPressingSuper.delay` | `140` | Appearance → Bar → Hold delay |
| `bar.resources.memoryWarningThreshold` | `95` | Appearance → Bar → Resources |
| `bar.resources.swapWarningThreshold` | `85` | Appearance → Bar → Resources |
| `bar.resources.cpuWarningThreshold` | `90` | Appearance → Bar → Resources |
| `bar.indicators.notifications.showUnreadCount` | `false` | Appearance → Bar → Unread indicator: show count |
| `bar.weather.enable` | `false` | Appearance → Bar → Weather |
| `bar.weather.enableGPS`, `useUSCS`, `city`, `fetchInterval` | `true`, `false`, empty, `10` | System → Services → Weather |
| `bar.utilButtons.*` | see Utility buttons above | Appearance → Bar → Utility buttons |
| `bar.workspaces.*` | see [Workspaces](workspaces.md) | Appearance → Bar → Workspaces |
| `tray.*` | see [System tray](tray.md) | Appearance → Bar → Tray |
| `time.format`, `dateFormat`, `shortDateFormat`, `secondPrecision` | `hh:mm`, `ddd, dd/MM`, `dd/MM`, `false` | System → Date & Time |
| `resources.updateInterval` | `3000` | System → Services → Polling interval |
| `updates.enableCheck`, `checkInterval`, `adviseUpdateThreshold`, `stronglyAdviseUpdateThreshold` | `true`, `120`, `1`, `200` | System → Services → Updates |
| `battery.low` | `20` | Power → Battery |
| `interactions.deadPixelWorkaround.enable` | `false` | System → Advanced |

## Where it lives

| Source | Holds |
|---|---|
| `src/main.rs` (`open_bar`) | the layer surface, input region, exclusive zone, auto-hide and Super reveal |
| `src/screens.rs` | one bar per screen, `bar.screenList`, rebuilding on config changes |
| `src/panels/bar/mod.rs` | the horizontal layout, the groups, the left and right regions, the corners |
| `src/panels/bar/strip.rs` | the three-part layout that keeps the center groups centered |
| `src/panels/bar/vertical.rs` | the vertical bar |
| `src/panels/bar/*.rs` | one file per widget: `media`, `mixer`, `resources`, `clock`, `recording`, `utilbuttons`, `battery`, `weather`, `privacy`, `keyboard`, `mute`, `network`, `wireguard`, `bluetooth`, `workspaces`, `tray`, `traymenu` |
| `src/services/media/deviceusage.rs`, `cameras.rs`, `privacy.rs` | what the privacy indicator lists |
| `src/panels/bar/privacydots.rs` | the dots over a fullscreen window |
| `src/ui/widgets/overlay/popup.rs` | the hover popups |
| `src/ui/reserve.rs` | the placeholder that holds the space while the bar hides for a fullscreen window |
