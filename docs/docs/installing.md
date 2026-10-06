---
title: Building and installing
sidebar_label: Building and installing
description: What proscenio needs to build and run, where the binary and the polkit policy go, how to start it with a Hyprland session, and the programs each feature calls.
---

# Building and installing

proscenio is one binary. It needs Hyprland to run, a few libraries to build, and, for some features,
programs it calls on demand.

## Building

| Needed to build | Version |
|---|---|
| Rust | edition 2024, so Rust 1.85 or newer |
| GTK | 4.22 or newer |
| gtk4-layer-shell | 1.3 or newer |
| libpulse | the PulseAudio client library; a PipeWire session serves it through `pipewire-pulse` |
| libwayland-client | loaded when the shell starts, not linked |

```bash
cargo build --release
```

The binary lands in `target/release/proscenio`. The release profile links with LTO into a single
codegen unit, aborts on panic and strips symbols. `cargo build --profile profiling` builds the same
code with line tables kept, for a profiler.

## Installing

On Arch Linux, `packaging/PKGBUILD` builds the package `fEst-proscenio` from `main` on GitHub; it
installs the binary and the polkit policy:

```bash
curl -fsSLO https://raw.githubusercontent.com/The1fEst/proscenio/main/packaging/PKGBUILD
makepkg -si
```

The sidebar's Shell update button runs the same two steps in `~/.cache/proscenio/package` (see
[Updates](sidebar.md#updates)). Elsewhere, install the build by hand:

```bash
sudo install -m755 target/release/proscenio /usr/bin/proscenio
sudo install -m644 packaging/dev.fEst.Proscenio.policy /usr/share/polkit-1/actions/
```

The policy defines the polkit action `dev.fEst.Proscenio.system`, which names `/usr/bin/proscenio` as
its program. The settings that change the system run that binary as root through `pkexec`: the
firewall rules, the power button and lid actions, the battery charge limit and the system language.
With the policy installed, an active session authenticates once and the authorization is kept for
five minutes, so several changes in a row ask once. Without it, or with the binary somewhere else,
`pkexec` asks every time.

:::note

A shell started by a systemd user unit inherits systemd's `PATH`, not the one your login shell
builds. The programs proscenio runs have to be found there, which `/usr/bin` always is and
`~/.cargo/bin` or `~/.local/bin` usually are not.

:::

## Starting it

Run one proscenio per Hyprland session. It takes over these roles on the session bus, so another
program holding one of them has to go:

| Role | Bus name or registration |
|---|---|
| Notification server | `org.freedesktop.Notifications` |
| Tray host | `org.kde.StatusNotifierWatcher` |
| Polkit authentication agent | registered with `org.freedesktop.PolicyKit1` for the session |
| Keyring prompter | `org.gnome.keyring.SystemPrompter` |

A systemd user unit restarts it if it ever exits:

```ini
[Unit]
Description=proscenio
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart=/usr/bin/proscenio
Restart=on-failure
RestartSec=2
Slice=session.slice

[Install]
WantedBy=graphical-session.target
```

Or start it from Hyprland's own config:

```lua
hl.on("hyprland.start", function()
    hl.exec_cmd("proscenio")
end)
```

## Hyprland's config

The settings window writes Hyprland options, monitor rules, window rules and shortcuts to one file
per area under `~/.config/hypr/settings/`, in Hyprland's Lua syntax: `appearance.lua`,
`displays.lua`, `keyboard.lua` and so on. Hyprland applies them only if its config loads that
folder, and loads it last, so that a change made in the settings window wins over the rest of the
config:

```lua
local folder = os.getenv("HOME") .. "/.config/hypr/settings"
local settings = io.popen('ls -1 "' .. folder .. '" 2>/dev/null')
if settings then
    for name in settings:lines() do
        local area = name:match("^(.+)%.lua$")
        if area then
            require("settings." .. area)
        end
    end
    settings:close()
end
```

Idle timeouts go to `~/.config/hypr/hypridle.conf`, the file `hypridle` reads.

## Fonts

Every icon in the shell is a glyph of **Material Symbols Rounded**, so that font is required. The
text fonts are settings, and these are the defaults:

| Role | Default |
|---|---|
| Main and titles | Google Sans |
| Reading | Readex Pro |
| Expressive | Space Grotesk |
| Monospace and Nerd icons | JetBrains Mono NF |

## Services on the system bus

The pages and panels that need one of these say so when it is missing, and the rest of the shell
works without it.

| Service | What needs it |
|---|---|
| NetworkManager | Wi-Fi, Ethernet, VPN, the hotspot and the network indicators |
| BlueZ | Bluetooth |
| UPower | the battery indicator and the power page |
| power-profiles-daemon | the power profile toggle |
| AccountsService | the Users page |
| GeoClue | locating the weather when no city is set; the forecast itself comes from wttr.in |

## Programs called on demand

proscenio runs these when a feature needs them. A missing one disables only that feature, and the
settings page it belongs to names it along with the package it comes in.

| Program | Arch package | Without it |
|---|---|---|
| `bluetoothctl` | bluez-utils | pairing a device that asks for confirmation fails |
| `brightnessctl` | brightnessctl | the built-in screen's brightness cannot change |
| `btop`, `kitty` | btop, kitty | the default task manager command opens nothing |
| `cava` | cava | the media controls show no audio wave |
| `cliphist` | cliphist | the launcher's clipboard prefix finds nothing |
| `ddcutil` | ddcutil | external monitors' brightness cannot change |
| `djpeg` | libjpeg-turbo | the shell cannot read colors from a JPEG wallpaper |
| `easyeffects` | easyeffects | the EasyEffects toggle does nothing |
| `efibootmgr` | efibootmgr | the session screen cannot restart into Windows |
| `gnome-keyring-daemon` | gnome-keyring | unlocking leaves the keyring as it is |
| `grim`, `magick`, `wl-copy`, `satty` | grim, imagemagick, wl-clipboard, satty | screenshots and the snip actions fail |
| `hypridle` | hypridle | the session never blanks, locks or suspends on its own |
| `hyprlock` | hyprlock | the session always locks with proscenio's own lock screen |
| `hyprpicker` | hyprpicker | the color picker does nothing |
| `kdialog` | kdialog | there is no file picker for wallpapers, pictures and certificates |
| `matugen` | matugen | nothing follows the wallpaper: the shell keeps its built-in palette, since its own colors come from a matugen template too (see [Colors](colors.md)) |
| `mpvpaper`, `ffmpeg` | mpvpaper, ffmpeg | a video cannot be the wallpaper |
| `nmcli` | networkmanager | connections are not listed or switched |
| `pactl` | libpulse | `proscenio record --sound` finds no output to record |
| `paru` or `yay` | paru or yay (AUR) | the update count leaves out AUR packages, since `pacman` counts the repositories only |
| `pw-dump` | pipewire | the microphone and screen always read as unused |
| `qalc` | libqalculate | the launcher shows no math results |
| `slurp`, `wf-recorder` | slurp, wf-recorder | screen recording fails |
| `ufw` | ufw | the firewall cannot be set up |
| `warp-cli` | cloudflare-warp-bin (AUR) | the Cloudflare WARP toggle stays unavailable |
| `wg` | wireguard-tools | a WireGuard connection gets no key |
| `wpctl` | wireplumber | the microphone button does nothing |
| `ydotool` | ydotool | the on-screen keyboard types nothing |

## Renderer

The shell draws with GTK's Cairo renderer by default, so its surfaces hold no GPU memory. OpenGL and
Vulkan are a setting away. A newly picked renderer is on trial: after the restart a dialog, drawn
with Cairo so that it shows even if the new renderer fails, asks whether to keep it, and reverting,
closing it or letting its 15 seconds run out goes back to the previous one. `GSK_RENDERER` set in
the environment overrides the setting and stays in the environment the shell's apps inherit; a
renderer the shell picks from the setting is cleared once its windows exist, so the apps it starts
do not inherit that one.
