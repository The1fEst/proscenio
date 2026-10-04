---
title: The dock
sidebar_label: Dock
description: The dock's pinned and running apps, how it hides and comes back, its live window previews and how they are captured, and the settings that shape it.
---

# The dock

The dock is a row of app buttons along the bottom edge of every monitor the shell draws on. It is
off by default: turn it on with `dock.enable`, or with **Enable** under **Appearance › Panels ›
Dock** in the [settings](settings-appearance.md).

From left to right it holds:

- the pin button, which keeps the dock shown and reserves room for it;
- the pinned apps, then a separator when anything is pinned, then the other apps with open windows;
- a button that opens the [overview](overview.md).

## Apps

The list comes from `dock.pinnedApps` and from Hyprland's list of windows:

- Pinned apps come first, in the order of `dock.pinnedApps`, whether they have windows or not.
- Every other window adds its app, grouped by window class. A window whose class matches one of the
  regular expressions in `dock.ignoredAppRegexes` gets no button; the match ignores case.
- Up to three dots under an icon count the app's windows: short bars for one to three windows, small
  squares for more. They take the accent color while one of the app's windows has the focus.
- With `dock.monochromeIcons` on, the default, icons are mostly desaturated and tinted toward the
  accent color.

| On an app button | Does |
|---|---|
| Click | Launches the app when it has no windows; otherwise focuses its windows one after another, starting from the first each time the pointer comes back to the button |
| Middle-click | Launches another instance |
| Right-click | Pins or unpins the app |
| Drag | Moves the button; see below |
| Hover | Opens the window previews, when the app has windows |

Dragging a button more than 10 px picks it up, and dropping it writes the order of every button in
front of the separator to `dock.pinnedApps`. Dropping a running app in front of the separator pins
it, and dropping a pinned app behind it unpins it. With nothing pinned there is no separator, and a
drop changes nothing.

An entry of `dock.pinnedApps` is a window class or a desktop entry ID. Windows join it when their
class equals it, ignoring case. To launch it, the dock looks for the desktop entry `<id>.desktop`,
ignoring case, then for an entry whose `StartupWMClass` is the ID. The app starts in a transient
systemd scope of its own, so it keeps running when the shell restarts.

## Showing and hiding

The dock is shown while any of these is true:

- it is pinned;
- the pointer is over it, with `dock.hoverToReveal` on (the default);
- a window preview is open;
- none of its apps has the focused window, as on an empty workspace.

Otherwise, it slides off the bottom edge. With hover to reveal on, a strip `dock.hoverRegionHeight`
pixels tall (2 by default) and as wide as the dock stays at the edge, and moving the pointer onto it
brings the dock back. With hover to reveal off, a hidden dock leaves nothing behind.

The pin button pins the dock on every monitor at once. While pinned, the dock reserves its height,
so tiled windows end above it. `dock.pinnedOnStartup` decides whether it starts pinned, and changing
that setting also pins or unpins the running dock.

The dock hides while the screen is locked and while a fullscreen window covers its monitor. Under a
fullscreen window a pinned dock keeps its space reserved, so the windows behind do not resize.

## Window previews

Hovering the button of an app with windows for 100 ms opens a popup above it with a card for each
window: the window's title, a close button and a live picture of the window fitted into 300 × 200.
The popup stays open while the pointer is on the button or on the popup, and fades out when it
leaves both.

| On a card | Does |
|---|---|
| Click | Focuses the window |
| Middle-click | Closes the window |
| Close button | Closes the window |

The popup waits until every card has its first picture. A window whose capture fails three times
gets a card without a picture, so it never holds the popup back. After the first picture, each card
asks for the next frame in which the window changed, at most 30 times a second per window. Capturing
stops when the popup has faded out.

## How windows are captured

`src/platform/capture.rs` captures windows for the previews and for the [overview](overview.md)'s
workspace grid. It asks Hyprland for one window at a time through `hyprland-toplevel-export-v1`,
without the cursor.

**On the GPU**, when it can. When the capturer starts, it reads the compositor's default dmabuf
feedback (`linux-dmabuf-v1`) and keeps the format and modifier pairs that Hyprland offers, that GTK
can import and that Vulkan can sample on the compositor's render node. For each frame it allocates a
GBM buffer with one of them, Hyprland copies the window into it, GTK imports it as a dmabuf texture,
and the window's own renderer scales it down to the picture's size with trilinear filtering. Only
the small texture is kept; the full-size buffer is released at once.

**Over `wl_shm`** otherwise: when no pair qualifies, when no GBM device opens, or once the GPU path
has failed (a failed copy, a rejected buffer or a failed import). The frame is copied into shared
memory and shrunk on the CPU by averaging. A capturer that falls back stays on `wl_shm`; the overview
gets a fresh one each time it opens.

**A window with no positive size gets no capture.** Hyprland reports a negative width or height as a
huge unsigned number; such a frame gets no buffer at all, and its capture counts as failed.

The first frame of a picture is taken at once. Every later request waits for the window to change,
and for a thirtieth of a second to have passed since the previous request for the same window.

## Settings

Every setting applies at once.

| Key in `config.toml` | Default | Setting under Appearance › Panels › Dock |
|---|---|---|
| `dock.enable` | `false` | Enable |
| `dock.hoverToReveal` | `true` | Hover to reveal |
| `dock.pinnedOnStartup` | `false` | Pinned on startup |
| `dock.hoverRegionHeight` | `2` | Hover region height (px) |
| `dock.monochromeIcons` | `true` | Tint app icons |
| `dock.height` | `60` | Height (px) |
| `dock.pinnedApps` | `org.kde.dolphin`, `brave-origin`, `kitty`; an empty list pins nothing | arranged in the dock by dragging |
| `dock.ignoredAppRegexes` | none | Ignored apps |

```toml
[dock]
enable = true
pinnedApps = ["firefox", "kitty"]
ignoredAppRegexes = ["^steam_app_.*"]
```

## Surfaces

The dock is drawn on the `top` layer. Its layer namespaces, for Hyprland layer rules:

| Namespace | Surface |
|---|---|
| `proscenio:dock` | The dock itself; unmapped once it has slid out |
| `proscenio:dockTrigger` | The hover strip of a hidden dock, painted at alpha 1/255 so that it maps and takes the pointer |
| `proscenio:dockReserve` | An invisible surface that keeps a pinned dock's space under a fullscreen window |

The code is in `src/panels/dock/mod.rs` and, for the previews, `src/panels/dock/preview.rs`.
