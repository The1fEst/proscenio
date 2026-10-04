---
title: System settings
sidebar_label: System
description: The Notifications, Privacy & Security and System pages, with their subpages for screenshots, language, date and time, users, autostart, the About page, the shell's services and advanced options.
---

# System settings

These settings span three pages of the settings window and their subpages:

| Page | Subpages |
|---|---|
| Notifications | |
| Privacy & Security | Screen Lock, Screenshots & Recording |
| System | Region & Language, Date & Time, Users, Autostart, About, Services, Advanced |

Controls bound to the shell's config write `~/.config/proscenio/config.toml`. Its keys below are
dotted TOML paths, so `notifications.timeout` is the `timeout` key of the `[notifications]` table.
The rest talk to system services over D-Bus or edit files of their own, as each section says.

## Notifications

| Control | Key | Default | Range |
|---|---|---|---|
| Do not disturb | `notifications.silent` | off | |
| Stays on screen for (ms) | `notifications.timeout` | 7000 | 1000–60000, in steps of 1000 |
| Always on one display | `notifications.forceMonitor.enable` | off | |
| The display, under Placement | `notifications.forceMonitor.name` | | a monitor's connector name |
| Stays on screen for (ms), under On-screen display | `osd.timeout` | 1000 | 100–3000, in steps of 100 |

**Do not disturb** is the notification server's own switch: notifications still arrive and are kept
in the sidebar, but none pops up. The time on screen applies to notifications that do not ask for a
time of their own. The display box lists the monitors as "model (connector)", is live only while
**Always on one display** is on, and follows monitors being plugged in and out. See
[Notifications](notifications.md) and the [on-screen display](osd.md).

### Applications

Every application that has sent a notification gets a card with its icon, its name and two
switches. The server remembers each application name it has seen in
`~/.local/state/proscenio/states.json`, under `notificationApps`, and a new name adds its card to the
open page. Until then the section reads "Apps show up here once they have sent a notification".

| Switch | Off adds the app name to | Effect |
|---|---|---|
| Pop up | `notifications.quietApps` | its notifications go to the sidebar without a popup |
| Keep | `notifications.forgottenApps` | its notifications count as transient and go once their popup ends |

With both off, the app's notifications are not kept at all.

## Privacy & Security

**System** holds two link rows: **Screen Lock**, the settings of the [lock screen](lock.md),
described on [Power and screen lock](settings-power.md), and **Screenshots & Recording**.

### Devices

Two tiles show whether the microphone is in use and whether the screen is being shared; a tile's
symbol turns to the error color while it is. While the page is open, they are read from `pw-dump`
every 2 seconds: a PipeWire link out of a `Video/Source` node means the screen is shared, and one
from an `Audio/Source` node into a `Stream/Input/Audio` node means the microphone is in use. Without
`pw-dump` a notice says both always read as unused.

### Work safety

| Control | Key | Default |
|---|---|---|
| Hide clipboard images copied from sussy sources | `workSafety.enable.clipboard` | off |
| Hide sussy/anime wallpapers | `workSafety.enable.wallpaper` | off |
| Network names, under Trigger keywords | `workSafety.triggerCondition.networkNameKeywords` | a built-in list, such as `cafe`, `eduroam`, `guest` and `public` |
| File names | `workSafety.triggerCondition.fileKeywords` | a built-in list |
| Links | `workSafety.triggerCondition.linkKeywords` | a built-in list |

The keyword fields are comma-separated. Work safety only acts on a network whose name holds a
network keyword:

- the wallpaper hides when its switch is on, the wallpaper is an image rather than a video, and its
  path holds a file keyword;
- in the launcher's clipboard results, an image is blurred when its switch is on and the entry just
  before or after it holds a link keyword.

### Screenshots & Recording

Notices name the missing programs: `grim`, `magick`, `wl-copy` and `satty` for screenshots and the
snip actions, `wf-recorder` and `slurp` for recording.

| Subsection | Control | Key | Default | Range |
|---|---|---|---|---|
| Screenshots | Also save to a file | `screenSnip.save` | off | |
| Screenshots | Folder | `screenSnip.savePath` | | live only while saving |
| Screen recordings | Folder | `screenRecord.savePath` | the XDG Videos folder | |
| Region selector | Include the pointer | `regionSelector.showPointer` | off | |
| Hint target regions | Windows | `regionSelector.targetRegions.windows` | on | |
| Hint target regions | Layers | `regionSelector.targetRegions.layers` | off | |
| Hint target regions | Show region labels | `regionSelector.targetRegions.showLabel` | off | |
| Hint target regions | Hint opacity (%) | `regionSelector.targetRegions.opacity` | 30 %, stored as 0.3 | 0–100 %, in steps of 5 |
| Hint target regions | Selection padding | `regionSelector.targetRegions.selectionPadding` | 5 | 0–50 |
| Rectangular selection | Show aim lines | `regionSelector.rect.showAimLines` | on | |
| Circle selection | Stroke width | `regionSelector.circle.strokeWidth` | 6 | 1–20 |
| Circle selection | Padding | `regionSelector.circle.padding` | 10 | 0–100, in steps of 5 |

A screenshot always goes to the clipboard; **Also save to a file** keeps a file in the folder as
well. The region selector is the one screen snipping uses; see
[Region selector](region-selector.md).

## System

The page is link rows to its subpages: **Region & Language**, **Date & Time**, **Users**, whose
second line is the account's name from AccountsService, **Autostart** and **About**, then, under
**The shell itself**, **Services** and **Advanced**.

### Region & Language

**Language** offers **Auto (System)** and every bundled translation, each by its own name and code,
such as "Русский (ru_RU)". The choice is stored in `language.ui`, `auto` or the code.

Choosing a language other than Auto also makes it the system language, unless it already is: the
shell runs `pkexec proscenio set-system-locale <locale>`, which enables the locale in
`/etc/locale.gen`, runs `locale-gen` and sets `LANG` with `localectl set-locale`. The polkit policy
from [Building and installing](installing.md) covers it. Since the interface is translated once at
start, the shell then restarts and reopens the settings on this subpage.

### Date & Time

The **Date & Time** section works through systemd's `timedated` (`org.freedesktop.timedate1`) and
follows its property changes:

| Control | What it does |
|---|---|
| the current date and time | the local time, updated every second |
| Set the time automatically | calls `SetNTP`; disabled when `CanNTP` is false. While on, a line says "Synchronized with a time server" or "Not synchronized yet" |
| Time zone | shows the zone and its UTC offset; opens a dialog of every zone `ListTimezones` gives, each with its current offset, with a search field that keeps the zones holding every typed word. Picking one calls `SetTimezone` |
| Set the time | shown while the time is not set automatically: a date (YYYY-MM-DD) and a time (HH:MM, or HH:MM:SS) filled with the moment the page opened, and **Set**, which reads them in the current zone and calls `SetTime` |
| Hardware clock keeps local time | calls `SetLocalRTC`; for a computer that also runs Windows, whose clock otherwise comes up hours off |

The error of the last call shows under them. Each call lets polkit ask for a password.

| Section | Control | Key | Default | Range |
|---|---|---|---|---|
| Time Format | 24h, 12h am/pm or 12h AM/PM | `time.format` | `hh:mm` (24h) | `hh:mm`, `h:mm ap`, `h:mm AP` |
| Clock & Calendar | Seconds | `time.secondPrecision` | off | |
| Date formats | Date | `time.dateFormat` | `ddd, dd/MM` | |
| Date formats | Short date | `time.shortDateFormat` | `dd/MM` | |
| Date formats | Date with year | `time.dateWithYearFormat` | `dd/MM/yyyy` | |
| Pomodoro | Focus (min) | `time.pomodoro.focus` | 25 min, stored as 1500 s | 1–180 min, in steps of 5 |
| Pomodoro | Break (min) | `time.pomodoro.breakTime` | 5 min, stored as 300 s | 1–60 min |
| Pomodoro | Long break (min) | `time.pomodoro.longBreak` | 15 min, stored as 900 s | 1–120 min, in steps of 5 |
| Pomodoro | Cycles before long break | `time.pomodoro.cyclesBeforeLongBreak` | 4 | 1–12 |

Choosing a time format also switches the clock of `hyprlock`: in `~/.config/hypr/hyprlock.conf`,
the first whole word `TIME` on each line becomes `TIME12` for a 12-hour format, and the first whole
word `TIME12` becomes `TIME` for 24 hours. **Seconds** makes the shell's clocks show seconds. The time
and date formats use Qt's date and time notation, where `ddd` is the short weekday, `dd` the day,
`MMM` the short month name, `yyyy` the year, `hh` and `mm` the hour and minute, and `AP` or `ap` the
AM/PM marker.

### Users

The **Account** section works through AccountsService (`org.freedesktop.Accounts`); without it, a
notice says the name, email and picture can be neither read nor changed.

- The header shows the account's picture, cropped to a square, or a person symbol; its name; and
  "username · Administrator" or "username · Standard".
- **Choose picture…** asks `kdialog` for a PNG, JPEG or WebP picture in the Pictures folder. The
  picture is scaled and cropped to a 256 px square, saved as
  `$XDG_RUNTIME_DIR/proscenio/face.png` and handed to `SetIconFile`. The button is disabled without
  `kdialog`. **Remove picture** sets an empty picture. A refusal shows under the buttons.
- **Name** and **Email address** are written with `SetRealName` and `SetEmail` when editing
  finishes; the name field shows the username while the name is empty.
- **Change password…** opens a form for the current password, the new one and its repeat. **Change
  password** is live once all three are filled and the two new ones match. It feeds the three answers
  to `passwd` under the C locale; success closes and clears the form, and a failure shows "The
  current password is not correct" or `passwd`'s own complaint.

While AccountsService runs, two more sections follow.

**Other users** lists the accounts `ListCachedUsers` gives, apart from the current one, or says
"Nobody else has an account here". Each card shows the name over the username, adding "password
asked at first login" while the account has no password yet, and holds:

| Control | Call |
|---|---|
| Administrator | `SetAccountType` |
| Set password… | a dialog for a new password and its repeat; the password is hashed with libxcrypt's default method and passed to `SetPassword` |
| Delete account… | a dialog, **Delete NAME?**, with Cancel, **Keep files** and **Delete files**, which call `DeleteUser` and keep or remove the home folder |

A line under the list reports the last outcome, in the error color for a failure.

**Add a user** takes a full name; a user name, filled from the first word of the full name until it
is typed into; the account type, Standard or Administrator; and a password with its repeat. A user
name is up to 32 lower-case letters, digits, `-` and `_`, and starts with a letter or `_`. **Add
user** calls `CreateUser`, then `SetPassword`; with both password fields empty it calls
`SetPasswordMode` instead, so the password is chosen at the first login. It reads "Adding…"
meanwhile and clears the form on success; whatever blocks it shows under it.

Every AccountsService call lets polkit ask for a password.

### Autostart

**Starts with the session** lists one card per `.desktop` file in `~/.config/autostart`, by name,
with its icon (or a terminal symbol), its `Exec` line, a switch and a remove button. While there are
none it reads "Nothing starts with the session yet".

- The switch writes `Hidden`, and `X-GNOME-Autostart-enabled` where the file has it. An entry counts
  as enabled unless `Hidden` is true or `X-GNOME-Autostart-enabled` is false.
- Remove deletes the file.
- **Add application…** opens a list of the installed applications and copies the chosen one's
  desktop file into the folder, enabled.
- **Add command…** asks for a name and a command, and writes a `.desktop` file with `Type`, `Name`
  and `Exec`. The file is named after the name in lower-case letters, digits and dashes (`command`
  when nothing is left), with `-2`, `-3` and so on when the name is taken.

Errors show under the buttons.

The shell starts the enabled entries itself, once per session. When it starts, unless
`$XDG_RUNTIME_DIR/proscenio/autostarted` exists, it creates that file and launches each enabled
entry whose `OnlyShowIn` and `NotShowIn` allow `XDG_CURRENT_DESKTOP` and whose `TryExec`, if any, is
on `PATH`. The runtime folder is emptied at logout, so restarting the shell starts nothing twice.
`/etc/xdg/autostart` is not read.

### About

| Section | Shows |
|---|---|
| Device | the host name and kernel from `/proc/sys/kernel`, the first `model name` in `/proc/cpuinfo`, the graphics controllers `lspci -mm` lists, `MemTotal` from `/proc/meminfo` in GiB, and the session, `XDG_CURRENT_DESKTOP` with Wayland or X11. A row with no value is hidden |
| Storage | one card per `/dev/` device `df` lists, squashfs left out: the mount point, the share used, a bar, the free and total space, and the device and filesystem. Hidden without disks |
| Distro | the name, logo and home page from `/etc/os-release`, with buttons for its documentation, support, bug report and privacy policy pages |
| Dotfiles | the dotfiles project's banner, with buttons for its documentation, issues, discussions and donations |
| Shell | proscenio's banner and repository link; the version, `r<commits>.<short hash>` of the build's git checkout, or the crate version outside one; the GTK version it runs on; the settings window's renderer, Cairo, OpenGL or Vulkan; and buttons for the documentation and the issues |

Links open in the default handler.

### Services

| Section | Control | Key | Default | Range |
|---|---|---|---|---|
| Resources | Polling interval (ms) | `resources.updateInterval` | 3000 | 100–10000, in steps of 100 |
| Conflict killer | Kill notification daemons without asking | `conflictKiller.autoKillNotificationDaemons` | off | |
| System updates (Arch only) | Enable update checks | `updates.enableCheck` | on | |
| System updates | Check interval (mins) | `updates.checkInterval` | 120 | 60–1440, in steps of 60 |
| Pending package thresholds | Advise updating at | `updates.adviseUpdateThreshold` | 75 | 1–1000, in steps of 25 |
| Pending package thresholds | Strongly advise updating at | `updates.stronglyAdviseUpdateThreshold` | 200 | 1–2000, in steps of 25 |
| Weather | Enable GPS based location | `bar.weather.enableGPS` | on | |
| Weather | Fahrenheit unit | `bar.weather.useUSCS` | off | |
| Weather | City name | `bar.weather.city` | | |
| Weather | Polling interval (m) | `bar.weather.fetchInterval` | 10 | 5–50, in steps of 5 |

**Conflict killer**: when the shell starts, it looks for a running `mako` or `dunst`, which conflict
with its own notification server. With the switch on it stops them (`pkill -x`); otherwise a dialog
asks, with **Always**, which also turns the switch on, **No** and **Yes**.

**System updates** count the packages pacman could upgrade with `checkupdates`, from the
pacman-contrib package; a notice says so while checks are on and it is missing. The interval and
both thresholds are disabled while checks are off. With more pending packages than **Advise
updating at**, the bar shows its System updates button; with more than **Strongly advise updating
at**, the button turns to the error color.

The weather comes from wttr.in: for the position GeoClue gives while GPS based location is on, and
otherwise for the city named here.

### Advanced

| Section | Control | Key | Default | Range |
|---|---|---|---|---|
| Workarounds | Dead pixel workaround | `interactions.deadPixelWorkaround.enable` | off | |
| Workarounds | Race condition delay (ms) | `hacks.arbitraryRaceConditionDelay` | 20 | 0–500, in steps of 5 |
| Rendering | Cairo, OpenGL or Vulkan | `renderer` | Cairo (`cairo`) | `cairo`, `opengl`, `vulkan` |

Hyprland leaves one pixel at the right and bottom edges out of interactions; the dead pixel
workaround is for screen corners that do not react to the pointer. Raise the race condition delay
when things occasionally show up in the wrong place or size on a slow system.

**Renderer** picks GTK's renderer for the whole shell. Cairo draws on the processor and holds no
graphics memory; OpenGL and Vulkan draw on the graphics card. A choice that differs from the
renderer in use shows a notice with a **Restart shell** button, and the restart reopens the settings
on this subpage.

A new renderer is on trial until it is kept. Picking it stores the renderer in use in
`rendererFallback`. After the restart, a dialog drawn with Cairo asks whether to keep it, counting
down 15 seconds: keeping clears `rendererFallback`, while reverting, closing the dialog or letting
the time run out restores the previous renderer and restarts the shell. The same answers are IPC
calls, `renderer keep` and `renderer revert`, and `proscenio ipc call renderer reset` returns to
Cairo from a terminal; see [IPC](ipc.md). A `GSK_RENDERER` set in the environment wins over the
choice.

**Shell usage** shows the shell's own load every 2 seconds while the page is open: CPU (of one core)
and resident memory from `/proc/self`, and GPU and video memory from the DRM `fdinfo` of the shell's
own GPU clients, the busiest engine and the video or local memory, falling back to shared memory. On
the proprietary NVIDIA driver, which has no `fdinfo`, `nvidia-smi pmon` gives the shell's row. A
dash means the driver reports nothing.
