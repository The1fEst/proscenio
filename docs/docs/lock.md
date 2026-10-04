---
title: Lock screen
sidebar_label: Lock screen
description: How proscenio locks the session with ext-session-lock, what the lock screen shows, unlocking with a password through PAM or a fingerprint through fprintd, the keyring, and Hyprlock as the alternative.
---

# Lock screen

proscenio locks the session itself, through the `ext-session-lock-v1` protocol. While the lock
holds, all input goes to the lock screen, which shows the wallpaper, blurred by default, and the
clock, with the password field and a few controls along the bottom edge.

## Locking

| Way | Locks |
|---|---|
| The `lock` global shortcut | At once |
| `proscenio ipc call lock activate` | At once |
| `lock.launchOnStartup` | When the shell starts in a new Hyprland session |
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

## Unlocking

Enter or the confirm button checks the password with PAM's `login` service, on a thread of its own so
the screen stays responsive.

- Right: the lock lifts.
- Wrong: the field empties, reads "Incorrect password" and shakes.

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
clearing itself after ten seconds. The sleep button suspends at once either way.

### The keyring

With `lock.security.unlockKeyring` on (the default), unlocking with a password also unlocks the
GNOME keyring with it:

- If the `login` keyring is locked, proscenio unlocks it in the running `gnome-keyring-daemon`.
- If there is no `login` keyring, proscenio creates one with that password, labels it "Login", and
  makes it the default keyring when none is set.
- A keyring password prompt that waited while the screen was locked is answered with the same
  password, once. See [Keyring prompt](keyring-prompt.md).

The keyring gets the text in the password field, so a fingerprint unlock with an empty field leaves a
password-protected keyring locked.

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

`src/panels/lock.rs` is the lock screen. It locks through the session lock of the
`gtk4-layer-shell` library, whose functions `src/platform/sessionlock.rs` declares.
`src/platform/pam.rs` runs the PAM conversation, and `src/platform/fprint.rs` drives fprintd's
`net.reactivated.Fprint` interface.

1. proscenio asks Hyprland for the lock first and only then moves the workspaces, so that another
   locker cannot get in between.
2. For every monitor the library reports, proscenio assigns a lock surface and gives its password
   field the keyboard.
3. If Hyprland has not confirmed the lock within six seconds, proscenio gives up and puts the desktop
   back.

proscenio also follows the session's lock state through `hyprland-lock-notify-v1`
(`src/platform/locknotify.rs`). Once its lock has been confirmed, a session that turns unlocked means
another program lifted it, and proscenio drops its own lock and puts the desktop back.

The fingerprint goes through fprintd's D-Bus interface rather than a second PAM conversation,
because a D-Bus verification can be stopped when the password unlocks first, and a blocking
`pam_authenticate` cannot.

The keyring is unlocked over the Secret Service API on the session bus: `UnlockWithMasterPassword`
or `CreateWithMasterPassword` on `org.gnome.keyring.InternalUnsupportedGuiltRiddenInterface`, inside
a `plain` session. A keyring created this way lives at `/org/freedesktop/secrets/collection/login`.
