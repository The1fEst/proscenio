---
title: Lock screen
sidebar_label: Lock screen
description: How proscenio locks the session with ext-session-lock, what the lock screen shows, unlocking with a password through PAM or a fingerprint through fprintd, the keyring, and Hyprlock as the alternative.
---

# Lock screen

proscenio locks the session itself, through the `ext-session-lock-v1` protocol. While the lock
holds, all input goes to the lock screen, which shows the wallpaper, blurred by default, and the
clock, with the password field and a few controls along the bottom edge.

![The lock screen](/img/screenshots/lock.webp)

## Locking

| Way | Locks |
|---|---|
| The `lock` global shortcut | At once |
| `proscenio ipc call lock activate` | At once |
| `lock.launchOnStartup` | When the shell starts in a new Hyprland session |
| Starting the shell while the session is locked | At once, see [Restarting while locked](#restarting-while-locked) |
| `loginctl lock-session` | When the program that answers logind's lock request starts the lock |

`loginctl lock-session` is what the [session screen](session-screen.md)'s **Lock** button, the
automatic screen lock and **Lock before sleep** run. logind passes the request to the session, and
`hypridle` answers it by running its `lock_cmd`. To have those lock with proscenio, set that command
in `~/.config/hypr/hypridle.conf`:

```ini
general {
    lock_cmd = proscenio ipc call lock activate
}
```

Locking while the screen is already locked does nothing. **Launch on startup** compares Hyprland's
instance signature with the one stored in `~/.local/state/proscenio/states.json` by the previous
start, so restarting the shell inside the same session does not lock it again.

`lockFocus` and `proscenio ipc call lock focus` give the password field the keyboard again, for when
Hyprland takes it away after the machine wakes up.

proscenio holds a logind `delay` inhibitor for sleep. When the machine is about to sleep, it keeps
the sleep waiting until the lock screen has finished appearing and the compositor has shown that
frame, for at most 2 seconds, or 500 ms when no lock starts. After waking, it replaces the lock
surfaces of the monitors still connected with new ones, because a frame sent just before sleep may
never be confirmed, and GTK draws nothing more until it is. Every connected monitor without a lock
surface then gets one.

### Restarting while locked

When the shell crashes or restarts while the screen is locked, Hyprland keeps the session locked and
shows its "lockdead" screen. The shell that starts next locks again at once and takes the password
as before; for Hyprland to hand the session to the new lock, `misc.allow_session_lock_restore` has
to be on, and without it the "lockdead" screen stays. Unlocking returns each monitor to the
workspace it was on before the first lock.

While its lock holds, proscenio sets logind's `LockedHint` on the session, which
`loginctl show-session --property=LockedHint` shows, and it clears the hint when the lock lifts. At
start, the shell decides:

| Hyprland's lock state | Hyprland instance | `LockedHint` | Locks |
|---|---|---|---|
| Locked | Any | Any | Yes |
| Unlocked | The same as at the previous start | Any | No, and a set hint is cleared |
| Unknown, without `hyprland-lock-notify-v1` | The same as at the previous start | Set | Yes |
| Unknown, without `hyprland-lock-notify-v1` | The same as at the previous start | Not set | No |
| Unlocked or unknown | New | Set | Yes |
| Unlocked or unknown | New | Not set | Only with `lock.launchOnStartup` |

Within one Hyprland instance, Hyprland's own state decides, so a hint left set by a shell killed
right after unlocking never locks an open desktop. The hint decides when Hyprland itself was
restarted inside the same logind session while locked, which would otherwise open the desktop.

## What the lock screen shows

The lock surfaces are transparent, and what shows through is the desktop, changed for the lock:

- Every monitor moves to an empty workspace, numbered 2147483647 minus its current one, so no window
  shows. 150 ms after unlocking, each monitor goes back to the workspace it was on.
- With `lock.blur.enable` on (the default), the [background](background.md) zooms in by
  `lock.blur.extraZoom` (110 %) over 400 ms and draws the wallpaper blurred by `lock.blur.radius`
  (100), washed with the background color. Then it moves above the other layers.
- The background's clock moves to the center of the screen with `lock.centerClock` and carries a
  "Locked" badge with `lock.showLockedText`. Both are on by default.
- The bar, the dock, the notification popups and the on-screen keyboard hide, and the session screen
  closes. Keyring prompts wait until the screen unlocks.

Along the bottom edge, three groups grow into place:

| Group | Holds |
|---|---|
| Left | Your full name from `/etc/passwd`, or your username; the keyboard layout; the Fcitx tray item while Fcitx runs |
| Middle | The fingerprint symbol when a reader is set up, the password field, and the confirm button |
| Right | The battery level when there is a battery, and the sleep, power off and restart buttons |

The battery shows a bolt while charging, and turns to the error color at or below `battery.low`
(20 %) when it is not charging.

## Typing the password

All monitors share one password field: what is typed on one shows on every one. Moving the pointer,
clicking or pressing a key gives the field the keyboard.

- With `lock.materialShapeChars` on (the default), each character shows as a shape, the seven shapes
  taking turns, fading from the accent color. With it off, the field shows ordinary dots.
- Escape clears the field, and so do ten seconds without a key.
- Holding Control turns the confirm button into a coffee cup: Ctrl+Enter unlocks and turns on
  **Keep awake**, the idle inhibitor of the [quick toggles](quick-toggles.md).
- While Caps Lock is on, "Caps Lock is on" shows above the field.

## Unlocking

Enter or the confirm button checks the password with PAM's `login` service, on a thread of its own so
the screen stays responsive.

- Right: the lock lifts.
- Wrong: the field empties and shakes. Error and informational messages PAM sent during the check
  show above the field in the error color, one per line, and the field reads "Enter password";
  `pam_faillock`, for example, says that the account is locked and for how many minutes. Without a
  message the field reads "Incorrect password". The messages stay until the next check.

### Fingerprint

When the lock starts, proscenio asks fprintd on the system bus for the default reader and for the
fingers enrolled for you. With at least one finger, the fingerprint symbol shows left of the field
and the reader starts verifying.

- A matching finger unlocks, as the right password would.
- Three fingers that do not match stop the reader until the next lock.
- Any other result that ends a verification, such as the reader disconnecting, starts it again a
  second later.

The reader is released when the lock lifts.

### Power off and restart

The power off and restart buttons act at once. With `lock.security.requirePasswordToPower` on, a
click arms them instead: the button lights up, and the next successful password or fingerprint
powers off or restarts instead of unlocking. A second click disarms it, and so does the field
clearing itself after ten seconds; a password already being checked keeps the action it was
submitted for, and the field waits with clearing until the check has ended. The sleep button
suspends at once either way.

### The keyring

With `lock.security.unlockKeyring` on (the default), unlocking with a password also unlocks the
GNOME keyring with it:

- If the `login` keyring is locked, proscenio unlocks it in the running `gnome-keyring-daemon`.
- If there is no `login` keyring, proscenio creates one with that password, labels it "Login", and
  makes it the default keyring when none is set.
- A keyring password prompt that waited while the screen was locked is answered with the same
  password, once. See [Keyring prompt](keyring-prompt.md).

The keyring gets only the password PAM has just accepted, and never an empty one, so a fingerprint
unlock leaves the keyring as it is.

## Hyprlock instead

With `lock.useHyprlock` on, and `hyprlock` installed, every way of locking starts `hyprlock`
instead, unless it is already running, and Hyprlock's own config decides what it shows. Without
`hyprlock` the setting has no effect.

## Settings

The settings are under **Privacy & Security › Screen Lock** in the [settings](settings.md), with the
automatic lock timeout and **Lock before sleep**, which edit `hypridle.conf`.

| Key in `config.toml` | Default | Setting |
|---|---|---|
| `lock.useHyprlock` | `false` | Use Hyprlock (instead of proscenio) |
| `lock.launchOnStartup` | `false` | Launch on startup |
| `lock.security.requirePasswordToPower` | `false` | Require password to power off/restart |
| `lock.security.unlockKeyring` | `true` | Also unlock keyring |
| `lock.centerClock` | `true` | Center clock |
| `lock.showLockedText` | `true` | Show "Locked" text |
| `lock.materialShapeChars` | `true` | Use varying shapes for password characters |
| `lock.blur.enable` | `true` | Enable blur |
| `lock.blur.extraZoom` | `1.1` | Extra wallpaper zoom (%) |
| `lock.blur.radius` | `100` | Blur radius |

## How it works

`src/panels/lock/` is the lock screen. It locks through the session lock of the
`gtk4-layer-shell` library, whose functions `src/platform/wayland/sessionlock.rs` declares.
`src/platform/security/pam.rs` runs the PAM conversation, and `src/platform/dbus/fprint.rs` drives
fprintd's `net.reactivated.Fprint` interface.

1. proscenio asks Hyprland for the lock first and only then moves the workspaces, so that another
   locker cannot get in between.
2. For every monitor the library reports, proscenio assigns a lock surface and gives its password
   field the keyboard. Each monitor has one lock surface, also when it is connected while the
   screen is locked. A monitor that is disconnected loses its lock surfaces. For 2 seconds
   after Hyprland confirms the lock, and after each lock surface that appears later, proscenio
   keeps each window's frame clock running. A window whose frame clock has not run for a second at
   the end is replaced with a new lock surface. Each display connector gets at most two
   replacements, counted afresh at each lock, each wake and each time the display reconnects. Each
   replacement writes a line starting with `lock:` to the journal, and so does a stalled window
   once both are used up.
3. If Hyprland has not confirmed the lock within six seconds, proscenio puts the desktop back and
   stops the fingerprint reader, but keeps the request, because the library cannot withdraw a lock
   that is still pending. When Hyprland confirms it later, the workspaces are moved away again, the
   reader starts again, and the lock screen takes a password or fingerprint as usual. Locking again
   while the request is pending does nothing.

proscenio also follows the session's lock state through `hyprland-lock-notify-v1`
(`src/platform/wayland/locknotify.rs`). Once its lock has been confirmed, a session that turns
unlocked means another program lifted it, and proscenio drops its own lock and puts the desktop
back. At start, proscenio waits for Hyprland's answer to the lock notification request, which
reports a session that is already locked, before it decides whether to lock.
`src/platform/dbus/lockhint.rs` reads and sets `LockedHint` on logind's
`/org/freedesktop/login1/session/auto`.

The fingerprint goes through fprintd's D-Bus interface rather than a second PAM conversation,
because a D-Bus verification can be stopped when the password unlocks first, and a blocking
`pam_authenticate` cannot.

The keyring is unlocked over the Secret Service API on the session bus: `UnlockWithMasterPassword`
or `CreateWithMasterPassword` on `org.gnome.keyring.InternalUnsupportedGuiltRiddenInterface`, inside
a `plain` session. A keyring created this way lives at `/org/freedesktop/secrets/collection/login`.
