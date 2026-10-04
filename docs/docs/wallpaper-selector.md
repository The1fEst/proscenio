---
title: Wallpaper selector
sidebar_label: Wallpaper selector
description: The folder browser that picks a wallpaper, with its quick folders, address bar, search and thumbnails, the random pick, and the system file picker it can hand over to.
---

# Wallpaper selector

The wallpaper selector is a folder browser for pictures. It opens on the focused monitor, at the top
center under the bar, and choosing a picture makes it the wallpaper, recolors the shell from it (see
[Colors](colors.md)) and closes the selector.

## Opening it

| Global shortcut | `proscenio ipc call …` | Does |
|---|---|---|
| `wallpaperSelectorToggle` | `wallpaperSelector toggle` | Opens or closes the selector |
| `wallpaperSelectorRandom` | `wallpaperSelector random` | Picks a random entry of the current folder without opening anything |
| — | `wallpapers apply <path>` | Makes that file the wallpaper, in the current light or dark mode |

The launcher's `/wallpaper` action opens it too; see [Overview and launcher](overview.md).

With `wallpaperSelector.useSystemFileDialog` on (**Use system file picker** under **Appearance ›
Background** in the [settings](settings-appearance.md)), opening the selector opens the system file
picker, `kdialog`, instead.

Escape, the close button or a click outside closes it.

## The window

On the left, **Pick a wallpaper** heads a list of quick folders. The one that is current is
highlighted.

| Quick folder | Path |
|---|---|
| Home | `~` |
| Documents, Downloads, Pictures, Videos | The XDG folders |
| Wallpapers | `Wallpapers` in the XDG pictures folder |
| Homework | `homework` in the XDG pictures folder; listed only with `policies.weeb = 1` |

On the right are the address bar, the grid and the toolbar.

**The address bar** shows the current folder as a row of path parts. Clicking a part goes there, and
after going up the deeper parts stay in the row so that you can click back into them. The button on
its left goes to the parent folder. The edit button, or Ctrl+L, turns the row into a text field:
Enter goes to the folder typed, a path that does not exist is ignored, and Escape turns it back.

**The grid** has four columns. A tile shows the picture's thumbnail, or the icon of a folder or of
the file's type, and the name under it. The tile under the pointer or the keyboard selection is
filled with the primary color, and the tile of the current wallpaper with the secondary container
color. Clicking a folder enters it; clicking a picture applies it.

**The toolbar** floats over the bottom of the grid:

| Control | Does |
|---|---|
| System file picker | Opens the system file picker and closes the selector; a right-click also makes the picker the default, turning on `wallpaperSelector.useSystemFileDialog` |
| Random | Picks a random entry of the current folder |
| Light/dark | Chooses the mode that the next wallpaper is applied in; it starts at the current one |
| Search field | Filters the folder |
| Close | Closes the selector |

A thin bar above the grid shows thumbnail generation: a sweeping bar while it starts, then the share
of files done.

## The folder

The selector lists the current folder, newest first by modification time, then by name. Folders are
always listed. Files are listed when their extension is one of `jpg`, `jpeg`, `png`, `webp`, `avif`,
`bmp` or `svg`, in any case, and when they can be opened. Hidden files are left out.

The search keeps the files whose name, without its extension, contains every word typed, in that
order and ignoring case. It does not filter folders.

The selector starts in the `Wallpapers` folder each time the shell starts, and remembers the folder
it was left in until the shell exits. It follows changes on disk while it is open. Back and forward
follow the folders visited.

:::note

The selector lists pictures only. A video wallpaper is chosen through the system file picker.

:::

A random pick takes any entry of the current folder that the search leaves, as if it were chosen:
a picture is applied, and a folder is entered instead.

## Keys

| Key | Does |
|---|---|
| Up, Down | Move the selection a row |
| Left, Right | Move the selection a tile, while the search field does not have the keyboard |
| Enter | Chooses the selected tile |
| Alt+Up | Goes to the parent folder |
| Alt+Left, Alt+Right | Go back and forward; so do the mouse's back and forward buttons |
| Ctrl+L | Edits the address |
| Ctrl+V | With a folder on the clipboard, opens it; with a file, opens the folder it is in |
| `/` | Moves to the search field |
| Any other typing | Goes to the search field; Backspace deletes from it |
| Escape | Closes the selector, or leaves the address field |

The search field has the keyboard when the selector opens.

## Applying a wallpaper

Choosing a picture runs `proscenio switchwall --mode <light|dark> --image <file>`, which sets the
wallpaper and regenerates the colors from it. The system file picker runs the same command without
`--image`, which asks `kdialog` for a file.

## Thumbnails

`src/services/thumbnails.rs` writes thumbnails the freedesktop way, so other programs share them:
`~/.cache/thumbnails/<size>/<MD5 of the file's URI>.png`, with the `Thumb::URI` and `Thumb::MTime`
keys. A thumbnail whose `Thumb::MTime` matches the file's modification time is reused, and any
other is written again.

The size is the smallest of `normal` (128), `large` (256), `x-large` (512) and `xx-large` (1024)
that covers a tile. Each time the selector opens or enters a folder, it goes through the folder's
files and makes the missing thumbnails, in the background. Tiles show thumbnails for `jpg`, `jpeg`, `png`, `webp`,
`tif`, `tiff` and `svg` files with a lowercase extension; other files show their type's icon.

## How it works

The selector is built when it opens and destroyed when it closes: a surface on the `overlay` layer,
namespace `proscenio:wallpaperSelector`, 1200 × 690. Tiles are built five rows at a time, then four
more as the grid scrolls near its end. Only the current folder and its history survive between
openings, in `src/services/wallpapers.rs`. The window is in `src/panels/wallpaperselector/`.
