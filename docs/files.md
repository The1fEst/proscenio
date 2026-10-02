# Where proscenio keeps its files

| Path | What |
| --- | --- |
| `~/.config/proscenio/config.toml` | the configuration, same keys as the QML shell's `config.json` |
| `~/.config/proscenio/actions/` | the launcher's `>name` scripts |
| `~/.local/state/proscenio/todo.json` | the to-do list |
| `~/.local/state/proscenio/states.json` | timers and other state kept across restarts |
| `~/.local/state/proscenio/generated/` | the color scheme: `colors.json` and `color.txt` from matugen, `material_colors.scss`, `terminal/kitty-theme.conf`, `terminal/sequences.txt` |
| `~/.local/state/proscenio/first_run.txt`, `defaults_applied.txt` | the first-run markers ([welcome.md](welcome.md)) |
| `~/.local/share/proscenio/default_wallpaper.png` | the default wallpaper the first run sets |
| `~/.cache/proscenio/notifications.json` | the notification history |
| `~/.cache/proscenio/notifications/` | images that notifications sent as raw pixels, one `<id>.png` each |
| `~/.cache/proscenio/coverart/` | downloaded album art |
| `$XDG_RUNTIME_DIR/proscenio/` | screenshots in progress, decoded clipboard images, the cava config |
| `~/.config/proscenio/translations/<code>.json` | extra translation keys, read with the built-in catalog ([foundations.md](foundations.md) §10) |

When the shell stores a setting in `config.toml`, it changes only that key
and keeps the rest of the file, comments included.

The dots point matugen, kitty, fish and zsh at
`~/.local/state/proscenio/generated`. The QML shell does not follow those
wallpaper changes.

Icons, the default wallpaper, the cava config and the terminal templates are
built into the binary; nothing is read from `~/.config/quickshell`.

## Commands

| Command | What |
| --- | --- |
| `proscenio` | the shell |
| `proscenio ipc call …`, `proscenio ipc show` | [ipc.md](ipc.md) |
| `proscenio switchwall …` | [colors.md](colors.md) |
| `proscenio colors generate …`, `scheme-for-image …`, `kde-selection` | [colors.md](colors.md) |
| `proscenio record [--region WxH+X+Y] [--sound] [--fullscreen]` | starts `wf-recorder` into `screenRecord.savePath` (else the Videos folder), or stops it when one is running |
| `proscenio firewall enable`, `disable`, `default POLICY`, `add …`, `delete …` | run as root through `pkexec`: changes `ufw` for the Firewall page ([settings.md](settings.md) §19.4) |
| `proscenio power-settings button KEY ACTION`, `charge-limit BATTERY PERCENT` | run as root through `pkexec`: sets a logind button or lid action, or the battery's charge limit ([settings.md](settings.md) §23) |
| `proscenio set-system-locale LOCALE` | run as root through `pkexec`: enables `LOCALE` (such as `ru_RU.UTF-8`) in `/etc/locale.gen`, generates it, and makes it the system language ([foundations.md](foundations.md) §10) |

The `pkexec` commands (`firewall`, `power-settings`, `set-system-locale`) run
under the polkit action
`dev.fEst.Proscenio.system` when `packaging/dev.fEst.Proscenio.policy` is
installed in `/usr/share/polkit-1/actions/`: it names `/usr/bin/proscenio` as
its program and keeps an active session's authentication for five minutes
(`auth_admin_keep`), so several changes in a row ask once. Without it, or for
a binary elsewhere, `pkexec` asks every time. Started by `pkexec` (with
`PKEXEC_UID` set), proscenio runs only these commands and exits with a
failure for anything else, so the kept authentication cannot start the shell
or another command as root.

`record` takes the sound from the default output's monitor source. The QML
shell's `record.sh` passes every monitor source at once, which fails when
there are two or more.
