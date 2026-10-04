---
title: Power and screen lock
sidebar_label: Power and screen lock
description: The Power page, which sets idle timeouts, power button and lid actions, battery warnings, the charge limit and power profiles, and the Screen Lock subpage, which sets when and how the session locks.
---

# Power and screen lock

The **Power** page sets what happens when the computer sits idle, what the power button and the lid
do, and how the battery is looked after. **Screen Lock**, a subpage of Privacy & Security, sets when
the session locks and how the [lock screen](lock.md) looks.

## Idle timeouts and hypridle

The idle controls on both pages edit `~/.config/hypr/hypridle.conf`, the file `hypridle` reads, and
then restart it with `systemctl --user restart hypridle.service`. They exist only when `hypridle` is
on `PATH`; without it, a notice says the session never blanks, locks or suspends on its own.

Each timeout is a switch and an "after (min)" number from 1 to 600, in steps of 5, which is inactive
while the switch is off. Turning a switch on starts with a default length; turning it off removes
the matching `listener` block from the file. A listener is recognized by what its `on-timeout` runs:
a command mentioning `dpms` blanks the screen, then one mentioning `lock` locks, then one mentioning
`suspend` suspends.

| Timeout | Listener it writes | Default when turned on |
|---|---|---|
| Blank the screen | `hyprctl dispatch 'hl.dsp.dpms({ action = "disable" })'`, and `enable` on resume | 15 minutes |
| Lock the session | `$lock_cmd` when the file defines it, else `loginctl lock-session` | 30 minutes |
| Suspend | `$suspend_cmd` when the file defines it, else `systemctl suspend \|\| loginctl suspend` | 45 minutes |

## Power

**Power Saving**

| Control | Does |
|---|---|
| Automatic Screen Blank | turns the screens off after a period of inactivity |
| Apps can keep the screen on | lets video players, calls and games hold off blanking, locking and suspend while they play |

"Apps can keep the screen on" is on while none of `ignore_dbus_inhibit`, `ignore_systemd_inhibit`
and `ignore_wayland_inhibit` in the `general` block of `hypridle.conf` is `true`. Turning it off
writes all three as `true`; turning it on removes them.

**Buttons & Lid** fills in once systemd-logind answers. Each row offers Nothing, Lock, Suspend,
Hibernate and Power off (logind's `ignore`, `lock`, `suspend`, `hibernate` and `poweroff`), Hibernate
only when logind's `CanHibernate` says yes or it is the current choice. A value set some other way
that is none of these shows as an extra choice.

| Row | Shown | logind setting |
|---|---|---|
| Power button | always | `HandlePowerKey` |
| Lid closed | when UPower reports a lid | `HandleLidSwitch` |
| Lid closed on the charger | when UPower reports a lid | `HandleLidSwitchExternalPower` |
| Lid closed with an external display | when UPower reports a lid | `HandleLidSwitchDocked` |

A choice runs `pkexec proscenio power-settings button <setting> <action>`, which writes the setting
under `[Login]` in `/etc/systemd/logind.conf.d/50-proscenio.conf` and sends logind `SIGHUP` so it
reads its configuration again. The row then shows what logind reports, and an error appears under
the section when the change fails. See [polkit](polkit.md) for how the authorization is asked for.

**Battery** needs UPower; without it a notice says the battery level is unknown and none of these
fire. The keys here and on Screen Lock are in `~/.config/proscenio/config.toml`.

| Control | Does | Key, default |
|---|---|---|
| Low warning | sends a notification when the battery falls to this percent | `battery.low`, 20 |
| Critical warning | sends another one at this percent, naming the level that suspends | `battery.critical`, 5 |
| Automatic suspend | suspends the system when the battery runs this low | `battery.automaticSuspend`, on |
| at | the percent it suspends at; inactive while Automatic suspend is off | `battery.suspend`, 3 |
| Full warning | sends a notification when the charging battery reaches this percent; 101 never does | `battery.full`, 101 |

The warnings and the suspend apply only while the battery is discharging, the full warning only while
it charges.

**Stop charging at (%)** shows where the first battery in `/sys/class/power_supply` (named `BAT…`)
has a `charge_control_end_threshold`: a battery that stays plugged in lasts longer when it is not
kept full. It goes from 50 to 100 in steps of 5. A second after the last change it runs
`pkexec proscenio power-settings charge-limit <battery> <percent>`, which writes the threshold and
keeps it across reboots with `/etc/tmpfiles.d/proscenio-charge-limit.conf`; at 100 that file is
removed.

While power-profiles-daemon is on the system bus, two more rows follow. When the charger is plugged
in or out, the shell switches to the chosen profile, unless it is Unchanged:

| Control | Choices | Key |
|---|---|---|
| Power profile on battery | Unchanged, Power saver, Balanced | `battery.profileOnBattery` |
| Power profile on the charger | Unchanged, Balanced, Performance | `battery.profileOnCharger` |

**Automatic Suspend** holds the **Suspend when idle** timeout. With it off, the machine keeps
drawing power while nobody is at it.

## Screen Lock

Screen Lock opens from the System section of Privacy & Security.

| Control | Does | Key, default |
|---|---|---|
| Automatic Screen Lock | locks the session after a period of inactivity (a hypridle timeout) | — |
| Lock before sleep | locks the session before the system sleeps | `before_sleep_cmd` in hypridle's `general` block |
| Use Hyprlock (instead of proscenio) | locks with `hyprlock` instead of the shell's own lock screen, for example for fingerprint unlock | `lock.useHyprlock`, off |
| Launch on startup | locks the session when the shell starts with a new Hyprland session, not when the shell restarts | `lock.launchOnStartup`, off |

"Lock before sleep" is on while `before_sleep_cmd` runs something that locks. Turning it on writes
`before_sleep_cmd = loginctl lock-session`; turning it off removes the key. Without `hyprlock` a
notice says the session always locks with proscenio.

**Security**

| Control | Does | Key, default |
|---|---|---|
| Require password to power off/restart | makes the lock screen's power off and restart buttons ask for the password first; holding the power button still forces a shutdown | `lock.security.requirePasswordToPower`, off |
| Also unlock keyring | unlocks the login keyring along with the session, useful when locking on startup stands in for a display manager | `lock.security.unlockKeyring`, on |

Unlocking the keyring needs `gnome-keyring-daemon`; without it a notice says unlocking leaves the
keyring as it is.

**Style: general**

| Control | Key, default |
|---|---|
| Center clock | `lock.centerClock`, on |
| Show "Locked" text | `lock.showLockedText`, on |
| Use varying shapes for password characters | `lock.materialShapeChars`, on |

**Style: Blurred**

| Control | Key, default |
|---|---|
| Enable blur | `lock.blur.enable`, on |
| Extra wallpaper zoom (%) | `lock.blur.extraZoom`, 1 to 150 %, stored as a factor: 1.1 shows as 110 |
| Blur radius | `lock.blur.radius`, 100 (0 to 300) |

The zoom and radius are inactive while blur is off.
