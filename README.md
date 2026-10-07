# proscenio

A desktop shell for Hyprland, written in Rust with GTK 4. It draws everything around the windows:
the bar, the sidebar with its quick toggles, notifications, the dock, the launcher and workspace
overview, on-screen indicators, the session and lock screens, and a settings app for the shell and
for Hyprland itself. Its colors come from the wallpaper.

![The desktop with the sidebar open, in dark mode](docs/static/img/screenshots/sidebar-dark.webp)

| | |
|---|---|
| ![The overview with its workspace grid](docs/static/img/screenshots/overview.webp) | ![The launcher listing results](docs/static/img/screenshots/search.webp) |
| ![The settings window in dark mode](docs/static/img/screenshots/settings-dark.webp) | ![The settings window in light mode](docs/static/img/screenshots/settings-light.webp) |
| ![The calendar panel](docs/static/img/screenshots/calendar.webp) | ![The wallpaper selector](docs/static/img/screenshots/wallpapers.webp) |
| ![The cheatsheet](docs/static/img/screenshots/cheatsheet.webp) | ![The welcome window](docs/static/img/screenshots/welcome.webp) |
| ![The session screen](docs/static/img/screenshots/session.webp) | ![The lock screen](docs/static/img/screenshots/lock.webp) |
| ![The desktop in light mode with the sidebar open](docs/static/img/screenshots/sidebar-light.webp) | ![The desktop in light mode](docs/static/img/screenshots/desktop-light.webp) |

## Install

On Arch Linux, `packaging/PKGBUILD` builds the package `fEst-proscenio` from `main`:

```bash
curl -fsSLO https://raw.githubusercontent.com/The1fEst/proscenio/main/packaging/PKGBUILD
makepkg -si
```

proscenio runs only on Hyprland. [hypr-dots](https://github.com/The1fEst/hypr-dots) installs it
together with the Hyprland config and apps it is set up with.

## Documentation

[the1fest.github.io/proscenio](https://the1fest.github.io/proscenio/docs/) describes every panel,
setting, command and file, and how the shell is built. The pages are in `docs/`.
