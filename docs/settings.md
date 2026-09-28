# The settings window

Source: `modules/ii/settings/SettingsWindow.qml`, over
`modules/common/widgets/NavigationRail*.qml`, `FloatingActionButton.qml`,
`MaterialTextField.qml` and `StyledFlickable.qml`. The pages themselves live in
`modules/settings/`.

## 1. The window

A `FloatingWindow`, so an ordinary toplevel rather than a layer surface:
titled "illogical-impulse Settings", 1100 × 750, at least 750 × 500,
`m3background`. The dots make that title float. It exists only while
`GlobalStates.settingsOpen`, which `Session.openSettings(page)`, the sidebar's
gear, its dialogs' Details buttons and the `settings` IPC target set.
`openPage` and `settingsPage` take a page by its component path, such as
`modules/settings/SoundConfig.qml`.

Inside, 8 px from every edge, a column with 5 px spacing:

- **The title bar**, shown with `windows.showTitlebar` (on by default):
  "Settings" in the title font at 22 px, weight 550, `colOnLayer0`, centered
  over the whole width, or 12 px from the left with `windows.centerTitle` off.
  A 35 px round close button with a 20 px `close` symbol sits on the right.
- **A row** with 8 px spacing: the navigation rail, 5 px in on every side, and
  the content pane, `m3surfaceContainerLow` with radius
  `windowRounding − 8` (10).

Escape closes the window. Ctrl+Page Down and Ctrl+Page Up step through the
pages and stop at the ends; Ctrl+Tab and Ctrl+Shift+Tab wrap around.

## 2. The navigation rail

It is expanded while the window is wider than 900 px; after the first press
of its toggle, the toggle alone sets it. Expanded, it is as wide as its widest tab, 56 + 20 + the
label, kept between 150 and 230; collapsed, 56. The width moves over
`elementMoveFast` (200 ms, expressive effects). A column with 10 px spacing:

- **The toggle**: 40 px, round, 8 px from the left, `menu_open` or `menu` at
  24 px in `colOnLayer1`, turned −180° while collapsed. It acts on press.
- **The FAB**: 56 px tall, radius 16, `colPrimaryContainer`. An `edit` symbol
  at 26 px centered in the first 56 px, then "Config file" at 14 px, weight
  450, 5 px after it, shown only while expanded, when the button also fills
  the rail. A press opens the config file; a right click copies its path and
  shows `check` / "Path copied" for 1.5 s. Its tooltip reads "Open the shell
  config file" and "Alternatively right-click to copy path".
- **The search field**, only while expanded, 4 px in from both sides. While
  it holds text, the tabs give way to a list of results: pages by name and
  keywords, and every section, subsection and control by its title, each with
  its page and sections below it at 12 px in `colSubtext` ("Screen Lock ›
  Style: Blurred"). Every word has to match the title or that trail; titles
  that start with the query come first, then titles that hold every word,
  then the rest. A result is 48 px tall with 12 px padding and a 60-result
  cap; "No settings found" stands in for an empty list. Pressing a result,
  or Enter for the first, opens its page and scrolls the setting to a third
  of the way down, tinting it `colPrimary` at 20 % for 1.5 s. Collapsing the
  rail clears the field.
- **The tabs**, scrolling: 56 px each. Those that start a group (Wi-Fi,
  Displays, Apps, Mouse & Touchpad, Accessibility) sit 13 px lower with a 1 px
  `colOutlineVariant` line in the middle of the gap, 8 px in, except while
  searching. A tab is a 24 px symbol centered in a 56 × 32 box, filled and in
  `m3onSecondaryContainer` when current, weight 600 when current or hovered;
  expanded, the name follows at 14 px in `colOnLayer1`. Hovering tints the
  tab `colLayer1Hover` and pressing `colLayer1Active`, fading over 200 ms; the
  current tab stays clear. A `colSecondaryContainer` pill behind the list
  marks it and slides to a new tab over 350 ms (expressive fast spatial).
  Expanded, the pill and the tints are the tab's full height; collapsed, the
  56 × 32 box. A tab selects its page on press, and the list scrolls to keep
  the current tab in view. Collapsed tabs show their name as a tooltip.

## 3. The pages

Each page is loaded on its own and unloaded when another is shown. A switch
fades the old one out over 100 ms, then fades the new one in over 200 ms while
it drops 20 px into place. A page may open a subpage, which takes its place
under a header: a 35 px back button and the subpage's name at 19 px.

## 4. The building blocks

Every page is a `ContentPage`: a scrolling column 600 px wide, or wider when
its content asks and the page does not force the width, centered, 20 px from
the top, with 30 px between sections and 80 px of room below the last.

- **`ContentSection`**: an optional header, a symbol at 23 px and a title at
  19 px in `colOnSecondaryContainer`, 6 px apart, then the rows 4 px apart. The
  symbol has no height of its own, so the header is as tall as its title.
- **`ContentSubsection`**: 4 px lower, a 15 px `colSubtext` label 2 px in and
  an optional `info` symbol whose tooltip explains it, then rows 2 px apart.
- **`ConfigSwitch`**: the sidebar's labeled switch, filling the width. A
  click hands the change to the page, then shows the setting's stored value;
  a refused write springs back. Its label takes the button's font, the KDE
  general font's family and weight (Google Sans Medium) at 15 px, not the main
  family. The track's 1.5 px border draws as two solid pixels.
- **`ConfigRow`**: switches side by side, 4 px apart. Qt's `RowLayout` gives
  the spare width to the filling items in proportion to their natural widths,
  not in equal parts, and rounds each item's position to a whole pixel. When
  nothing in a row can grow (a nested layout whose own items do not fill
  cannot either), every item's cell takes a share in proportion to its width
  and the item keeps its width at the cell's left, so the gaps widen. When the
  items want more than the row has, the filling ones give the difference back
  down to their minimum widths, each in proportion to
  `desired · (available / sumDesired)^(desired / sumDesired)` of what it
  could give (`growthFactorBelowPreferredSize` in `qgridlayoutengine.cpp`);
  labels that fill take a minimum of 0 and elide.
- **Disabled rows** (`enabled: false`, also through a disabled
  `ContentSubsection`): the symbol, the label and the spin box each at 40 %
  opacity; the subsection's own label stays. The stylesheet turns off the GTK
  theme's own dimming of disabled labels.
- **`NoticeBox`**: `colPrimaryContainer` with radius 17 and 8 px inside, a
  22 px symbol at the top and 8 px to its right the message, wrapping at
  words, both `colOnPrimaryContainer`.
- **Missing programs**: `Page::tools_notice` puts a `NoticeBox` where a
  control depends on an optional program that is not on `PATH`, naming the
  programs, what does not happen without them and their Arch packages
  (`src/core/tools.rs`); services are checked on the system bus (owned or
  activatable). The notice appears when the page is built. Where the control
  only writes the shell's config it stays; where it writes the tool's own
  files (hypridle) it is left out.

  | Page | Checked | Without it |
  | --- | --- | --- |
  | Sound | a pulse server | the page is only a notice |
  | Sound, Alert Sound | `/usr/share/sounds/*/stereo` | notice |
  | Displays, Night light | `hyprsunset` | notice |
  | Power | `hypridle`, `org.freedesktop.UPower` | idle rows left out; notice |
  | Screen Lock | `hypridle`, `hyprlock`, `gnome-keyring-daemon` | idle rows left out; notices; the lock falls back to the shell's own |
  | Welcome, Power saving | `hypridle` | notice only |
  | Privacy, Devices | `pw-dump` | notice |
  | Screenshots & Recording | `grim`, `magick`, `wl-copy`, `satty`, `wf-recorder`, `slurp` | notices |
  | Network, Saved Networks | `nmcli`, `org.freedesktop.NetworkManager` | the page is only a notice |
  | Network, "Set up connections" | the program of `/apps/network` | button disabled, tooltip names it |
  | Bluetooth | `org.bluez`; `bluetoothctl` | the page is only a notice; notice |
  | Appearance, Color generation | `matugen`, `plasma-apply-colorscheme` | notices |
  | Quick, "Choose file" | `kdialog` | button disabled, tooltip names it |
  | Bar, Utility buttons | `grim`, `magick`, `wl-copy`, `hyprpicker`, `ydotool`, `wpctl`, `wf-recorder`, `slurp`; `net.hadess.PowerProfiles` | notices |
  | Panels | `kdialog` (wallpaper selector), `ydotool` (on-screen keyboard) | notices |
  | Search, Prefixes | `qalc`, `cliphist` | notices |
  | Apps, Commands | the program of each command | one notice, updated as the commands change |
  | Users | `org.freedesktop.Accounts` | notice |
- **A busy section** (`ContentSection { busy: … }`) adds an 18 px
  `MaterialLoadingIndicator` 4 px after its title while busy.
- **`ContentPlaceholder`**: a whole-page state 220 px tall, with a
  `PagePlaceholder` centered in it: a `Clover4Leaf` in `colSecondaryContainer`
  around a 56 px symbol with 12 px to spare, then 5 px apart the title in the
  title family at 19 px and the description at 15 px, both `m3outline` and
  centered.
- **`ConfigSpinBox`**: a row 8 px in from both sides: symbol, label, and a
  spin box 35 px tall on `colLayer2` with radius 12. Its − and + ends are
  35 px squares, radius 2 on the inner side, tinted `colLayer2Hover` and
  `colLayer2Active`; holding one repeats after 300 ms, every 100 ms. The value
  in the middle, at least 40 px wide, can be typed.
- **`StyledComboBox`**: the sidebar's pill, with an optional leading symbol,
  at 40 % opacity when disabled. Its implicit width is the symbol, 8 px and the
  text plus the chevron; the 16 px on each side and the 8 px before the
  chevron are not in it.
  The list opens 4 px below the box, or 4 px above it when there is no room
  below, and is built when it opens.
- **`ConfigSelectionArray`**: `SelectionGroupButton`s in a `Flow` that wraps,
  2 px apart both ways. Each is 12 px in from the sides and 8 px from top and
  bottom around an optional 19 px symbol and a 15 px label, 4 px apart; the
  label's box is as tall as "Abc" in the main family at Qt's default font
  size, which is the KDE general font (11 pt, so 19 px, and buttons 35 px
  tall). `colSecondaryContainer`, `colPrimary` when current, with
  `colOnSecondaryContainer` or `colOnPrimary` text. The current button and the
  first and last of each line are round on the outer side; the other sides
  keep a 6 px radius, changing over 200 ms (expressive effects). A press
  selects; there is no bounce.
- **`ConfigSpinBox` in other units**: Pomodoro shows minutes and stores
  seconds, so the value is multiplied back and written as a whole number.
- **`MaterialTextField` / `MaterialTextArea`**: Qt's Material field, 56 px
  tall and growing when the text wraps. Its label rests in `m3outline`,
  centered on the field with the 9 px left free above it for the floating
  label, so 4.5 px above the frame's middle, and floats up to 80 % size, over
  150 ms, once the field has text or focus; it turns `m3primary` with focus.
  A password field (`echoMode: Password`) masks its text on one line.
  Outlined, the frame is 1 px
  in Qt's hint color (white at 30 % in the dark scheme, black at 38 % in the
  light one) with a gap behind the floating label, 2 px `m3primary` with
  focus, and the text sits 15 px in. Filled, the field is `m3surface` with a
  4 px top radius and a 1 px bottom line in `m3outlineVariant`, `m3outline`
  on hover and `m3primary` with focus. The value is written back on Enter or
  when focus leaves. A disabled field's text takes Qt's hint color; an
  outlined field keeps its label color, a filled one fades it.
- **Link rows** (`RippleButtonWithIcon` in the pages): 56 px, radius 12,
  `colLayer2`, a symbol, a title and a 12 px `colSubtext` line, and a chevron;
  a press opens a subpage.
- **`RippleButtonWithIcon`** on its own: 35 px, radius 12, `colLayer2`, 10 px
  in from both sides, a filled 19 px symbol and a 15 px label 5 px apart, both
  `colOnSecondaryContainer`, as wide as that content.
- **Tooltips** appear at once, above the row, whenever it is hovered.

## 5. Notifications

One untitled section: "Do not disturb" (the notification service's own
switch, which keeps notifications but stops their popups), "Stays on screen
for (ms)" (`notifications.timeout`, 1000 to 60000 in steps of 1000, for
notifications that name no time), and a "Placement" subsection with "Always
on one display" (`notifications.forceMonitor.enable`) and a box of the
monitors, "model (name)", enabled with it, writing
`notifications.forceMonitor.name`. Then "On-screen display" with its own
"Stays on screen for (ms)" (`osd.timeout`, 100 to 3000 in steps of 100).

## 6. Search

One untitled section: a switch for `search.sloppy`, which ranks apps,
clipboard entries and emojis by Levenshtein distance instead of fuzzy
matching, keeping what scores above 0.2; the non-app result delay
(`search.nonAppResultDelay`, 0 to 500 ms in steps of 10); and a "Prefixes"
subsection with "Show default actions without a prefix" and filled fields for
the six prefixes, four in the first row and two in the second.

## 7. Privacy & Security

- **System**: link rows to the Screen Lock subpage and to Screenshots &
  Recording.
- **Devices**: two 52 px tiles, "Microphone in use" / "idle" and "Screen being
  shared" / "not shared", their symbol turning `colError` while active. They
  are read from `pw-dump` every 2 s: a link from a `Video/Source` node means
  the screen is shared, one from an `Audio/Source` into a
  `Stream/Input/Audio` means the microphone is in use.
- **Work safety**: switches for hiding clipboard images and wallpapers, and a
  "Trigger keywords" subsection of outlined fields for network names, file
  names and links, shown joined with ", " and saved split at the commas. The
  wallpaper hides, under a `colLayer0` veil mixed 75 % toward `colPrimary`,
  while the switch is on, the wallpaper's path holds a file keyword and the
  network's name holds a network keyword.

## 8. Screenshots & Recording (subpage)

"Save paths": "Also save to a file" (`screenSnip.save`) and the screenshot
folder, enabled with it; the recordings folder, defaulting to the XDG videos
folder. "Region selector": the pointer switch, the hint targets (windows,
layers, labels), hint opacity in percent, selection padding, aim lines for
rectangles, and the circle's stroke width and padding.

## 9. System

Link rows only: Region & Language, Date & Time, Users (its second line the
account's real name, or its username, from AccountsService) and About, then
"The shell itself" with Services and Advanced.

## 10. Date & Time (subpage)

"Time Format": 24h (`hh:mm`), 12h am/pm (`h:mm ap`) and 12h AM/PM
(`h:mm AP`) for `time.format`. Choosing also rewrites the hyprlock clock in
`~/.config/hypr/hyprlock.conf`, the first `TIME12` on a line to `TIME` for 24h
and the first whole `TIME` to `TIME12` otherwise, as the QML's `sed` does.
"Clock & Calendar": the seconds switch (`time.secondPrecision`) and outlined
fields for the date, short date and date-with-year formats. "Pomodoro": focus,
break and long break in minutes (stored in seconds) and the cycles before a
long break, two to a row.

## 11. Users (subpage)

One "Account" section. A header 10 px from above and below, centered: an
80 px `colLayer2` circle holding the AccountsService icon, cropped to its
centered square, or a 40 px `person` in `colSubtext`; 20 px to its right the
display name at 22 px over "user · Administrator" or "user · Standard" in
`colSubtext`. Then outlined fields for the real name (its label is the
username) and the email address, written through AccountsService's `SetRealName`
and `SetEmail` when editing finishes, after which the header rereads the
account. "Change password…" swaps itself for a form, 8 px apart: current, new
and repeated password, an `m3error` line ("The new passwords do not match",
"The current password is not correct" for an authentication failure, or
`passwd`'s last complaint), and Cancel and "Change password" on the right,
the latter live only when all three are filled and match. It feeds `passwd`
the three answers under the C locale; success closes and clears the form.

## 12. About (subpage)

- **Device**: label and value rows, the label 140 px wide and 8 px in, both
  15 px, the value `colOnLayer0` and cut with an ellipsis, a row hidden while
  its value is empty: the host name and kernel from `/proc/sys/kernel`, the
  first `model name` in `/proc/cpuinfo`, `MemTotal` in GiB to one decimal,
  the display controllers `lspci -mm` lists (vendor and device, shortened to
  their bracketed alias or without "Corporation", "Inc." and the like), and
  `XDG_CURRENT_DESKTOP` with Wayland or X11.
- **Storage**, hidden without disks: one card per `/dev/` device `df` lists
  (squashfs left out), two to a row 8 px apart, `colLayer2` with radius 17 and
  12 px inside: a 23 px `hard_drive`, the mount point cut in the middle and
  the percentage used at 12 px; a progress bar 2 px clear above and below;
  "free of" at 12 px and "device · filesystem" at 10 px, all `colSubtext` but
  the mount.
- **Distro** and **Dotfiles**: a banner 10 px from above and below, centered
  on its fractional width like the Qt layout, of an 80 px theme icon (the
  `os-release` `LOGO`, else the distro family's symbolic icon; the dots'
  `illogical-impulse`) and 20 px to its right the name at 22 px over its
  links at 16 px, the fork line at 12 px. Links take KDE's `ForegroundLink`
  color, without underline, and open in the default handler through GIO
  (links and buttons alike). Under each banner a
  wrapping row of `RippleButtonWithIcon`s 5 px apart for the distro's
  documentation, support, bug and privacy pages, and for the dots'
  documentation, issues, discussions and sponsorship.

## 13. Services (subpage)

- **Resources**: the polling interval (100–10000 ms, by 100). qs's history
  length is left out; neither shell keeps such a history.
- **Conflict killer**: a switch for killing notification daemons without
  asking, with a tooltip. At start `src/panels/conflicts.rs` looks for mako
  and dunst; with the switch on it kills them, otherwise it asks in a dialog
  (Always, which also turns the switch on, Yes, No), as qs's `killDialog.qml`.
- **System updates (Arch only)**: the update-check switch with a tooltip; a
  notice that `checkupdates` is missing while checks are on and it is not on
  the `PATH`; the check interval (60–1440 min, by 60); and under "Pending
  package thresholds" the advise and strongly-advise counts (1–1000 and
  1–2000, by 25). The interval and both thresholds are disabled while checks
  are off.
- **Weather**: a `ConfigRow` of the GPS switch and the Fahrenheit switch (with
  a tooltip), a filled "City name" field and the polling interval (5–50 min,
  by 5).

## 14. Advanced (subpage)

- **Scrolling**: the faster-touchpad switch, then the mouse and touchpad
  scroll distances (10–1000, by 10) and the mouse detection threshold (1–500,
  by 10) with a tooltip, all under `interactions.scrolling`.
- **Workarounds**: the dead-pixel switch
  (`interactions.deadPixelWorkaround.enable`) and the race condition delay
  (`hacks.arbitraryRaceConditionDelay`, 0–500 ms, by 5), each with a tooltip.
- **Rendering** (proscenio only): Cairo, OpenGL or Vulkan (`renderer`, Cairo
  by default). GTK picks its renderer once: the shell sets `GSK_RENDERER`
  before its first window and clears it once the surfaces are built, so the
  apps it starts do not inherit it. A `GSK_RENDERER` set in the environment
  wins. A notice with a "Restart shell" button shows while the choice differs
  from the renderer in use; the restart opens the settings on this page. Each
  renderer has a tooltip.

  A new choice stays on trial until it is kept. Picking it writes the renderer
  in use to `rendererFallback`. On start, while that is set, the shell spawns
  `proscenio renderer-check` with `GSK_RENDERER=cairo`, so the question draws
  even when the new renderer fails. It asks "Keep the … renderer?" with a
  15-second countdown:
  - Keep clears the fallback (`renderer keep`);
  - Revert, closing the dialog or the timeout restores it and restarts the shell
    (`renderer revert`).

  With no shell to answer the call, the check writes the config itself and
  starts one.
- **Shell usage** (proscenio only): the shell's own CPU (of one core), resident
  memory, GPU and video memory, every 2 s while the page is open
  (`services/shellusage.rs`):
  - CPU and memory come from `/proc/self`;
  - GPU and video memory come from the DRM fdinfo of the shell's GPU clients (the
    busiest engine, and VRAM or local memory, falling back to GTT or system
    memory), which covers amdgpu, i915, xe and the other kernel drivers;
  - the proprietary NVIDIA driver has no fdinfo, so there `nvidia-smi pmon` gives
    the shell's row;
  - a dash means the driver reports nothing.

## 15. Quick

- **Wallpaper & Colors**, first a row 5 px apart: the wallpaper, 340 × 200,
  cropped to cover and faded in over 400 ms (emphasized decelerate) once
  decoded. Its mask is 360 px wide stretched over the 340, so the radius-17
  corners are 16 px across and 17 px tall. Beside it a column 5 px apart: a
  35 px `RippleButtonWithIcon` ("Choose file", the filled `wallpaper` symbol
  and the keys Ctrl, the cheatsheet's Super key, "+" and T, 3 px apart and
  10 px after the label) that runs the wallpaper picker, and under it Light
  and Dark, equal halves filling the rest, `colLayer2` with radius 12 and
  `colPrimary` for the current mode: a 30 px symbol over a 12 px name,
  `colOnPrimary` or `colOnLayer2`. They run `switchwall --mode … --noswitch`
  and follow the generated palette's `$darkmode`.
- A `KeyboardKey` is a 5 px-round cap in `colOnLayer0` 1 px wide and 3 px at
  the bottom around a 4 px-round face in `m3surfaceContainerLow`, the key's
  name at 12 px in the monospace family, 6 px in from the sides and 1 px from
  top and bottom.
- The palette type as selection buttons (Auto and the eight schemes), which
  regenerate the colors with `switchwall --noswitch`; an outlined accent
  color field 8 px in from both sides, trimmed and regenerating the same way
  when finished; the switches for the extra background tint, transparency and
  automatic transparency values; and a uniform row of the background and
  content transparency in percent, the background one live only while
  transparency is on and neither while the values are automatic.
- **Bar & screen**: a `ConfigRow` of "Bar position" (Top, Left, Bottom, Right,
  stored as `bar.bottom` and `bar.vertical`) and "Bar style" (Hug, Float,
  Rect), and a row of "Screen round corner" (No, Yes, When not fullscreen).

## 16. Wi-Fi

- While Wi-Fi is available, an untitled section: a switch that turns the radio
  on or off, and link rows to "Saved Networks" and "Connect to Hidden
  Network…".
- Without an adapter a placeholder, `signal_wifi_off`, "No Wi-Fi Found" and
  what to check; with the radio off another, "Wi-Fi Off".
- While it is on, "Visible Networks", busy while scanning, with "Searching for
  networks…" (8 px in, `colSubtext`) until the list has something, then one
  `WifiNetworkItem` per name: 56 px, or 166 px while it asks for a password,
  with Disconnect on the joined network and Forget on saved ones. The page
  rescans every 15 s while the radio is on, starting as it opens.
- A network item is the sidebar dialog's, over the same service
  (`src/panels/wifinetwork.rs`, `src/services/wifi.rs`); see
  [sidebar-right.md](sidebar-right.md) §6 for the password path and clicks.

## 17. Saved Networks (subpage)

- With no Wi-Fi profile, a placeholder: `wifi_off`, "No Saved Networks".
- Otherwise, an untitled section with one card per profile: 56 px,
  `colLayer2` with radius 12, 12 px in on the left and 8 on the right, 10 px
  between `wifi` (the joined one) or `wifi_lock`, the name over a 12 px
  `colSubtext` line ("Connected", "Joins on its own" or "Only when chosen"),
  an "Automatic" switch for `connection.autoconnect` with a tooltip, and a
  Forget button.

## 18. Connect to Hidden Network (subpage)

An untitled section: a `colSubtext` line explaining that a hidden network has
to be named in full; "Network name" with an outlined field; "Password", whose
tooltip says to leave it empty for an open network, with a masked field; a
"Connect" `RippleButtonWithIcon` (`wifi_add`) 4 px lower, live only while a
name is typed and nothing is connecting, reading "Connecting…" meanwhile; and,
once an attempt ends, "Connected" in `colSubtext` or NetworkManager's error in
`colError`, 8 px in and wrapping. Enter in either field connects too, and the
password is cleared after each attempt. The connection goes to NetworkManager
over D-Bus with `hidden` set, like the visible networks' passwords.

## 19. Network

"Wired" (`lan`) and "VPN" (`vpn_key`), each either a `colSubtext` line saying
nothing is set up (8 px in) or one card per NetworkManager connection of that
kind (`802-3-ethernet`; `vpn`, `wireguard`, `tun`, `ip-tunnel`), sorted by
name: 52 px, `colLayer2` with radius 12, 12 px in on the left and 8 on the
right, the name over "Connected · device" or "Not connected" at 12 px in
`colSubtext`, and a `StyledSwitch` that brings the connection up or down and
then shows what NetworkManager reports. Under the VPNs, 4 px lower, "Set up
connections" runs `apps.network` (by default KDE's network module), with a
tooltip. The list follows NetworkManager's changes, 500 ms after the last one.

## 20. Bluetooth

- While an adapter exists, an untitled section with the power switch; without
  one a placeholder (`bluetooth_disabled`, "No Bluetooth Found"), and with it
  off another ("Bluetooth Turned Off").
- While it is on, "Devices", busy while discovering, with "Searching for
  devices…" (8 px in) until something turns up, then a card per device:
  connected ones, then paired, then the rest, each group by name with bare
  addresses last. A card is the network card's shape (56 px): `bluetooth` or,
  for a device that names its kind, `bluetooth_connected`; the name (or the
  address) over "Pairing…", "Connecting…", "Connected", "Could not connect.
  Wake the device and try again" in `colError`, "Paired" or "Not set up"; a
  Forget button on paired devices; and Pair, Connect or Disconnect, dead while
  pairing or connecting.
- The page keeps discovery running while nothing is being set up, restarting
  it every second if it stops, and ends it on leaving.
- Pairing holds a `bluetoothctl --agent NoInputNoOutput` for its length, stops
  discovery first, pairs, trusts the device, waits 5 s for the bonding link to
  drop and then tries to connect up to four times, 1.5 s apart; it gives up
  after 90 s. Connecting stops discovery too and gives up after 15 s.

## 21. Displays

- With more than one display, `MonitorArrangement`: a 240 px `colLayer2`
  field with radius 12 where every enabled display is a plate scaled to fit
  90 % of it, radius 8, `colSecondaryContainer` for the chosen one and
  `colLayer3` otherwise, a 1 px `colOutlineVariant` border (2 px `colPrimary`
  while dragged) and its model or name in the middle. A tap chooses it; a drag
  past 10 px moves it, snapping within 10 px to the neighbors' edges and
  pushed out to the nearest side of any display it lands on, and the new
  spots are written with the main display at 0 × 0.
- The displays as selection buttons ("model (name)"), disabled ones included,
  with "Rescan displays" (a renderer reload, with a tooltip) at the end of the
  same row, and under subsections: "Use as" (main, extended, a mirror of
  another, or "Off", offered while another display stays on or when this one
  is off; "Off" writes `disabled = true` alone, hands the main display to an
  enabled one, and picking anything else writes `disabled = false` first),
  "Resolution" (the native mode scaled by
  1, 1.25, 4⁄3, 1.5, 1.6 and 2 wherever that gives whole pixels, plus the panel
  modes at the running size, or every panel mode with "Show all
  resolutions"), "Refresh rate", "Rotation" and "Variable refresh rate".
- "Color": the color profile (with `hdr` ones forcing 10-bit), bit depth,
  forced wide color and HDR (with a warning tooltip), the SDR transfer
  function and an ICC profile from the usual color directories.
- "Luminance": SDR brightness, saturation and minimum luminance in hundredths,
  SDR maximum luminance, and under "Display" the display's own minimum,
  maximum and maximum average luminance.
- "Reserved area": top, right, bottom and left, 0–2000.
- "All displays": Auto HDR (with a tooltip; `render:cm_auto_hdr`), the
  global variable refresh rate that a display set to follow it uses (with a
  tooltip; `misc:vrr`: off, on, fullscreen only, fullscreen games and video)
  and keeping X11 apps sharp on scaled displays (with a tooltip;
  `xwayland:force_zero_scaling`).
- "Night light": the automatic schedule switch, a uniform row of the From and
  To times (dead while the schedule is off), and the color temperature.
- Every change is written to the display's `hl.monitor` block in
  `~/.config/hypr/settings.lua`, starting from what is running, and Hyprland
  reloads. The block writer (`src/platform/monitorrules.rs`) and the option
  writer (`src/platform/hyprconfig.rs`) are Rust ports of the shell's
  `hypr-monitor.py` and `hypr-config.py` and write the same file byte for
  byte.

## 22. Sound

- "Output" and "Input": the default device under "Device" (a description
  that starts with the device's nick reads "nick · the rest"), a
  `ConfigSlider` for its volume and a Mute switch.
- A `ConfigSlider` is a row 8 px in, 10 px apart: a 19 px symbol, a label
  120 px wide (cut with an ellipsis, where Qt let a long name run into the
  slider) and an extra-small slider (12 px track) from 0 to 100 with a
  percentage tooltip and a stop at 1.
- "Volume Levels": one slider per playing application, `volume_off` while it
  is muted, or "Nothing is playing".
- "Alert Sound": a uniform row of the battery, Pomodoro and microphone
  switches (the last with a tooltip), and the sound theme, one of the themes
  under `/usr/share/sounds` with a `stereo` folder. Alerts play the theme's
  `.oga` (else `.ogg`) file with `paplay`, which comes with `libpulse`, the
  library the shell links against.
- "Sound cards": each card's available profiles, in `pactl`'s order, under
  its description with a tooltip.
- "Earbang protection": the switch, with a tooltip, and a row of the largest
  allowed step and the volume limit, dead while protection is off.

## 23. Power

- "Power Saving" and "Automatic Suspend" each hold an `IdleTimeoutRow`: a
  subsection with a tooltip over a row as wide as its content, the switch
  (on means a timeout; turning it on uses 15 or 45 minutes) and "after (min)",
  1–600 by 5, dead while the switch is off. They read and write the screen
  and suspend listeners of `~/.config/hypr/hypridle.conf` and restart
  hypridle; `src/platform/hypridle.rs` is a port of the shell's
  `hypr-idle.py`.
- "Apps can keep the screen on", under the screen blank row: on while none of
  `ignore_dbus_inhibit`, `ignore_systemd_inhibit` and
  `ignore_wayland_inhibit` in the `general` block is `true`; off writes all
  three as `true`, on removes them.
- Every hypridle control, here, on Screen Lock and in the Welcome "Power
  saving" section, exists only when `hypridle` is on `PATH`. Without it the
  "Power Saving" section (the Welcome section, the Screen Lock page) shows a
  notice that hypridle is not installed, and "Automatic Suspend" is left out.
- "Battery": a uniform row of the low and critical warnings, a row of the
  "Automatic suspend" switch (with a tooltip) and the level it suspends "at",
  dead while the switch is off, and the full warning.

## 24. Screen Lock (subpage)

An untitled section: the `IdleTimeoutRow` for the lock listener (30 minutes
when turned on), "Lock before sleep" (on while the hypridle `general` block
has a `before_sleep_cmd` that locks; on writes `loginctl lock-session`, off
removes the key), switches for using Hyprlock and for locking on startup;
"Security", with requiring the password to power off and unlocking the
keyring, both with tooltips; "Style: general", with the centered clock, the
"Locked" text and varying password shapes; and "Style: Blurred", with the blur
switch and the extra wallpaper zoom (percent) and blur radius, dead while
blur is off.

## 25. Multitasking

- Hyprland options are read from the running compositor (`getoption`) and
  written one line each into `settings.lua`, then Hyprland reloads
  (`src/services/hyproptions.rs`, `src/panels/settings/hyprrows.rs` for the
  `HyprlandSwitch` and the option spin boxes).
- "Tiling": the layout (Dwindle, Master, Scrolling or Monocle); "Spacing"
  (inner and outer gaps, border width, and "Smart gaps" with a tooltip, on
  while `settings.lua` holds the four lines of Hyprland's example: workspace
  rules for `w[tv1]` and `f[1]` with no inner or outer gaps, and the window
  rules `no-gaps-wtv1` and `no-gaps-f1` taking border and rounding off tiled
  windows there; the switch adds or removes the four together and reloads,
  `src/platform/hyprconfig.rs`); "Dwindle", "Master" or "Scrolling",
  whichever layout is in use, with their switches, the master's new-window
  place and side, the split or master share in percent, and for Scrolling the
  column width in percent (`scrolling:column_width`, 10–100), the direction new
  windows open in, how a focused column is brought into view (center or fit),
  scrolling to the focused window and a single column filling the screen;
  "Snapping" (with a tooltip): the switch and a uniform row of the window and
  screen gaps, dead while snapping is off; "Resizing": dragging borders to
  resize (`general:resize_on_border`) and the grab area around them in px,
  dead while dragging is off.
- "Focus": what the pointer does to focus (`input:follow_mouse`: follows the
  pointer 1, click to focus 0, click to focus with hover and scroll still going
  to the window under the pointer 2, never, not even on a click 3), where focus
  goes after a window closes (`input:focus_on_close`: next window, window under
  the pointer, window used last), and letting apps take focus when they ask
  (`misc:focus_on_activate`).
- "Overview": the enable switch; "Looks": centered icons and the scale in
  percent; "Workspace grid": rows and columns, and the horizontal and vertical
  order as two selection arrays side by side. The orders are booleans in the
  config; the QML compares them loosely with 0 and 1, proscenio reads and
  stores them as booleans.
- "Workspaces": five switches for going back and forth, wrapping around, its
  animation and the special workspace; "Swiping between workspaces" (with a
  tooltip): the full swipe, the give-up ratio, the flick speed, the direction
  lock (and after how far, dead while the lock is off), making a new workspace
  and swiping on; "Distance between workspaces" (with a tooltip): the gap.

## 26. Appearance

- "Desktop": link rows to the Background, Bar and Panels subpages.
- "Theme": the GTK theme, the Qt style and the icon theme (with a tooltip).
  They come from `src/platform/appearance.rs`, the port of
  `appearance.py`, through `src/services/appearance.rs`: every toolkit keeps
  its own copy (gsettings, `gtk-3.0` and `gtk-4.0` `settings.ini`,
  `kdeglobals`, `~/.icons/default`, and `hl.env` lines in `settings.lua`), and
  each writer edits one key and leaves the rest of the file alone. The files
  match the script's output byte for byte. Reads and writes run off
  the main thread, one write at a time, and each write rereads everything.
- "Color generation": the three theming switches (two with tooltips) and
  "Terminal colors" (with a tooltip): forced dark mode, harmony and foreground
  boost in percent, and the harmonize threshold.
- "Fonts": "Apps & panels" (with a tooltip) is one row per role, a 110 px
  label, the family and the size (5 to 72); General, Fixed width and Titles
  also set the shell's own main, reading, monospace and title families. A
  family that fontconfig does not list is shown first. "Adjust all" (with a
  tooltip) has switches for the family and the size, a family and a size
  that start from General's, each dead while its switch is off, and "Apply to
  all fonts", which leaves Fixed width out. "Panels only" (with a tooltip):
  the Nerd icons and Expressive families, stored in the config alone.
- "Pointer": the cursor theme (with a tooltip), which also runs
  `hyprctl setcursor` and is kept for the next start.
- "Windows": corner rounding and shape (the shape tenfold with one decimal,
  with a tooltip); blur with its radius and passes, dead while blur is off,
  and X-ray (with a tooltip); the focused and other windows' opacity in
  percent and "Keep fullscreen windows opaque" (with a tooltip); "Shadows":
  drop shadows (`decoration:shadow:enabled`) with their size in px, falloff
  (`render_power`, 1–4) and a sharp edge, dead while shadows are off;
  "Dimming": dimming windows out of focus, "Keep fullscreen windows undimmed"
  (with a tooltip) and the strength in percent, both dead while dimming is
  off, and the dimming around the special workspace in percent; allow tearing
  (with a tooltip).
- The two fullscreen switches each own one `hl.window_rule` line in
  `settings.lua` matching `fullscreen = true`, which Hyprland also sets for
  maximized windows: `no-dim-fullscreen` with `no_dim = true`, and
  `opaque-fullscreen` with `opacity = "1 override 1 override"`. A true
  fullscreen window takes `decoration:fullscreen_opacity` whether focused or
  not, so the opacity rule is what keeps maximized ones opaque. Each switch is
  on while its line is present, adds or removes it and reloads Hyprland
  (`hyprrows::lines_switch`, the same mechanism as Smart gaps).
- "Shell windows": the title bar switch (with a tooltip) and centering the
  title, dead while the title bar is off.

## 27. Background (subpage)

- "Wallpaper": hiding it under a fullscreen window (with a tooltip).
- "Parallax": two uniform rows of switches (vertical, vertical for tall
  wallpapers with a tooltip; following the workspace and the sidebars), the
  preferred zoom and the widget movement (with a tooltip), both in percent.
- "Widget: Clock": the enable switch at its own width, a spacer and the
  placement (Draggable or Random) at the right; showing it only when locked;
  a row of the clock style, hidden while the clock shows only when locked, and
  the locked clock style at its own width. "Digital clock settings" (with a
  tooltip) shows while either style is Digital: two uniform rows of switches,
  "Font family" with a family box, and four `ConfigSlider`s (weight, size,
  width, roundness) whose tooltips show the plain value and whose stops mark
  the defaults. The cookie's settings, dial, hands and date style show while
  either style is Cookie; hour marks are dead unless the dial is Dots or Full,
  the digits unless it is not Numbers. "Quote": the switch and the text.
- "Widget: Weather": the enable switch and the placement, as for the clock.
- **`ConfigSlider`**: 8 px in from both sides, the symbol, a 120 px label and
  the extra-small slider, 10 px apart.
- A row lays out a label at its fractional text width, as Qt does, so what
  follows a 75.4 px label starts at 79 and not 80.

## 28. Bar (subpage)

- "Notifications": the unread count switch.
- "Positioning": a row of the bar position (shared with Quick) and
  "Automatically hide" (No or Yes) at its own width; "Holding Super" (with a
  tooltip): revealing the bar and the numbers (with a tooltip) and the hold
  delay, dead while that is off; pushing windows away (with a tooltip), dead
  unless the bar hides; the hover region (with a tooltip); a row of the corner
  style (shared with Quick) and the group style at its own width.
- "Appearance": the background, the floating shadow (with a tooltip, dead
  unless the background shows and the corners float) and verbose (with a
  tooltip); "Monitors" (with a tooltip): a switch per monitor, `model (name)`,
  on while the bar's list names it or is empty. Turning off the last one
  stores nothing, and the switch springs back on.
- "Resources": the memory, swap and CPU warning thresholds in steps of 5.
- "Tray": four switches (two with tooltips) and the pinned or unpinned item
  IDs, whichever the first switch makes the list mean, as an outlined field
  split at commas.
- "Utility buttons": four uniform rows of two switches (System updates with a
  tooltip). "Weather": the enable switch.
- "Workspaces": four switches (the Nerd Font one with a tooltip), how many
  are shown, and the number style (Normal, Han characters, Roman numerals),
  which stores the list of labels itself.

## 29. Panels (subpage)

- "Dock": enable; "Reveal": hover to reveal and pinned on startup side by
  side, and the hover region height, dead unless hovering reveals; "Looks":
  tinted icons and the height; the pinned apps and ignored app patterns (both
  with tooltips) as outlined fields split at commas.
- "Sidebars": "Quick toggles" (with a tooltip): Classic or Android and the Android columns, dead
  for Classic; "Sliders": enable and the brightness, volume and microphone
  sliders, dead while sliders are off; "Corner open" (with a tooltip): enable,
  hover to trigger (with a tooltip), then a plain row, not a layout, of the
  forced corner end (no symbol) and its vertical offset, both at their own
  widths and dead unless the corners open by clicking; bottom placement and
  value scroll side by side, visualizing the region, and the region's width
  and height, all dead while corner open is off.
- "Wallpaper selector": the system file picker;
  "On-screen keyboard": pinned on startup and the layout (English (US),
  German, Russian — the names of the keyboard's layouts).
- "Cheat sheet": the Super key symbol as a selection of Nerd Font glyphs;
  three symbol switches whose symbols are Nerd Font glyphs, with tooltips;
  split keycaps (with a tooltip) and the key and description font sizes side
  by side.
- A text's width is the larger of its advance and its ink, as Qt measures it,
  so a Nerd Font glyph that draws past its advance takes the room it draws in.
- A text field's text takes the same optical size as every other main-font
  text, and the field measures its lines with it, so a field grows exactly
  when its text wraps.
- Keeping the right sidebar loaded and the launcher's pinned apps are left
  out: the sidebar is always built, and the launcher has no pinned apps.
- Known difference: where Qt lays out whole nested rows at fractional widths
  (the forced corner row, the region row), items can land 1 px off.

## 30. Apps

- "Default Apps": a subsection per kind of file or link (web, mail, calendar,
  music, video, photos, text, files), each a box of the entries that declare
  the kind's type, named as the entries name themselves, on the one in use,
  led by "Not set" when none is, and ending in "Other…". "Other…" puts the box
  back on its choice and opens a `WindowDialog` over the settings window
  (`Context::dialog_presenter`, an overlay around the window's content), the
  way the sidebar opens its Wi-Fi and Bluetooth dialogs: the prompt Plasma
  uses for the kind ("Select default text editor"), a search field that has
  focus, the shown applications GIO lists, sorted by name, with their icons,
  filtered by name, desktop id and executable ("No matches" when nothing is
  left), and "Cancel". Picking one sets it for the kind and closes the
  dialog. `src/platform/defaultapps.rs` is the port of `default-apps.py`: it
  reads the association files the way the mime-apps spec lays them out,
  follows a kind's parent kinds, and shows the entry set for the kind's main
  type. A choice is authoritative over every type the kind owns, whether or
  not the entry declares it: first the list Plasma's component chooser uses
  for that kind (web adds `text/html` and `application/xhtml+xml`), then every
  type in shared-mime-info's `mime/types` that no other kind lists, by group:
  `image/*` for photos, `audio/*` for music, `video/*` for video, and for text
  `text/*` and any type descending from `text/plain` (JSON, YAML, XML,
  scripts). Each owned type gets the entry as its only default and first in
  its added associations in `~/.config/mimeapps.list`, written once, and in
  the same pass every default and added entry naming a desktop file that is
  not installed is dropped, a type left with none losing its line. Other
  lines and the removed associations stay as they were. It reads off the main
  thread, again after a choice, and a second after the installed applications
  change.
- "Window rules": the rules the settings app owns, one `hl.window_rule` line
  each in `settings.lua` (`src/platform/windowrules.rs`, the port of
  `hypr-rules.py`, byte for byte), as 48 px `colLayer2` cards of the class,
  what the rule does in `colSubtext` at 12 px and a 32 px round remove button
  with a tooltip, or "No rules yet". "Add a rule" (with a tooltip): the window
  class, a box of the classes of the windows open now that fills it in, the
  kind of rule, its value for opacity (percent, 1 to 100, 100 when it is not
  a number) or a workspace, and "Add rule", dead until there is a class.
  Adding or removing reloads Hyprland.
- "Commands": outlined fields for the terminal, the task manager, the network
  editor and the system update command.
- The popup of a box with few items is as tall as they are; it scrolls only
  past 300 px.

## 31. Mouse & Touchpad

- Hyprland options as on Multitasking, through `hyprrows` (switches, spin
  boxes, the scroll method box, selections, and switches whose on and off
  are other values).
- "General": the primary button (with a tooltip), left or right.
- "Mouse": the pointer speed slider (−100 to 100, the value in its tooltip),
  acceleration (with a tooltip), natural scrolling (with a tooltip), the
  scroll method and the scroll amount in percent.
- "This mouse only": "No pointing device is connected", or "Device" (with a
  tooltip): the mouse, its own speed, whether it is enabled (with a tooltip;
  `enabled = false` in its `hl.device` line), acceleration and natural
  scrolling, each showing the general value until the mouse has its own, and
  "Follow the general settings", dead until it has one. Per-device settings are one
  `hl.device` line each in `settings.lua` (`src/platform/devicesettings.rs`,
  the port of `hypr-device.py`, byte for byte, through
  `src/services/deviceoptions.rs`), batched for 50 ms into one write, then
  Hyprland reloads.
- "Pointer": hiding it when still (with a tooltip) and while typing; who
  draws it (with a tooltip; `cursor:no_hardware_cursors`: the screen except
  while tearing 2, always the screen 0, never 1) and hyprcursor themes.
- "Touchpad": disable while typing; "Clicking": tap to click (with a
  tooltip), tap and drag, three-finger middle click, and nested "Secondary
  click" and "Tap with two or three fingers" selections; "Scrolling": natural
  scrolling and the scroll amount.
- "Gestures" (with a tooltip): the gestures in effect, as 48 px cards with the
  fingers and direction, the action (a Lua function reads "Custom action") and
  a round remove button (`src/panels/settings/gestures.rs`). Hyprland has no
  request that lists gestures, so `src/platform/gestures.rs` reads the
  `hl.gesture` calls of `~/.config/hypr/hyprland/general.lua` as the defaults,
  then applies `settings.lua` in order: an `action = "unset"` line takes the
  default with the same fingers, direction and modifiers away, any other line
  adds a gesture. The `custom` files are not read. Removing a gesture from
  `settings.lua` deletes its line; removing a default appends an unset line.
- "Add a gesture": combos for 3–5 fingers, the direction (swipe any way, left
  or right, up or down, each of the four, pinch, pinch in, pinch out) and the
  action (switch workspace, move, resize, close, toggle floating, toggle
  fullscreen, toggle the special workspace, scroll the layout), and "Add
  gesture", which appends a one-line `hl.gesture` to `settings.lua`, or
  removes the unset line when it brings back a default. A gesture that one in
  effect would shadow, by Hyprland's own rule (same fingers and modifiers, and
  the same direction, its axis, or a swipe over a horizontal or vertical one),
  is refused with a message naming that gesture. After each write Hyprland
  reloads; a new `hl.gesture` config error puts the file back, reloads again
  and shows the error.

## 32. Keyboard

- "Input Sources": the layouts in `input:kb_layout` and `input:kb_variant`,
  as 44 px `colLayer2` cards: the place in the cycle, the layout's name (from
  the xkb registry, or its code), its code and variant, and round move up,
  move down and remove buttons with tooltips, dead at the ends and for the
  last layout. "Add input source" becomes "Cancel" and opens "Add an input
  source": a search field and a 260 px list of every layout and variant,
  filtered by all the words typed, which adds the one clicked and closes.
  The list is a GTK `ListView`, so only the rows on screen exist; each is a
  34 px ripple row with Qt's 8 px button padding.
- "Input Source Switching": the layout switching key (the xkb `grp` options,
  or only the shell shortcut) and Num Lock at start; "Special Character
  Entry": the third-level key and the Compose key; "Modifier Keys": the xkb
  `caps`, `ctrl` and `altwin` options (Caps Lock, Ctrl, Alt and Super), each
  with "Default" for none. Every one of these boxes replaces its own entry in
  `input:kb_options` and keeps the rest.
- "Keyboard Shortcuts": shortcuts following the symbol (with a tooltip), a
  search field, and Hyprland's described binds grouped by the category before
  the colon, each a label and its keycaps.
- `src/platform/xkbregistry.rs` reads `evdev.xml` as `xkb-layouts.py` does,
  with a small reader for that file rather than an XML library; its catalog
  matches the script's entry for entry.
- Known difference: keycaps are whole-pixel widgets, so a row of them can
  drift 1 to 2 px from Qt's fractional placement.

## 33. Devices

proscenio's own page; the QML shell has none.

- "Input devices": a notice on extra devices, then one subsection per piece
  of hardware with a switch row per device Hyprland reports (`hyprctl
  devices`: keyboards, mice, tablets, touch, switches), each with its kind's
  icon and Hyprland's name; the main keyboard's row adds "main keyboard" and
  its active keymap.
- `src/platform/inputdevices.rs` groups the devices: a Hyprland name is the
  kernel name from `/proc/bus/input/devices` in lower case with spaces as
  dashes, a duplicate getting `-N`; devices whose `Phys` match up to
  `/inputN` are one piece of hardware, and devices without `Phys` group by the
  first word of their name. A group is titled with the words its members'
  kernel names share, a doubled leading word once. Hardware comes first, then
  virtual devices, each by title.
- The switch writes `enabled = true` or `false` in the device's `hl.device`
  line through `src/services/deviceoptions.rs`. `true` is written, not the
  line removed: Hyprland keeps a device off when its `enabled = false` line
  goes away.
- The list is read every 2 s while the page is open and rebuilt only when a
  device, the main keyboard or a keymap changes.

## 34. Accessibility

- "Seeing": the cursor size (8 to 128 in steps of 4), set with the cursor
  theme through the appearance port; reduced motion (with a tooltip), which
  is Hyprland's animations turned off; animating manual resizes and dragged
  windows.
- "Typing": "Repeat keys" (with a tooltip), the delay and the rate side by
  side.
- "Zoom": "Magnifier" (with a tooltip), the magnification in percent and
  keeping the magnified image sharp.

---

**Status (proscenio).** The frame lives in `src/panels/settings/`: `mod.rs`
the window, `rail.rs` the navigation rail, `pages/mod.rs` the page list,
`index.rs` the search, `content.rs` the page, sections and bound rows, with
the spin box in `src/ui/widgets/spinbox.rs` and the text field in
`src/ui/widgets/textfield.rs`. Every page and subpage is ported except Region
& Language, which shows a placeholder: proscenio has no translations. The
window, the rail, the tabs, the title bar and the pages, text fields' labels
and values included, match qs pixel for pixel apart from the differences
named in their sections. No page runs Python: each script the QML pages use
has a Rust port under `src/platform/`.

The search index is built at compile time: `build.rs` reads every
`pages/*.rs` through `scan.rs` and lists the titles of sections,
subsections and controls, each with the sections it sits in. A control is a
call that passes a symbol name followed by a capitalized title, as a pair of
arguments or a tuple; `IdleTimeout`-style struct literals add their `title`
and `*_text` fields. A new row is found by the search without being listed
anywhere.

Every row reads its setting from `config.toml`, writes it back there, and
follows the file when it changes elsewhere. The notification timeout, the
popup monitor, the OSD timeout, the search prefixes and mode, the region
selector's options, the work-safety keywords, the bar clock's formats and
precision and the Pomodoro durations apply at once
(`config::current()` rereads the file when it changes); the rest of the shell
reads its config at start only.

The stylesheet overrides two `adw-gtk3` rules: the 55 % dimming of entry
placeholders, and the outline 8 px inside the scrollbar slider, which on the
4 px slider is a negative-width rectangle that pixman reports as a bug on
every scroll.

As in qs: opening the window when it is already open, from another workspace,
focuses it there; the window is built on open and destroyed on close; an idle
Pomodoro shows a new duration as soon as it is set; the Wi-Fi list is read
with `--rescan no`, a scan runs only when asked for (the page's 15 s timer,
the sidebar dialog, switching the radio on), and a saved profile counts as
connected when NetworkManager reports it active; the `checkupdates` notice
shows only once the program is known to be missing.

Deliberate differences:

- **The title** is "proscenio Settings", which the dots float next to the qs
  one.
- **Focus on the first opening.** Hyprland does not give a new window from
  proscenio the keyboard, so proscenio focuses it once it maps.
- **Morphing.** Collapsing and expanding moves the tabs' labels and tints at
  once; qs animated their anchors.
- **The emoji search** in sloppy mode ranks the whole emoji list. qs's emoji
  service never reads the setting and always uses its fuzzy search.
- **AccountsService** is reached over D-Bus rather than through `busctl`,
  and the account is found by name rather than by `id -u`.
- **Focus after a switch.** Replacing a page drops the keyboard focus it
  held, so GTK does not move focus to the new page's first field; qs never
  focuses a field on its own.
