# The lock screen

Source: `modules/ii/lock/` (three files) and `modules/common/panels/lock/`
(`LockScreen.qml`, `LockContext.qml` and a PAM configuration for the
fingerprint reader).

## 1. Locking

`GlobalStates.screenLocked` turns on, and a `WlSessionLock` puts one
transparent surface on every monitor through `ext-session-lock-v1`. The
desktop shows through, changed as follows:

- every monitor is moved to workspace `2147483647 − n`, which is empty, so no
  window shows; each monitor's workspace is remembered and focused again
  150 ms after the lock lifts;
- the background layer moves to `Overlay` once its zoom has finished, and with
  `lock.blur.enable` draws a copy of the wallpaper blurred by
  `lock.blur.radius` (a `GaussianBlur`, σ = (radius + 1) / 3.3333), washed with
  `colLayer0` at 30 %, and scaled by `lock.blur.extraZoom` about its center over
  400 ms on `expressiveDefaultSpatial`;
- the bar, the dock and the notification popups hide, and the session screen
  closes.

`lock.useHyprlock` runs `pidof hyprlock || hyprlock` instead. On start, with
`lock.launchOnStartup`, the screen locks when the Hyprland instance signature
differs from the one the last run stored.

The IPC target `lock` (`activate`, `focus`) and the shortcuts `lock` and
`lockFocus` reach it; `focus` gives the password field the keyboard again
after Hyprland drops it on a wake.

## 2. The surface

Three toolbars (`Toolbar`: `m3surfaceContainer`, 56 px tall, radius 28, 8 px
padding, 4 px spacing, the elevation shadow) along the bottom edge, 20 px up.
The middle one is centered; the others sit 10 px to its left and right. They
grow from 90 % to full size over 500 ms on `expressiveFastSpatial` and fade in
over 200 ms.

- **Middle:** the password field — 200 × 40, `colLayer1`, fully rounded, the
  placeholder at 14 px (Qt's `leftPadding` is the padding plus 4) in
  `colSubtext`, "Enter password" or "Incorrect password" — and a 40 px round
  confirm button in `colPrimary` with a 24 px symbol: `arrow_right_alt`,
  `coffee` while Control is held, or the power action waiting for the
  password.
- **Left:** `account_circle` with the user's full name from `/etc/passwd`,
  and `keyboard_alt` with the layout code in capitals.
- **Right:** the battery (`bolt` while charging, `colError` when low and not
  charging), then round 40 px buttons for sleep, power off and restart, 22 px
  symbols. Power off and restart act at once unless
  `lock.security.requirePasswordToPower`, in which case they toggle
  (`colSecondaryContainer`) and the next unlock does them instead.

With `lock.materialShapeChars` each character is a Material shape, 20 px
apart, cycling through Clover4Leaf, Arrow, Pill, SoftBurst, Diamond,
ClamShell and Pentagon: it fades in over 50 ms, grows from half size over
200 ms and from nothing to 18 px over 250 ms, and turns from `colPrimary` to
`colOnLayer1` over a second. A 2 px `colPrimary` caret follows the cursor, and
the row scrolls to keep its end in view, both over 400 ms on
`emphasizedDecel`. A wrong password shakes the field −30, 30, −15, 15, 0 px.

Every monitor shares one text: typing on one shows on all. Escape clears it,
and it clears itself after 10 s without a key. Any pointer motion or click
gives the field the keyboard back.

## 3. Unlocking

Enter or the confirm button hands the text to PAM's `login` service off the
main thread. On success the power action runs if one is waiting; otherwise,
with `lock.security.unlockKeyring`, the GNOME keyring is unlocked with the same
password when it is still locked, the surfaces go, and with Control held the
idle inhibitor turns on. On failure the text clears and the field shakes.

---

**Status (proscenio).** `src/panels/lock.rs`, with `ext-session-lock-v1` from
the `gtk4-layer-shell` library. Its Rust bindings do not cover the session
lock, so `src/platform/sessionlock.rs` declares the four calls. PAM goes
through `src/platform/pam.rs`, and the background is in
`src/panels/background/mod.rs`. The blurred background is within about five
levels of qs's on average.

proscenio asks for the lock before it moves the workspaces: the `hyprctl`
calls take long enough for another locker to get in first. The dots allow a
second locker through `misc:allow_session_lock_restore`. Hyprland 0.56 then
either ignores the later request without an answer, while the first lock is
still being set up, or hands the lock over without telling the old holder.
proscenio watches the session through `hyprland-lock-notify-v1`
(`src/platform/locknotify.rs`):

- a request that gets no `locked` within 6 s is given up, and everything the
  lock changed is put back;
- once the lock has been granted, an unlocked session means somebody else
  lifted it. proscenio then unlocks its own `GtkSessionLockInstance`, the
  library's only way to forget a lock and destroy its windows, and puts the
  desktop back.

A request that Hyprland ignored stays pending inside `gtk4-layer-shell`,
which has no call to drop it, and every later lock fails until proscenio
restarts. Only two lockers racing cause it; the dots' `hypridle.conf` starts
hyprlock only while no shell runs.

The keyring is unlocked in Rust the way `scripts/keyring/unlock.sh` does it:
kill the running `gnome-keyring-daemon`, then start `--daemonize --login` with
the password on its standard input. proscenio discards what the daemon
prints; the script exports it only into its own short-lived bash.

The password field takes its keys through GTK's own input method, never an
input method server.

The clock in the middle and the "Locked" badge under it come from the
background's clock widget ([background.md](background.md)).

The fingerprint reader is driven through fprintd's D-Bus interface
(`src/platform/fprint.rs`) rather than a second PAM context: a blocking
`pam_authenticate` in a thread cannot be aborted, and the D-Bus calls can.
Each lock asks fprintd for the default device and the user's enrolled fingers.
With at least one finger, the `fingerprint` icon shows left of the field and
the device is claimed for verification. As with `pam_fprintd`'s `max-tries`,
three fingers that do not match stop the reader until the next lock; any other
failure starts it again a second later. A match performs the pending action,
as a right password would. qs stops the reader whenever the 10 s timer clears
the field and never restarts it; proscenio keeps it running until the lock
goes.

The Fcitx item is the bar's tray row filtered to the `Fcitx` id
(`tray::build` with `only`), with no overflow button and no separator, hidden
while fcitx is not running. Like the bar's, it has no left-click
`activate()`; right-click opens the menu.
