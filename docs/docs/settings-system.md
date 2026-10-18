---
title: System settings
sidebar_label: System
description: The Notifications, Privacy & Security and System pages, with their subpages for screenshots, language, date and time, users, autostart, updates, the About page, the shell's services and advanced options.
---

# System settings

These settings span three pages of the settings window and their subpages:

| Page | Subpages |
|---|---|
| Notifications | |
| Privacy & Security | Screen Lock, Screenshots & Recording |
| System | Region & Language, Date & Time, Users, Autostart, Updates, About, Services, Advanced |

Controls bound to the shell's config write `~/.config/proscenio/config.toml`. Its keys below are
dotted TOML paths, so `notifications.timeout` is the `timeout` key of the `[notifications]` table.
The rest talk to system services over D-Bus or edit files of their own, as each section says.

## Notifications

![The Notifications page with Do not disturb, the popup settings and the list of apps](/img/screenshots/settings-notifications.webp)

| Control | Key | Default | Range |
|---|---|---|---|
| Do not disturb | `notifications.silent` | off | |
| Hide popups while sharing the screen | `notifications.hideWhileSharing` | on | |
| Stays on screen for (ms) | `notifications.timeout` | 7000 | 1000–60000, in steps of 1000 |
| Always on one display | `notifications.forceMonitor.enable` | off | |
| The display, under Placement | `notifications.forceMonitor.name` | | a monitor's connector name |
| Stays on screen for (ms), under On-screen display | `osd.timeout` | 1000 | 100–3000, in steps of 100 |

**Do not disturb** is the notification server's own switch: notifications still arrive and are kept
in the sidebar, but none pops up. **Hide popups while sharing the screen** does the same while an app
shares or records the screen. The time on screen applies to notifications that do not ask for a
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
every 2 seconds. Only links in the `active` state count: one out of a `Video/Source` node that is
not a camera means the screen is shared, and one from an `Audio/Source`, `Audio/Source/Internal`,
`Audio/Source/Virtual` or `Audio/Duplex` node into a `Stream/Input/Audio` node means the
microphone is in use. A link out of an `Audio/Duplex` node's monitor port does not count, nor does
one into a level meter's stream (`stream.monitor`). A virtual or duplex node counts even when an
app feeds it instead of a microphone. A camera is a node with the `media.role` `Camera` or the
`device.api` `v4l2` or `libcamera`. Without `pw-dump` a notice says both always read as unused.

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

![The System page with link rows for region, date and time, users, autostart, updates and the shell's services](/img/screenshots/settings-system.webp)

The page is link rows to its subpages: **Region & Language**, **Date & Time**, **Users**, whose
second line is the account's name from AccountsService, **Autostart**, **Updates** and **About**,
then, under **The shell itself**, **Services** and **Advanced**.

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
| Set the time | shown while the time is not set automatically: a date (YYYY-MM-DD) and a time (HH:MM, or HH:MM:SS) filled with the moment the page opened, and **Set**, which first fills a field the user has not edited with the current moment, reads them in the current zone and calls `SetTime` |
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

### Updates

![The Updates subpage with the System section's pending packages and the Shell section's commits](/img/screenshots/settings-system-updates.webp)

Two sections show what is pending. Each has one button beside its title: **Update** when there is
something to install, otherwise **Check now**, which counts that section's part again, the way the
[sidebar](sidebar.md#updates) describes: the pending packages under System, the commits the shell is
behind under Shell. While a section's check runs, a loading indicator turns beside its title and its
**Check now** is unavailable.

| Section | Row under the title | Update runs |
|---|---|---|
| System | **Packages**: how many packages can be upgraded, from the repositories and from AUR; when part of the last check failed, the count it did find, if any, and **Could not check for updates** | `pkexec /usr/lib/proscenio/proscenio-packages session`, which lists and then upgrades the repositories through libalpm (see below); then, when AUR packages are pending, `paru -Sa` or `yay -Sa` with the names of the AUR packages listed, without questions, `--noconfirm` and no PKGBUILD review, with `proscenio-packages` in place of `sudo` |
| Shell | **Commits**: how many commits `main` is ahead | after the commits are listed and confirmed, downloads `packaging/PKGBUILD` from `main` into `~/.cache/proscenio/package`, builds and installs it with `makepkg -Acfsi --noconfirm` in a root session of `proscenio-packages`, then restarts `proscenio.service` |

![The Packages subpage listing the pending packages with their old and new versions](/img/screenshots/settings-system-update-packages.webp)

**Packages** opens a page with every pending package, each with its installed and its new
version: under **Repositories** those the [check](sidebar.md#updates) finds in the repositories,
under **AUR** those `paru -Qua` or `yay -Qua` reports. A package's name can be selected, and the
copy button at the end of its row copies the name; the button shows a check for a moment.
**Commits** opens a page with every new commit: its subject and, below it, the first seven
characters of its hash, which can be selected; the copy button at the end of the row copies the
full hash. The **Packages** and **Commits** rows are unavailable while their count is 0, so a check
that failed and found no package leaves **Packages** closed, with **Could not check for updates**
on the row. When part of the last check failed and some packages
were found, the Packages page reads **Could not check for updates** above them. A page that is open
when its last item goes reads **Up to date**. Updates run without a terminal, in a systemd scope
of their own so that restarting the shell does not stop them, and one at a time: both **Update**
buttons are disabled while one runs.

Each update asks for the password once, through polkit's action `dev.fEst.Proscenio.packages`, in
the shell's own dialog, by opening a root session of `proscenio-packages`. Whatever the update
builds installs through that session: `paru`, `yay` and `makepkg` get
`/usr/lib/proscenio/proscenio-packages` as their `sudo` or `PACMAN_AUTH`, which passes their calls to
`pacman` on to the session over an abstract Unix socket, and only with the session's random token,
which the shell hands them in their environment. The session takes a call only from a process of the
user who opened it that descends from the `paru`, `yay` or build the shell started for this update,
and holds at most 8 unanswered connections. An install of package files goes through libalpm; any
other call runs `/usr/bin/pacman` in the session. The session ends with the update.

**Update** under System refreshes before it installs anything. It opens the root session, which asks
for the password, synchronizes the databases and plans the upgrade there, and lists the AUR packages
(see below). The counts and lists of this page and of the [sidebar](sidebar.md#updates) show the
refreshed result, and a dialog shows what the update would install, the same list the update
installs. Under **Repositories** each package of the
transaction has its installed and its new version; a package that comes in as a dependency has its
new version alone, and a package the transaction removes reads **removed** in place of a new
version. Under **AUR** come the packages `paru -Qua` or `yay -Qua` lists. **Update N packages**
installs the list; N counts the dependencies and removals as well, so it can be more than the count on
the **Packages** row. **Cancel**, Escape, a click outside the dialog and closing the Settings window
end the session without installing anything, and so does closing the window while the refresh runs.
When the refresh finds nothing, no dialog opens and the output below reads **Up to date**. When the
AUR listing fails, the update stops there and the output reads **Could not check for updates**; when
the session cannot refresh, such as with no network, the output reads **Failed** and the log names
the error. Either message stays until the next check or update.

A package reads **Needs a reboot** beside its versions, and a line above the list counts those
packages, when it is installed and any of these holds:

- Its installed version ships a kernel image, `usr/lib/modules/<release>/vmlinuz`, or a kernel
  module, a `.ko` file with or without `.zst`, `.xz` or `.gz`, under `usr/lib/modules` outside a
  `build` folder. That covers the kernel packages and prebuilt drivers such as `nvidia-open`.
- Its installed version ships DKMS sources, `usr/src/<folder>/dkms.conf`.
- Its name ends in `-ucode`, the CPU microcode, or in `-dkms`.
- It is `systemd` or `glibc`.

The file list is the `files` entry of the installed version in `/var/lib/pacman/local`; the files of
the new version are not known before it downloads, and a package that comes in as a dependency has
no installed version.

A system update upgrades the packages it listed, no others. The session first synchronizes the
repository databases into `proscenio-stage` inside the pacman database folder, a copy of the system
databases with their times so that unchanged ones are not downloaded again, which leaves the system
databases as they are, and plans the same upgrade as `pacman -Syu` there: the packages to
upgrade, to install as dependencies and to remove. `paru -Qua` or `yay -Qua` lists the AUR packages
at the same time. The shell then sends the repository list back to the session. The session takes
pacman's `db.lck`, which stops the update when another package manager holds it, plans the upgrade
again in the staged copy and refuses to go on when it differs from the list in any package or
version, such as when the repositories or the installed packages changed in between: nothing is
installed, the system databases stay as they are and the log names the packages that differ.
Otherwise it moves the staged databases into the system folder, plans once more against them and
commits the transaction if it still equals the list, without a second synchronization. When the AUR
listing fails, the update stops before it installs anything. The AUR step installs the listed AUR
packages by name, with the version AUR has when it builds them. The staged copy is removed when the
session ends and replaced by the next update's.

**Update** under Shell also refreshes first, without a password: it asks the GitHub API for the
commits again, which updates the count and the list on this page and in the
[sidebar](sidebar.md#updates). A dialog then lists the commits, each with its subject and the
first seven characters of its hash, under **Commits**, with **Cancel** and **Update**. **Cancel**,
Escape, a click outside the dialog and closing the Settings window stop before the password is asked
for. The shell builds `main` as it is when the build starts, so a commit that lands after the refresh
is installed as well. The output reads **Up to date** when the refresh finds no commit and **Could
not check for updates** when the request fails.

**Update** appears once an update has started. It reads **Checking for updates…** while the update
refreshes and the dialog is open, **Updating…** while the update runs, then **Done** or **Failed**
by its exit status. During a system update a bar shows the progress once the dialog is confirmed, with
the step under it: checking, downloading by bytes, installing package by package, running hooks, or
building an AUR package. The repositories and AUR share the bar, half each when both have packages;
a source with nothing to install keeps a tenth for its check. Within a source, the check takes a
tenth, the work eight tenths and the hooks the last tenth.

**Log** opens the output under the bar. It scrolls along with the output unless scrolled up, opens
with the command that runs and ends with how long the update took and, when it failed, its exit
code. Of a line a progress meter redraws with carriage returns, only the last drawing is kept, and
terminal colors are dropped. The log lasts until the next update or until the shell restarts. When
an update ends, the shell checks again.

### About

| Section | Shows |
|---|---|
| Device | the host name and kernel from `/proc/sys/kernel`, the first `model name` in `/proc/cpuinfo`, the graphics controllers `lspci -mm` lists, `MemTotal` from `/proc/meminfo` in GiB, and the session, `XDG_CURRENT_DESKTOP` with Wayland or X11. A row with no value is hidden |
| Storage | one card per `/dev/` device `df` lists, squashfs left out: the mount point, the share used, a bar, the free and total space, and the device and filesystem. Hidden without disks |
| Distro | the name, logo and home page from `/etc/os-release`, with buttons for its documentation, support, bug report and privacy policy pages |
| Shell | proscenio's logo, in the palette's primary fixed colors, with the repository link; the version, `r<commits>.<short hash>` of the build's git checkout, or the crate version outside one; the GTK version it runs on; the settings window's renderer, Cairo, OpenGL or Vulkan; and buttons for the documentation and the issues |

Links open in the default handler.

### Services

| Section | Control | Key | Default | Range |
|---|---|---|---|---|
| Resources | Polling interval (ms) | `resources.updateInterval` | 3000 | 100–10000, in steps of 100 |
| Conflict killer | Kill notification daemons without asking | `conflictKiller.autoKillNotificationDaemons` | off | |
| Updates | Enable update checks | `updates.enableCheck` | on | |
| Updates | Check interval (mins) | `updates.checkInterval` | 120 | 60–1440, in steps of 60 |
| Pending package thresholds | Advise updating at | `updates.adviseUpdateThreshold` | 1 | 1–1000, in steps of 25 |
| Pending package thresholds | Strongly advise updating at | `updates.stronglyAdviseUpdateThreshold` | 200 | 1–2000, in steps of 25 |
| Weather | Enable GPS based location | `bar.weather.enableGPS` | on | |
| Weather | Fahrenheit unit | `bar.weather.useUSCS` | off | |
| Weather | City name | `bar.weather.city` | | |
| Weather | Polling interval (m) | `bar.weather.fetchInterval` | 10 | 5–50, in steps of 5 |

**Conflict killer**: when the shell starts, it looks for a running `mako` or `dunst`, which conflict
with its own notification server. With the switch on it stops them (`pkill -x`); otherwise a dialog
asks, with **Always**, which also turns the switch on, **No** and **Yes**.

**Updates** counts the packages that can be upgraded and the commits the shell is behind; the
[sidebar](sidebar.md#updates) describes how. The interval and both thresholds are disabled while
checks are off. From **Advise updating at** pending packages, the bar shows its updates indicator
and the sidebar its update buttons; from **Strongly advise updating at**, the indicator turns to the
error color.

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
own GPU clients, the busiest engine and the video or local memory, falling back to shared memory when
the video or local memory is zero, unreadable or missing; a region a client reports both as
`drm-total-` and as `drm-memory-` counts once, by its `drm-total-` size when that is readable. An
engine's load is its busy time over the elapsed time, or, on drivers that count cycles such as xe,
the busy cycles of each client over the GPU's cycles, added up per engine of each GPU; an engine
class with several instances is divided by the instance count the driver reports. When the shell
has no DRM client, or none of its clients' `fdinfo` has a readable engine time, busy and total cycle
counts or memory size, as on the proprietary NVIDIA driver, `nvidia-smi pmon` gives the shell's
rows: the busiest GPU's load and the video memory it reports on each GPU or GPU instance, added up.
A dash means the driver reports nothing.
