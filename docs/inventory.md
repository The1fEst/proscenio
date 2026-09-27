# Everything in the shell

Every QML file under `dots/.config/quickshell/ii`, grouped the way the tree
is, with what `proscenio` has of it. 425 files, about 43 000 lines. The
per-area documents carry the detail.

Counts are `*.qml` files and their total lines.

## Shell surfaces — `modules/ii`

| Area | Files | Lines | State |
| --- | --- | --- | --- |
| `bar` | 28 | 3034 | done — [bar.md](bar.md) |
| `sidebarRight` | 44 | 2974 | done — [sidebar-right.md](sidebar-right.md) |
| `background` | 21 | 1552 | done — [background.md](background.md) |
| `overview` | 6 | 1353 | done — [overview.md](overview.md) |
| `calendarPanel` | 12 | 1305 | done — [calendar-panel.md](calendar-panel.md) |
| `regionSelector` | 9 | 1258 | done — [region-selector.md](region-selector.md) |
| `verticalBar` | 7 | 831 | done — [bar.md](bar.md) §12 |
| `dock` | 5 | 679 | done — [dock.md](dock.md) |
| `wallpaperSelector` | 3 | 679 | done — [wallpaper-selector.md](wallpaper-selector.md) |
| `cheatsheet` | 5 | 628 | done — [cheatsheet.md](cheatsheet.md) |
| `lock` | 3 | 587 | done — [lock.md](lock.md) |
| `mediaControls` | 2 | 541 | done — [media-controls.md](media-controls.md) |
| `welcome` | 1 | 540 | done — [welcome.md](welcome.md) |
| `settings` | 1 | 509 | done — [settings.md](settings.md) |
| `sessionScreen` | 2 | 396 | done — [session-screen.md](session-screen.md) |
| `onScreenDisplay` | 5 | 384 | done — [osd.md](osd.md) |
| `onScreenKeyboard` | 3 | 335 | done — [osk.md](osk.md) |
| `screenCorners` | 1 | 209 | done — [screen-corners.md](screen-corners.md) |
| `polkit` | 2 | 119 | done — [polkit.md](polkit.md) |
| `notificationPopup` | 1 | 49 | done |

## The settings window — `modules/settings`

30 files, 7449 lines: a page per section over the same `Config` file. The
window and every page are done ([settings.md](settings.md)) but Region &
Language, which stays a placeholder.

## Shared widgets — `modules/common`

126 widget files plus 11 functions, 7 models and the tokens, 10 855 lines in
all. [foundations.md](foundations.md) describes the ones the ported areas
use; the others are not ported.

## Services — `services`

51 files, 6347 lines. Ported: notifications, network, bluetooth, audio,
brightness and Hyprsunset, UPower battery, power profiles, updates, weather,
to-do, the pomodoro timer, EasyEffects, Cloudflare WARP, system info, the
Hyprland event stream, MPRIS, polkit, Ydotool, Persistent state (all of it
but `cheatsheet.tabIndex`, as the cheatsheet has one tab), Idle, Cliphist, app
search, emoji, the launcher search and Wallpapers. The four functions of
`modules/common/Icons.qml` sit next to their users: `battery_icon` in
`src/panels/bar/battery.rs`, `device_symbol` in `src/panels/sidebar/dialogs.rs`,
`bars` in `src/services/net.rs` and `symbol` in `src/services/weather.rs`.
Missing: translations.

## Scripts

The shell also runs 12 Python scripts: `scripts/system/*.py` behind the
settings services, `scripts/colors/*.py` for the wallpaper color scheme, and
`scripts/thumbnails/thumbgen.py` for the wallpaper selector. `proscenio` runs no
Python: each script is rewritten in Rust together with the area that uses
it. The translation tools are developer tools and are not ported.

Ported: the whole of `scripts/colors` ([colors.md](colors.md)),
`scripts/videos/record.sh` as `proscenio record`,
`scripts/system/boot-next-windows.sh` inside the session actions, and
`scripts/thumbnails` inside the wallpaper selector. The KDE color schemes
`kde-material-you-colors` made are built by `src/theming/kde.rs`.
