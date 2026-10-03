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
  A 35 px round close button with a 20 px `close` symbol sits on the right,
  4 px after two more of the same size (proscenio only), both with tooltips:
  "Highlight changed settings" (`ink_highlighter`, filled and `colPrimary`
  while on), which flips `settings.highlightChanged`, and "Reset this page to
  defaults" (`settings_backup_restore`), which asks "Reset this page?" with
  how many of the page's settings differ from their defaults (Settings kept by
  Hyprland or the system are not counted) and, on Reset, stores every such
  setting's default.
- **A row** with 8 px spacing: the navigation rail, 5 px in on every side, and
  the content pane, `m3surfaceContainerLow` with radius
  `windowRounding − 8` (10).

Escape closes an open dialog, otherwise the window. Ctrl+Page Down and
Ctrl+Page Up step through the pages and stop at the ends; Ctrl+Tab and
Ctrl+Shift+Tab wrap around.

Typing a printable character other than a space, without Ctrl, Alt or Super,
while no text field has focus and no dialog is open, expands the rail and
sends the key to its search field, cursor at the end.

## 2. The navigation rail

It is expanded while the window is wider than 900 px; after the first press
of its toggle, the toggle alone sets it. Expanded, it is as wide as its widest tab, 56 + 20 + the
label, and at least 230 (qs keeps it between 150 and 230, which clips longer
translated names); collapsed, 56. The width moves over
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
  then the rest. Titles and trails match in English and in every bundled
  translation, so "мышь" finds "Mouse & Touchpad" whatever the interface
  language, and results show in the interface language. The translations
  are gathered on the first query and kept while the rail lives. A result is 48 px tall with 12 px padding and a 60-result
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
under a header: a 35 px back button and the subpage's name at 19 px. A
subpage opened from another subpage goes back to that one; otherwise back
returns to the rail's page.

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
  | Network, Saved Networks, Connection, Hotspot | `nmcli`, `org.freedesktop.NetworkManager` | the page is only a notice |
  | Network, "Import from a file…" | `kdialog` | button disabled, tooltip names it |
  | Connection, "Generate a new key" | `wg` | button disabled, tooltip names it |
  | Bluetooth | `org.bluez`; `bluetoothctl` | the page is only a notice; notice |
  | Appearance, Color generation | `matugen` | notice |
  | Quick, "Choose file" | `kdialog` | button disabled, tooltip names it |
  | Bar, Utility buttons | `grim`, `magick`, `wl-copy`, `hyprpicker`, `ydotool`, `wpctl`, `wf-recorder`, `slurp`; `net.hadess.PowerProfiles` | notices |
  | Panels | `kdialog` (wallpaper selector), `ydotool` (on-screen keyboard) | notices |
  | Search, Prefixes | `qalc`, `cliphist` | notices |
  | Apps, Commands | the program of each command | one notice, updated as the commands change |
  | Users | `org.freedesktop.Accounts` | notice |
- **Changed settings** (proscenio only): every row bound to `config.toml`
  through the page's helpers (switch, spin boxes, slider, selection, text and
  list fields) is registered with its key and default. While
  `settings.highlightChanged` is on, a row whose stored value differs from its
  default (numbers compared as numbers; a missing key counts as the default)
  takes `colTertiary` at 12 % and a 3 px `colTertiary` bar inside its left
  edge, following the file as it changes. Rows over Hyprland options, system
  services and other files are not registered.
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
`notifications.forceMonitor.name`. Then "Applications" (`apps`): a 52 px card
for every application that has sent a notification, by name, with its icon
and two switches 300 px wide together, "Pop up" and "Keep"; "Apps show up here
once they have sent a notification" while there are none, and a 12 px
`colSubtext` line explaining both switches. Turning "Pop up" off adds the
name to `notifications.quietApps`: its notifications go to the list without a
popup. Turning "Keep" off adds it to `notifications.forgottenApps`: its
notifications count as transient and go once their popup ends, and with
"Pop up" off as well they are not kept at all. The service remembers each
application name it has seen in the state file (`notificationApps`); a new
name adds its card to the open page. Then
"On-screen display" with its own "Stays on screen for (ms)" (`osd.timeout`,
100 to 3000 in steps of 100).

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
account's real name, or its username, from AccountsService), Autostart and
About, then "The shell itself" with Services and Advanced.

### 9.2 Autostart (subpage)

An explanation in `colSubtext`, then "Starts with the session" (`start`): a
56 px card per `.desktop` file in `~/.config/autostart`, by name, with its
icon (or `terminal`), the name over the `Exec` line, a switch and a round
remove button with a tooltip; "Nothing starts with the session yet" while
there are none. Under them, 5 px apart, "Add application…" opens the Apps
page's application dialog and copies the chosen entry's desktop file into the
folder, and "Add command…" asks for a name and a command and writes a
`.desktop` file named after the name. The switch writes `Hidden` (and
`X-GNOME-Autostart-enabled` where the file has it); remove deletes the file.
Errors show in `colError` under the buttons (`src/platform/autostart.rs`).

The shell starts the enabled entries itself, once per session: on start,
unless `$XDG_RUNTIME_DIR/proscenio/autostarted` exists, it creates that file
and launches each entry that is not hidden, whose `OnlyShowIn` and
`NotShowIn` allow `XDG_CURRENT_DESKTOP`, and whose `TryExec` is on `PATH`, as
the launcher launches applications. The runtime directory goes away at
logout, so restarting the shell starts nothing twice. Entries in
`/etc/xdg/autostart` are not read.

### 9.1 Region & Language (subpage)

One untitled section with the subsection "Language", its tooltip "Select the
language for the user interface. "Auto" will use your system's locale.", and a
combo box with the `language` icon: "Auto (System)", then every bundled
catalog as its native name and code, such as "Русский (ru_RU)". Each starts
with a left-to-right mark, so a right-to-left name keeps its code on the
right. Choosing another entry writes `language.ui`, sets the system language
(see [foundations.md](foundations.md) §10) and restarts the shell on this page,
since the interface is translated once at start.

## 10. Date & Time (subpage)

"Date & Time" (`schedule`) comes first, over systemd's `timedated`
(`org.freedesktop.timedate1`, `src/services/timedate.rs`), and follows its
property changes:

- the local date and time, "Friday 2 October 2026, 10:08:27", every second;
- "Set the time automatically" (`SetNTP`), dead when `CanNTP` is false, with
  "Synchronized with a time server" or "Not synchronized yet" under it while
  on;
- a "Time zone" link row whose second line is the zone and its offset ("Asia
  Yekaterinburg · UTC+05:00"); it opens a dialog of every zone `ListTimezones`
  gives, each with its current offset, a search field that keeps the zones
  holding every typed word, and Cancel. Picking one calls `SetTimezone`;
- "Set the time", shown while the time is not set automatically: a date
  (YYYY-MM-DD) and a time (HH:MM or HH:MM:SS) field, filled with the moment
  the page opened, and "Set", which reads them in the current zone and calls
  `SetTime`;
- "Hardware clock keeps local time" (`SetLocalRTC`, with a tooltip about
  sharing the computer with Windows);
- the error of the last call in `colError`.

Each call lets polkit ask for a password and waits for it without a time
limit.

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
`colSubtext`. Under it, centered and 8 px apart, "Choose picture…" (dead
without `kdialog`, with a tooltip) asks `kdialog` for a PNG, JPEG or WebP
picture, which is decoded at the size it needs, cropped to its centered
square at 256 px and saved as `$XDG_RUNTIME_DIR/proscenio/face.png` before
`SetIconFile` copies it (AccountsService refuses files over 1 MB); "Remove
picture" sets an empty icon. A refusal shows in `m3error` under them. Then
outlined fields for the real name (its label is the
username) and the email address, written through AccountsService's `SetRealName`
and `SetEmail` when editing finishes, after which the header rereads the
account. "Change password…" swaps itself for a form, 8 px apart: current, new
and repeated password, an `m3error` line ("The new passwords do not match",
"The current password is not correct" for an authentication failure, or
`passwd`'s last complaint), and Cancel and "Change password" on the right,
the latter live only when all three are filled and match. It feeds `passwd`
the three answers under the C locale; success closes and clears the form.

While AccountsService runs, two more sections follow
(`src/panels/settings/pages/users/others.rs`):

- **Other users** (`group`): the accounts `ListCachedUsers` gives apart from
  the current one, by name, or "Nobody else has an account here". Each is a
  56 px card: `person`, the name over the username (and "password asked at
  first login" while its `PasswordMode` is 1), an "Administrator" switch
  (`SetAccountType`), and round `password` and `person_remove` buttons with
  tooltips. The first opens "Password for NAME": new and repeated password,
  the mismatch in `m3error`, Cancel and "Set password", live once both match;
  the password is hashed with libxcrypt's default method (`crypt_gensalt`,
  `crypt`) and passed to `SetPassword`. The second opens "Delete NAME?" with
  Cancel on the left and "Keep files" and "Delete files" on the right
  (`DeleteUser`). Under the list a line reports the last outcome, in
  `colError` for a failure.
- **Add a user** (`person_add`): full name, user name (filled from the first
  word of the full name, lower case, until it is typed into; up to 32
  lower-case letters, digits, - and _, starting with a letter), "Account
  type" (Standard or Administrator), and a "Password" subsection (with a
  tooltip) of a password and its repeat. "Add user" calls `CreateUser`, then
  `SetPassword`, or with both fields empty `SetPasswordMode(1)` so the
  password is chosen at the first login; it reads "Adding…" meanwhile and
  clears the form on success. What blocks it shows in `m3error` under it.

Every call lets polkit ask for a password and waits for it without a time
limit. Dialogs here and in the connection editor keep 10 px below their
buttons.

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
  on or off, and link rows to "Saved Networks", "Connect to Hidden
  Network…" and "Hotspot".
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
  an "Automatic" switch for `connection.autoconnect` with a tooltip, a 32 px
  round `edit` button (tooltip "Edit") that opens the profile in the
  connection editor (§19.1), and a Forget button.

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
`colSubtext`, a 32 px round `edit` button (tooltip "Edit") that opens the
connection editor, and a `StyledSwitch` that brings the connection up or down
and then shows what NetworkManager reports. The list follows NetworkManager's
changes, 500 ms after the last one.

4 px under the wired list, "Add wired connection" opens the editor on a new
wired profile. Under the VPNs, 4 px lower and 5 px apart, "Add WireGuard"
opens it on a new WireGuard profile, and "Import from a file…" (with a
tooltip) asks `kdialog` for a `.conf` or `.ovpn` file and imports it with
`nmcli connection import`, as WireGuard or, for `.ovpn`, as OpenVPN, which
needs NetworkManager's OpenVPN plugin; the editor then opens on the imported
connection. A failure shows NetworkManager's message in `colError` under the
buttons.

### 19.1 Connection (subpage)

The editor for one NetworkManager profile, opened from a Network or Saved
Networks card, the add buttons or an import. The header shows the profile's
name. Opened without a profile, as a search result does, it edits the first
active connection, or says there is nothing to edit. A new profile is named
"Wired connection" or "WireGuard", with " 2", " 3" and so on when that name
is taken.

- The profile is read over D-Bus (`GetSettings` on
  `org.freedesktop.NetworkManager.Settings.Connection`), and its secrets with
  `GetSecrets` for each secret-holding setting it has, without asking for
  authorization. A secret that could not be read shows an empty field with
  "Stored and hidden. Type to replace it, or press the eye to show it"; the
  eye then asks again, this time letting polkit ask for a password. With the
  secrets read, the eye shows and hides the text.
- Edits change a draft only. "Save" (live while the draft differs and nothing
  is wrong) writes the whole profile back with `Update2`, to disk, or adds a
  new one with `AddConnection2`; when the draft holds a typed secret but the
  stored ones were never read, the stored ones are read first, letting polkit
  ask, so the others are kept. An active connection is then brought up again
  ("Saved and reconnected"). "Revert" returns to the stored profile, and
  "Delete" asks first and then removes the profile and goes back. Under the
  buttons a line shows "Saving…", the first problem in `colError`, or the
  outcome.
- Every text field checks its value as it is typed and shows what is wrong
  under itself; the IP settings are written as `address-data`, `route-data`
  and `dns` (or `dns-data` for servers that are not plain addresses),
  dropping the older `addresses` and `routes` keys. Parsing and writing live
  in `src/platform/nmprofile.rs`, the D-Bus calls in
  `src/services/nmsettings.rs`.
- **General**: the name, "Connect automatically", "Available to all users"
  (off writes the account into `connection.permissions`, with a tooltip), and
  "Metered connection" (Automatic, Yes, No, with a tooltip).
- **Wired**: the device it is tied to ("Any device" or an Ethernet device), the
  cloned MAC address (an address, or preserve, permanent, random or stable),
  and the MTU, 0 for automatic.
- **Wi-Fi**: the network name, "Hidden network", "Security" (None, WPA & WPA2
  Personal, WPA3 Personal; enterprise and WEP profiles get a notice and keep
  theirs) with the password, the cloned MAC address and the MTU.
- **WireGuard**: the interface name (a new profile takes the first `wgN` no
  profile names), "Keys": the private key (a new profile gets one from `wg genkey`),
  the public key derived from it with `wg pubkey`, selectable, and "Generate
  a new key"; the listen port (0 picks one) and the MTU. **Peers**: one
  subsection per peer with its public key, endpoint (host:port), allowed IPs,
  optional preshared key and keepalive in seconds, and "Remove peer"; "Add
  peer" adds an empty one.
- **VPN** (plugin VPNs): the plugin's name and one field per entry of its
  `vpn.data`.
- **IPv4** and **IPv6**: the method (Automatic, Automatic DHCP only for IPv6,
  Manual, Link-local only, Shared with other computers, Disabled). Manual
  adds the addresses with their prefix lengths and the gateway; every method
  that configures addresses has "DNS" (Automatic DNS for the automatic ones,
  the servers and the search domains), "Routing" (Automatic routes for the
  automatic ones, and "Only for its own network", `never-default`, with a
  tooltip) and "Routes" written like `ip route`; automatic IPv6 adds "Privacy
  extensions" (Default, Off, Prefer the fixed address, Prefer a temporary
  address).
- Structural choices (methods, security, peers, switches) rebuild the form
  and keep the scroll position.

### 19.2 Hotspot (subpage)

An explanation in `colSubtext`, then the network name (the host name until a
`Hotspot` profile exists), the password (8 to 63 characters), the band
(Automatic, 2.4 GHz, 5 GHz) and "Share the connection". Turning it on deletes
the old `Hotspot` profile and runs `nmcli device wifi hotspot` with those
values; turning it off brings `Hotspot` down. The switch follows whether
`Hotspot` is active, and NetworkManager's error shows in `colError`. The
fields start from the stored profile, read with `nmcli -s`.

### 19.3 Proxy (subpage)

The Network page ends with a "Firewall & Proxy" section (`security`) of two
link rows, "Firewall" (`shield`, its second line On, Off or Not installed) and
"Proxy" (`travel_explore`, its second line the current mode); the section
shows even while NetworkManager is stopped. The Proxy subpage has an explanation in `colSubtext` and
"Proxy": Off, Automatic or Manual. Automatic adds "Configuration script" (with
a tooltip: empty finds the proxy with WPAD) and its address; Manual adds the
HTTP, HTTPS and SOCKS proxies, each a host field and a port spin box (0 to
65535) over a uniform row of "Username" and a masked "Password", "Use the
HTTP proxy for HTTPS too", which hides the HTTPS rows, and "Not for these
hosts" (with a tooltip), a list split at commas and spaces. The page fills in
once the usernames and passwords are read.

The source of truth is `org.gnome.system.proxy` for the proxies and the
Secret Service for their usernames and passwords, one item each, with the
attributes `application proscenio`, `proxy` (`http`, `https` or `socks`) and
`field` (`user` or `password`), stored and read with `secret-tool`. A change
waits 800 ms for the next one (or until the page closes), then
`src/platform/proxy.rs` writes it everywhere apps look:

- `org.gnome.system.proxy` and its `http`, `https` and `socks` children; with
  "for HTTPS too" the HTTPS keys take the HTTP values. The HTTP username and
  password also go into `use-authentication`, `authentication-user` and
  `authentication-password`, where GTK apps read them;
- `[Proxy Settings]` in `~/.config/kioslaverc`: `ProxyType` (0 off, 1 manual,
  2 a script, 3 WPAD), `Proxy Config Script`, `httpProxy`, `httpsProxy` and
  `socksProxy` as `scheme://host port`, and `NoProxyFor`; KDE apps ask for
  the password themselves;
- the Secret Service items whose value changed, only those, since a store into
  a missing or locked keyring brings up its prompt;
- `http_proxy`, `https_proxy`, `all_proxy` (`socks5://`) and `no_proxy`, in
  lower and upper case, as URLs with the username and password
  percent-encoded before the host. They are never written to a file: Hyprland
  gets them through `hyprctl eval` of `hl.env` calls, the systemd and D-Bus
  activation environment through `dbus-update-activation-environment
  --systemd`, and the shell its own environment, so what any of them starts
  next has them. Outside Manual they are set empty. On start, while the mode is
  Manual, the shell reads the passwords and sets the variables again, since
  nothing kept them across the session.

A schema that is not installed leaves only a notice.

### 19.4 Firewall (subpage)

The page drives `ufw` (`src/platform/firewall.rs`); without it, only a notice.
It reads without root: `ENABLED` in `/etc/ufw/ufw.conf`,
`DEFAULT_INPUT_POLICY` in `/etc/default/ufw`, and the `### tuple ###` lines of
`/etc/ufw/user.rules` and `user6.rules`. Incoming rules to any address from
any port are shown, the IPv4 and IPv6 copies of a rule once, with the
hex-encoded comment decoded; other tuples are left out.

- "Firewall" (`shield`) turns `ufw` on (`ufw --force enable`, and
  `systemctl enable ufw.service` so it starts with the system) or off.
- "Incoming connections no rule allows" (with a tooltip): Block, Refuse or
  Allow (`ufw default deny|reject|allow incoming`).
- **Rules**: a 48 px card per rule, "Allow 22/tcp" (the application profile,
  or "every port", in place of the port when that is what the rule names)
  over "from anywhere" or "from 10.0.0.0/8" and the comment, and a 32 px
  round remove button with a tooltip; "No rules yet" while there are none.
- **Add a rule**: the action (Allow, Deny, Reject, Limit, with a tooltip on
  what Limit does), the protocol (TCP and UDP, TCP, UDP), the port, a range
  or a list, where the connections come from (empty is anywhere), a comment,
  and "Add rule". A rule needs a port or a source; a range or a list needs TCP
  or UDP; a comment holds no quotes or line breaks. A refused rule shows why;
  an accepted one clears the fields.

Every change runs `pkexec proscenio firewall …` (`enable`, `disable`,
`default POLICY`, or `add`/`delete` with the action, protocol, port, source,
application and comment), which checks its arguments again and runs `ufw`
with them, never through a shell. The page rereads the files afterwards, and
`ufw`'s last error line shows in `colError` under the switch and the form.

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
  `~/.config/hypr/settings/displays.lua`, starting from what is running, and Hyprland
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
- "Alert Sound": two uniform rows of switches, battery and Pomodoro, then
  microphone and USB devices (both with a tooltip; `sounds.devices`, on by
  default), and the sound theme, one of the themes
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
- "Buttons & Lid" (`power_settings_new`), filled once systemd-logind answers:
  "Power button", and while UPower reports a lid (`LidIsPresent`) "Lid
  closed", "Lid closed on the charger" and "Lid closed with an external
  display", each Nothing, Lock, Suspend, Hibernate (only when `CanHibernate`
  says yes) or Power off, plus the value in effect when it is none of these.
  They show `HandlePowerKey`, `HandleLidSwitch`, `HandleLidSwitchExternalPower`
  and `HandleLidSwitchDocked` from logind; a choice runs `pkexec proscenio
  power-settings button KEY ACTION`, which writes the key under `[Login]` in
  `/etc/systemd/logind.conf.d/50-proscenio.conf` and sends logind `SIGHUP` so
  it rereads its configuration, and the row then shows what logind reports.
  Errors show in `colError` under the section.
- "Battery": a uniform row of the low and critical warnings, a row of the
  "Automatic suspend" switch (with a tooltip) and the level it suspends "at",
  dead while the switch is off, and the full warning. Where the first `BAT*`
  under `/sys/class/power_supply` has `charge_control_end_threshold`, "Stop
  charging at (%)" (50 to 100 by 5, with a tooltip): a second after the last
  change it runs `pkexec proscenio power-settings charge-limit BATTERY
  PERCENT`, which writes the threshold and keeps it across boots with
  `/etc/tmpfiles.d/proscenio-charge-limit.conf` (removed again at 100). While
  power-profiles-daemon is on the bus, "Power profile on battery" (Unchanged,
  Power saver, Balanced) and "Power profile on the charger" (Unchanged,
  Balanced, Performance), both with a tooltip, stored as
  `battery.profileOnBattery` and `battery.profileOnCharger`: when the charger
  is plugged in or out, the battery service sets that profile, unless it is
  Unchanged.

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

- What the settings app writes for Hyprland lives in `~/.config/hypr/settings/`,
  one file per area (`src/platform/hyprconfig.rs`, `Area`): `appearance.lua`,
  `displays.lua`, `multitasking.lua`, `keyboard.lua`, `accessibility.lua`,
  `mouse.lua` (options of those pages; also cursor and icon `hl.env` lines and
  the fullscreen rules in `appearance.lua`, monitor blocks and the primary
  monitor in `displays.lua`, Smart gaps in `multitasking.lua`, gestures in
  `mouse.lua`), `devices.lua`, `apps.lua` (window rules), `binds.lua`
  (shortcuts) and `other.lua`. The dots' `hyprland.lua` requires every
  `.lua` file there in name order, after the keybinds and before the other
  `custom` files. At start, before any writer runs, proscenio splits a
  `~/.config/hypr/settings.lua` from before this layout into those files,
  statement by statement (an option goes to the page whose list holds it,
  anything unrecognized to `other.lua`), appends each part to its file and
  renames the old file to `settings.lua.bak`.
- Hyprland options are read from the running compositor (`getoption`) and
  written one line each into the page's file, then Hyprland reloads
  (`src/services/hyproptions.rs`, `src/panels/settings/hyprrows.rs` for the
  `HyprlandSwitch` and the option spin boxes).
- "Tiling": the layout (Dwindle, Master, Scrolling or Monocle); "Spacing"
  (inner and outer gaps, border width, and "Smart gaps" with a tooltip, on
  while `multitasking.lua` holds the four lines of Hyprland's example: workspace
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
  `kdeglobals`, `~/.icons/default`, and `hl.env` lines in `appearance.lua`), and
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
  `appearance.lua` matching `fullscreen = true`, which Hyprland also sets for
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
- "File types" (`description`): a link row, "Every file type", to the File
  types subpage (§30.1).
- "Window rules": the rules the settings app owns, one `hl.window_rule` line
  each in `apps.lua` (`src/platform/windowrules.rs`, the port of
  `hypr-rules.py`, byte for byte), as 48 px `colLayer2` cards of the class,
  what the rule does in `colSubtext` at 12 px and a 32 px round remove button
  with a tooltip, or "No rules yet". "Add a rule" (with a tooltip): the window
  class, a box of the classes of the windows open now that fills it in, the
  kind of rule, its value for opacity (percent, 1 to 100, 100 when it is not
  a number) or a workspace, and "Add rule", dead until there is a class.
  Adding or removing reloads Hyprland.
- "Commands": outlined fields for the terminal, the task manager and the
  system update command.
- The popup of a box with few items is as tall as they are; it scrolls only
  past 300 px.

### 30.1 File types (subpage)

An outlined "Find a file type" field, focused when the page opens, over the
types GIO has registered (`content_types_get_registered`, those with a `/`),
each with its description; every typed word has to appear in the type or the
description. Until something is typed, a `colSubtext` line counts the known
types; then up to 40 matches show as 56 px `colLayer2` rows with radius 12:
the type's icon, the description over the type at 12 px, the application GIO
opens it with (or "Nothing opens it") and a chevron, and a line counts the
matches left out or says that none match.

A row opens "Open DESCRIPTION with": "Recommended" (GIO's recommended
applications for the type) and "Also opens it" (its fallback ones), the one in
use in `colPrimary` with a check; a pick makes it the default for that type
alone (`set_as_default_for_type`, in `~/.config/mimeapps.list`). "Reset"
drops the user's associations for the type (`reset_type_associations`),
"Other application…" opens the Apps page's application dialog for any
installed application, and Cancel closes. The list then shows the new
default.

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
  `hl.device` line each in `devices.lua` (`src/platform/devicesettings.rs`,
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
  then applies `mouse.lua` in order: an `action = "unset"` line takes the
  default with the same fingers, direction and modifiers away, any other line
  adds a gesture. The `custom` files are not read. Removing a gesture from
  `mouse.lua` deletes its line; removing a default appends an unset line.
- "Add a gesture": combos for 3–5 fingers, the direction (swipe any way, left
  or right, up or down, each of the four, pinch, pinch in, pinch out) and the
  action (switch workspace, move, resize, close, toggle floating, toggle
  fullscreen, toggle the special workspace, scroll the layout), and "Add
  gesture", which appends a one-line `hl.gesture` to `mouse.lua`, or
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
  search field, and one link row per category of Hyprland's described binds
  (the text before the colon), Shell, App, Window, Workspace, Media,
  Utilities, Screen, Input and Session first, the row's detail listing its
  shortcuts. Each opens the "Keyboard Shortcuts" subpage (`shortcuts`, parent
  `keyboard`) titled with the category. While the search field holds text,
  the link rows give way to the matching shortcuts, grouped by category and
  editable in place.
- A shortcut is one description: its binds outside submaps, at most two key
  combinations, primary and secondary in the order Hyprland lists them. Its
  row holds the label, a reset button shown only while the settings app has
  changed it, and two key buttons in columns sized together, keycaps or an
  `add` symbol when empty. Rows are sorted by label, numbers by value. Mouse
  binds show "LMB", "RMB" or "MMB" and their buttons are disabled; keypad
  keycodes show "KP 0" to "KP 9", "KP −" and "KP +". With "Highlight changed
  settings" on, a changed row is highlighted.
- A key button opens "Press the new shortcut" with the action's label. While
  it is open the window inhibits the compositor's shortcuts, so Hyprland's
  binds reach it; held modifiers show as keycaps followed by "…", and the
  first other key completes the combination from the unshifted key under
  it. A combination another shortcut uses shows "Used by “%1”, which loses
  it". "Clear" empties the slot, "Set" applies the combination, and taking
  the action's other slot's combination empties that slot.
- Changes go into `~/.config/hypr/settings/binds.lua` as one
  `shortcut("<description>", "<primary>" or false, "<secondary>" or false)`
  line per changed shortcut, including each one that lost a combination to
  it, and Hyprland reloads. The dots' `hyprland/lib/binds.lua` defines
  `shortcut`: it moves the action's own binds, with the fallback binds that
  share their combination, to the given combinations, and keeps the binds
  it takes from another action for that action's own line, so lines apply
  in any order. Reset removes the line.
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
  windows; and "Color filter" (with a tooltip): None, Grayscale, Inverted
  colors, Red–green (deuteranopia), Red–green (protanopia) and Blue–yellow
  (tritanopia), plus "Custom shader" while `decoration:screen_shader` names a
  file of someone else's. A filter writes its fragment shader to
  `~/.local/state/proscenio/shaders/NAME.frag` (`src/platform/colorfilter.rs`)
  and sets `decoration:screen_shader` to it; None clears the option. The three
  color-blindness filters daltonize: they simulate the missing cone in LMS
  space and move the color difference it loses into channels that are still
  seen.
- "Bell" (`notifications_active`): "Play the bell sound" (`misc:bell_sound`,
  `default` or `none`), shown only when Hyprland has that option, and "Flash
  the screen" (`accessibility.flashOnBell`, with a tooltip): on Hyprland's
  `bell` event the shell covers the focused display with a white layer that
  takes no input, at 35 % fading out over 300 ms, one flash at a time
  (`src/panels/bellflash.rs`).
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
