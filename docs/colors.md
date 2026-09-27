# The wallpaper color scheme — `scripts/colors`

Changing the wallpaper, the accent color or light and dark mode goes through
one script, `switchwall.sh`. It runs matugen for the application templates,
three Python scripts for the shell's own palette, and two more shell scripts
for the terminals and VS Code. `proscenio switchwall` is all of that in one
binary, with the same arguments.

## 1. `switchwall.sh`

    switchwall.sh [--mode dark|light] [--type SCHEME] [--color [HEX|clear]]
                  [--image PATH] [--noswitch] [PATH]

- `--color HEX` stores `appearance.palette.accentColor`, `--color clear`
  empties it, and a bare `--color` stores what `hyprpicker --no-fancy` picks.
- `--noswitch` keeps the current `background.wallpaperPath`.
- With no image, no accent and no `--noswitch`, `kdialog` asks for a file in
  `Pictures/Wallpapers/showcase`, else `Pictures/Wallpapers`, else `Pictures`.
- A new image clears the accent color.
- `--type` falls back to `appearance.palette.type`. It must be one of the
  eight `scheme-*` names or `auto`; `auto` asks `scheme_for_image.py`, and a
  missing image or an unknown answer means `scheme-tonal-spot`.
- Without `--mode`, the mode is whatever GNOME's `color-scheme` says, and the
  script then sets `color-scheme` and `gtk-theme` (`adw-gtk3` or
  `adw-gtk3-dark`) to match.

For an image it offers Upscayl in a notification when the picture is smaller
than the largest monitor. For a video (`mp4 webm mkv avi mov`) it plays it
with `mpvpaper` on every monitor, colors from the first frame `ffmpeg`
extracts, and writes `hypr/custom/scripts/__restore_video_wallpaper.sh` so
Hyprland can play it again after a restart. Any other wallpaper empties that
script.

`appearance.wallpaperTheming.enableAppsAndShell` false stops it after the
mode switch. Otherwise, it runs `matugen --source-color-index 0 image|color`
with the mode and scheme, then `generate_colors_material.py` with
`terminalGenerationProps` (`harmony`, `harmonizeThreshold`, `termFgBoost`,
and `forceDarkMode` for the terminal palette) into `material_colors.scss`.
After that, in the background:

- `applycolor.sh`, when `enableTerminal` is on, fills `kitty-theme.conf` and
  `sequences.txt` from `scripts/colors/terminal`, reloads kitty with SIGUSR1
  and writes the sequences to every `/dev/pts/N`.
- `kde-material-you-colors-wrapper.sh`, when `enableQtApps` is not off, runs
  `kde-material-you-colors` from the Python venv, then `kde-selection.py`
  copies the primary container into the KDE selection colors.
- `material-code-set-color.sh` writes the seed color into
  `material-code.primaryColor` of every VS Code fork's `settings.json`.

## 2. The palette — `generate_colors_material.py`

`materialyoucolor` 2.0.10: the image is resized with bicubic resampling to
the area of a 128 px square, quantized by Wu and then Wsmeans to 128 colors,
scored, and the first color seeds the scheme. `--smart` switches to
`scheme-neutral` below chroma 20. The output is 54 Material roles, the four
success colors, and the 16 terminal colors harmonized toward the primary
key color.

`scheme_for_image.py` resizes to 128 px with Lanczos and picks
`scheme-content` when the average colorfulness is above 40, otherwise
`scheme-tonal-spot`.

**Status:** `proscenio colors generate`, `scheme-for-image` and
`kde-selection` give the same output as the Python scripts byte for byte, in
every scheme, both modes and with `--smart`. Matching it takes the library's
own details: its C++ quantizer with glibc `rand()` seeded 42688, the integer
`abs()` in Wsmeans, Pillow's fixed-point resampling, `round()` rounding half
to even, and the inverted monochrome checks in the tertiary container tones.
Two inputs differ: a color without `#` (Python drops its first digit), and
`scheme-fidelity` on pure white or black (Python raises).

## 3. `proscenio switchwall`

Same arguments, same order of work. It leaves the same files as
`switchwall.sh`: the palette, both terminal themes, the config, gsettings and
`kdeglobals`. Differences from the script:

- the terminal themes are written unless `enableTerminal` is set to false, as
  the Qt and shell switches already are: `config.toml` holds only the keys
  that were changed, so a missing key means the default, which is on
- kitty is reloaded with `pkill -USR1 -x kitty`; the script's
  `kill $(pidof kitty)` passes all the PIDs as one word and reloads nothing
- the terminal sequences are written to `/dev/pts/N` with `O_NONBLOCK`
- `pkill -9 -x mpvpaper` instead of `pkill -f`, which also matches any
  command line containing the word
- a missing `mpvpaper` and `ffmpeg` install as two packages, not one quoted
  word
- an unreadable image never prompts to upscale
- `material-code.primaryColor` is added with a correct comma after an empty
  or comma-terminated object

The terminal templates and `scheme-base.json` are built into the binary.
`kde-material-you-colors` runs from the Python venv in
`ILLOGICAL_IMPULSE_VIRTUAL_ENV`.

The shell runs `proscenio switchwall` as a child process
(`switchwall::detach`) from the launcher's `accentcolor`, `dark` and `light`,
the bar's dark mode button, the dark mode quick toggle, IPC
`theme toggleLightDark`, and the Quick settings page's wallpaper, scheme,
accent and mode controls.
