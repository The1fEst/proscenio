# The welcome window

Source: `modules/ii/welcome/WelcomeWindow.qml` and `services/FirstRunExperience.qml`.
Read [foundations.md](foundations.md) and [settings.md](settings.md) first: the
window is built from the settings window's parts.

## 1. First run

`FirstRunExperience` reads `<state>/user/first_run.txt`. When the file is missing,
it writes it ("This file is just here to confirm you've been greeted :>"), sets
the default wallpaper through `switchwall.sh`, and opens the welcome window.

## 2. The window

A `FloatingWindow`, 900×650 with a 600×400 minimum, on `m3background`, loaded
while `GlobalStates.welcomeOpen`. The titlebar follows `windows.showTitlebar` and
`windows.centerTitle` like the settings window. It shows "Hi there! First things
first...", a "Show next time" switch at scale 0.6, and a close button with the tip
"Tip: Close a window with Super+Q". The switch deletes the marker when turned on
and writes it back when turned off. Escape closes the window. Closing it sends a
"Welcome app" notification that names Super+Shift+Alt+/ and Super+I.

Below the titlebar is a `ContentPage` in a `m3surfaceContainerLow` box with these
sections:

- **Language**: "Select language", a selection of "Auto (System)" and every
  catalog's code, for `language.ui`.
- **Displays**: a monitor choice and the arrangement (both only with more than one
  monitor), then resolution and refresh rate, as on the Displays page.
- **Sound**: output and input devices.
- **Bar**: position (Top, Left, Bottom, Right) and style (Hug, Float, Rect).
- **Style & wallpaper**:
  - two large `LightDarkPreferenceButton`s: a 250 px preview skeleton tinted with
    the primary hue, whose wavy bar moves only on the active one;
  - "Choose file" with Ctrl Super + T;
  - a notice.
- **Power saving**: the two idle timeout rows from the Power page.
- **Info**: Keybinds (Super + /), Usage and Configuration, the last two linking to
  the upstream wiki.
- **Useless buttons**: GitHub and "Funny number", linking upstream.

IPC target `welcome` with `open`, `close`, `toggle`.

---

**Status (proscenio).** `src/panels/welcome/` holds the window, the page and
`preference.rs` for the large light/dark button. The window reuses the settings
window's titlebar logic and CSS classes. It also reuses the Quick page's bar
selections and wallpaper button (`shortcut_button`), the Power page's
`idle_timeout_row` and the Sound page's `device_label`.

Differences from qs:

- **The marker** is `~/.local/state/proscenio/first_run.txt`. The compat build
  copies qs's `<state>/user/first_run.txt` there.
- **Defaults are applied once.** When `first_run.txt` is missing and
  `~/.local/state/proscenio/defaults_applied.txt` is missing too,
  `src/panels/welcome/defaults.rs` writes that second marker and sets:
  - the main display chosen by the Displays rule (`Displays::primary`);
  - the default applications: Web `zen.desktop`, Mail
    `org.mozilla.Thunderbird.desktop`, Calendar `org.gnome.Calendar.desktop`,
    Music and Video `vlc.desktop`, Photos `satty.desktop`, Text
    `com.microsoft.VSCode.desktop`, Files `org.kde.dolphin.desktop`, each only
    when it is installed;
  - the fonts: Google Sans Medium at 11 (general, menu), 10 (toolbar, title)
    and 9 (small), JetBrainsMono Nerd Font Medium 11 (fixed);
  - GTK theme `adw-gtk3-dark`, Qt style `Darkly`, icons `Papirus-Dark`, cursor
    `Bibata-Modern-Ice` at 36, through the Appearance page's writers;
  - the wallpaper: qs's `assets/images/default_wallpaper.png`, built into the
    binary and written to `~/.local/share/proscenio/default_wallpaper.png`, set
    through `switchwall`.

  The "Show next time" switch only brings the window back. The compat build
  copies `first_run.txt` to `defaults_applied.txt`, so an installation greeted
  earlier keeps its settings.
- **Choosing a language restarts the shell.** The new language applies only
  on a restart, so the choice writes `language.ui`, sets the system language
  as the Region & Language page does, and restarts the shell with
  `PROSCENIO_OPEN_WELCOME` set, which opens the welcome window again.
- **The page wraps to the window.** In qs the language row does not wrap and
  pushes the whole page off to the side, clipped and unscrollable.
