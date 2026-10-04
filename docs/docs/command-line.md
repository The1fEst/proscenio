---
title: Command line
sidebar_label: Command line
description: The proscenio binary's subcommands and their arguments, from the shell itself to ipc, switchwall, colors, record, renderer-check and the root-only commands the settings run through pkexec.
---

# Command line

proscenio is one binary. Without a subcommand it is the shell; with one, it does that job and exits
without starting the shell.

| Command | What it does |
|---|---|
| `proscenio` | Runs the shell |
| `proscenio ipc …` | Calls the running shell; see [IPC](ipc.md) |
| `proscenio switchwall …` | Sets the wallpaper and recolors; see [Colors from the wallpaper](colors.md) |
| `proscenio colors …` | Generates a palette or picks a scheme for an image |
| `proscenio record …` | Starts or stops a screen recording |
| `proscenio renderer-check` | Asks whether to keep a renderer on trial |
| `proscenio firewall …` | Changes the firewall, as root |
| `proscenio power-settings …` | Sets a power button action or the battery charge limit, as root |
| `proscenio set-system-locale …` | Sets the system language, as root |

The commands exit with 0 on success and 1 on failure, unless their section says otherwise.

## The shell

```bash
proscenio [--no-background]
```

`--no-background` leaves out the background surfaces, for when another program draws the wallpaper.

The shell is a GTK application with the ID `dev.fEst.Proscenio` on the session bus. Started while
another one runs, it hands over to that one and exits, so a session never gets two shells. When the
shell restarts itself, after a renderer change for instance, it keeps its arguments.

| Environment variable | Effect |
|---|---|
| `GSK_RENDERER` | Overrides the renderer setting for this run, and passes on to the programs the shell starts. Without it, the shell sets it from the setting and removes it again once its windows exist |
| `PROSCENIO_OPEN_SETTINGS` | A settings page ID to open once the shell is up, as `settings openPage` takes it in [IPC](ipc.md) |
| `PROSCENIO_OPEN_WELCOME` | Any value opens the welcome window once the shell is up |

## ipc

```bash
proscenio ipc show
proscenio ipc call <target> <function> [arguments…]
```

Lists what the running shell offers, or calls one function of it. It exits with 255 when no shell
runs. [IPC](ipc.md) lists every target.

## switchwall

```bash
proscenio switchwall [--mode dark|light] [--type SCHEME] [--color [HEX|clear]] [--image PATH] [--noswitch] [PATH]
```

Sets the wallpaper, the accent color or the mode, then runs `matugen` and writes the palette, the
terminal colors and the GTK and Qt color schemes. [Colors from the wallpaper](colors.md) describes
every argument and every file it writes.

## colors

The palette tools `switchwall` uses, on their own.

### colors generate

```bash
proscenio colors generate --path IMAGE [options]
proscenio colors generate --color HEX [options]
```

Prints a Material 3 palette as SCSS variables, the format of `material_colors.scss`: `$darkmode`,
`$transparent`, 54 roles, four success colors and, with a terminal scheme, `$term0` to `$term15`.

| Option | Default | Meaning |
|---|---|---|
| `--path IMAGE` | | Takes the source color from an image |
| `--color HEX` | | Takes the source color as given; `--path` wins when both are there |
| `--mode dark\|light` | `dark` | The mode of the palette |
| `--scheme NAME` | tonal spot | One of the `scheme-*` names `switchwall --type` takes, or `scheme-vibrant`; any other name gives tonal spot |
| `--size N` | 128 | The image is scaled to the area of an N × N square before its colors are counted |
| `--transparency transparent\|opaque` | `opaque` | Sets `$transparent` |
| `--termscheme FILE` | | A JSON file with two objects of base terminal colors, `term0` to `term15`, under `dark` and `light`; without it no terminal colors are printed |
| `--harmony F` | 0.8 | The share of the hue distance to the primary color each terminal color moves |
| `--harmonize_threshold F` | 100 | The most a terminal color's hue moves, in degrees |
| `--term_fg_boost F` | 0.35 | How much brighter (dark) or darker (light) each terminal color gets |
| `--blend_bg_fg` | off | Takes `term0` from `surfaceContainerLow` and `term15` from `onSurface` |
| `--cache FILE` | | With `--path`, also writes the source color to `FILE` |

Every option also takes the form `--option=value`.

### colors scheme-for-image

```bash
proscenio colors scheme-for-image [--colorfulness] IMAGE
```

Prints `scheme-neutral` for an image whose colorfulness is below 40 and `scheme-tonal-spot` for any
other, the choice `switchwall --type auto` makes. `--colorfulness` prints the measured number instead.
An image it cannot read makes it exit with 1 and print `scheme-tonal-spot` on stderr.

### colors kde-selection

```bash
proscenio colors kde-selection
```

Copies `primary_container` and `on_primary_container` from
`~/.local/state/proscenio/generated/colors.json` into the `[Colors:Selection]` group of
`~/.config/kdeglobals` and of the color scheme file it names. When that changed `kdeglobals`, it tells
running Qt apps through `org.kde.KGlobalSettings.notifyChange`. It fails when `colors.json` is missing
or lacks either color.

## record

```bash
proscenio record [--region 'X,Y WxH'] [--sound] [--fullscreen]
```

Starts a screen recording with `wf-recorder`, or, when any `wf-recorder` is running, stops it and
exits.

| Option | Meaning |
|---|---|
| `--region 'X,Y WxH'` | The area to record, in the format `slurp` prints |
| `--fullscreen` | Records the monitor Hyprland reports as focused |
| `--sound` | Records the default output's monitor source too, found with `pactl get-default-sink` |

With neither `--region` nor `--fullscreen`, `slurp` asks for the area; cancelling it cancels the
recording. The video goes to `recording_<date>_<time>.mp4` in `screenRecord.savePath`, or the XDG
Videos folder when that is empty, with a notification when recording starts and when it stops. The
command runs until the recording stops.

The [region selector](region-selector.md) records through this command, and the `recordStop`
[shortcut](shortcuts.md) runs `proscenio record` with no arguments to stop.

## renderer-check

```bash
proscenio renderer-check
```

When a renderer is on trial, that is when `rendererFallback` in `config.toml` names the one to go
back to, it shows a dialog on the focused monitor: Keep, Revert, and 15 seconds until it reverts by
itself. The answer goes to the shell as `ipc call renderer keep` or `revert`; with no shell to answer,
the command writes the choice to `config.toml` itself and, on revert, starts the shell. With nothing
on trial it exits at once.

The shell starts this command by itself, with `GSK_RENDERER=cairo`, when it comes up on a renderer
under trial. [Building and installing](installing.md) describes the renderers.

## Commands that run as root

The settings that change the system run one of these through `pkexec`:

```bash
pkexec /usr/bin/proscenio firewall enable
```

With `packaging/dev.fEst.Proscenio.policy` installed, polkit authenticates once for the action
`dev.fEst.Proscenio.system` and keeps the authorization for five minutes in an active session; see
[Building and installing](installing.md). Started by `pkexec`, that is with `PKEXEC_UID` set,
proscenio runs only these three commands and exits with 1 for anything else, so that a kept
authorization cannot start the shell or another subcommand as root. A command that can say why it
failed prints `ERROR: …` on stderr, and the settings window shows that line.

### firewall

```bash
proscenio firewall enable
proscenio firewall disable
proscenio firewall default deny|reject|allow
proscenio firewall add ACTION PROTOCOL PORT FROM APPLICATION COMMENT
proscenio firewall delete ACTION PROTOCOL PORT FROM APPLICATION COMMENT
```

Each command runs `ufw`. `enable` runs `ufw --force enable` and enables `ufw.service`; `default` sets
the policy for incoming connections.

`add` and `delete` take exactly six words; pass `""` for an empty one. Every rule is for incoming
connections to any address of this machine.

| Word | Values |
|---|---|
| `ACTION` | `allow`, `deny`, `reject` or `limit` |
| `PROTOCOL` | `any`, `tcp` or `udp` |
| `PORT` | A port, a range such as `6000:6007`, a comma-separated list, or empty. A range or a list needs `tcp` or `udp` |
| `FROM` | The source address or network, such as `192.168.1.0/24`, or empty for anywhere |
| `APPLICATION` | A `ufw` application profile; when set, `PORT` and `PROTOCOL` are not used |
| `COMMENT` | Free text without quotes or line breaks; `delete` ignores it |

A rule needs at least one of `PORT`, `FROM` and `APPLICATION`.

```bash
pkexec /usr/bin/proscenio firewall add allow tcp 22 192.168.1.0/24 "" "ssh from home"
```

### power-settings

```bash
proscenio power-settings button KEY ACTION
proscenio power-settings charge-limit BATTERY PERCENT
```

`button` sets what logind does on the power button or the lid. `KEY` is `HandlePowerKey`,
`HandleLidSwitch`, `HandleLidSwitchExternalPower` or `HandleLidSwitchDocked`, and `ACTION` is
`ignore`, `lock`, `suspend`, `hibernate` or `poweroff`. The setting goes to the `[Login]` section of
`/etc/systemd/logind.conf.d/50-proscenio.conf`, and logind rereads it on `SIGHUP`.

`charge-limit` stops charging the battery at `PERCENT`, from 50 to 100. `BATTERY` is its name under
`/sys/class/power_supply`, such as `BAT0`. The limit is written to its
`charge_control_end_threshold` and kept across reboots by `/etc/tmpfiles.d/proscenio-charge-limit.conf`,
which 100 removes.

### set-system-locale

```bash
proscenio set-system-locale LOCALE
```

`LOCALE` has the form `ll_RR.UTF-8`, such as `de_DE.UTF-8`. The command enables it in
`/etc/locale.gen`, runs `locale-gen` when that changed anything, and makes it the system language with
`localectl set-locale LANG=LOCALE`.
