# The session screen

Source: `modules/ii/sessionScreen/`, two files, plus
`modules/common/functions/Session.qml` and `services/SessionWarnings.qml`.

A full-screen overlay holding eight large buttons.

## 1. The window

```qml
WlrLayershell.namespace: "quickshell:session"
WlrLayershell.layer: WlrLayer.Overlay
WlrLayershell.keyboardFocus: WlrKeyboardFocus.Exclusive
exclusionMode: ExclusionMode.Ignore
anchors { top: true; left: true; right: true }
implicitWidth / implicitHeight: the focused screen's
color: transparentize(m3background, darkmode ? 0.05 : 0.12)
```

It lives behind a `Loader` on `GlobalStates.sessionOpen`, refreshes
`SessionWarnings` every time it opens, and closes itself when the screen
locks. Clicking anywhere closes it; so does Escape.

An `IpcHandler` on `session` and the shortcuts `sessionToggle`,
`sessionOpen`, `sessionClose`.

## 2. The content

A centered column, spacing 15:

1. the title `Session` in the title font at `pixelSize.title` (22), and under
   it, with no spacing, "Arrow keys to navigate, Enter to select / Esc or
   click anywhere to cancel" at `normal` (16);
2. a four-column grid, spacing 15, of eight `SessionActionButton`s;
3. a `DescriptionLabel` carrying the focused button's name.

Below the column, offset 10, two more `DescriptionLabel`s in the error
container colors, each behind a `Loader` on one of the warnings.

A `DescriptionLabel` is a `colTooltip` rectangle of radius 17, padded 10
vertically and 15 horizontally, and it animates its width with
`elementMove`.

## 3. The buttons

| Row | | | | |
| --- | --- | --- | --- | --- |
| 1 | `lock` Lock | `dark_mode` Sleep | `logout` Logout | `desktop_windows` Reboot to Windows |
| 2 | `downloading` Hibernate | `power_settings_new` Shutdown | `restart_alt` Reboot | `settings_applications` Reboot to firmware settings |

`SessionActionButton` is 120×120 with a 45 px symbol. Its radius is
`rounding.verylarge` (30) normally and **half its size** (60) while focused
or pressed, eased with `elementMoveFast`. The background is
`colSecondaryContainer`, `colPrimary` when focused or hovered, and
`colSecondaryContainerActive` while Enter is held. The text color flips to
`m3onPrimary` in all of those states. Lock starts focused, and the buttons
are wired to each other with explicit `KeyNavigation` in both axes.

## 4. The actions

All from `Session.qml`, and every one but lock, sleep and hibernate calls
`closeAllWindows()` first, which kills every pid in `HyprlandData.windowList`.

```
lock        loginctl lock-session
suspend     systemctl suspend  || loginctl suspend
hibernate   systemctl hibernate || loginctl hibernate
logout      closeAllWindows; pkill -i Hyprland
poweroff    closeAllWindows; systemctl poweroff || loginctl poweroff
reboot      closeAllWindows; reboot || loginctl reboot
firmware    closeAllWindows; systemctl reboot --firmware-setup || loginctl …
windows     closeAllWindows; '<scripts>/system/boot-next-windows.sh' && (systemctl reboot || …)
```

## 5. The warnings

`SessionWarnings.refresh()` runs two probes and reads their exit codes:

```
pidof yay paru dnf zypper apt apx xbps snap apk yum epsi pikman || ls /var/lib/pacman/db.lck
pidof curl wget aria2c yt-dlp || ls ~/Downloads | grep -E '\.crdownload$|\.part$'
```

---

**Status (proscenio).** `src/panels/sessionscreen.rs` opens one surface per monitor
anchored to all four edges, `Layer::Overlay`, exclusive keyboard,
`exclusive_zone(-1)`, and the same translucent background. The title, the
instruction, the four-column grid of eight 120 px buttons, the focused
button's name in the caption under the grid, and the two warnings re-probed on
every open are all there. It closes when the screen locks.

Each button is a `RippleButton`, as `SessionActionButton` is:

- background `colSecondaryContainer`, `colPrimary` while focused and
  `colSecondaryContainerActive` while Enter is held;
- hover `colPrimary`, ripple `colPrimaryActive`;
- the 45 px symbol in `m3onPrimary` while down, Enter-held, focused or
  hovered, else `colOnLayer0`;
- radius `rounding.verylarge` (30), or 60 while focused or down, animated
  over 200 ms of `expressiveEffects` (`RippleButton::animate_radius`);
- a styled tooltip with the action's name.

Opening focuses Lock. The arrow keys follow the QML's `KeyNavigation` map: a
4×2 grid with no wrap, where an arrow at an edge keeps the focus. Enter clicks
the focused button. A press on the background closes the screen; a press on a
button does not. The IPC target `session` (`toggle`, `open`,
`close`) and the shortcuts `sessionToggle`, `sessionOpen`, `sessionClose` act
on the focused monitor's surface. The eight actions are in
`src/services/session.rs`, `closeAllWindows` included, reading the pids out
of `hyprctl clients`.

The power button in the sidebar's system row calls the screen's `open()`
directly.
