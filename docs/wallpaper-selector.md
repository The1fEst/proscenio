# The wallpaper selector

Source: `modules/ii/wallpaperSelector/`, three files, over `services/Wallpapers.qml`,
`modules/common/widgets/AddressBar.qml`, `AddressBreadcrumb.qml`, `DirectoryIcon.qml`,
`ThumbnailImage.qml` and `scripts/thumbnails/thumbgen.py`.

## 1. The window

Created only while open. `WlrLayer.Overlay`, anchored to the top edge only, so
it is centered, `barHeight + hyprlandGapsOut` below the top, 1200 × 690,
keyboard `OnDemand`, namespace `quickshell:wallpaperSelector`. It is a
dismissible member of the focus grab. Opened by the `wallpaperSelectorToggle`
shortcut or `wallpaperSelector toggle`; with
`wallpaperSelector.useSystemFileDialog` on, both open `switchwall.sh`'s
`kdialog` picker instead. `wallpaperSelectorRandom` and
`wallpaperSelector random` pick a random entry of the current folder without
opening anything.

The card sits `elevationMargin` (10) inside the window: `colLayer0`, a 1 px
`colLayer0Border` border, radius `screenRounding − hyprlandGapsOut + 1` (19),
with the rectangular shadow.

## 2. The layout

A row with spacing −4:

- **Quick folders.** `colLayer1`, 4 px in from the card, radius 15. "Pick a
  wallpaper" at 16 px Medium with 12 px margins, then a 140 px list 4 px in:
  Home, Documents, Downloads, Pictures, Videos, a disabled "---", Wallpapers,
  and Homework when `policies.weeb` is 1. Each is a 38 px pill; the current
  folder is `colSecondaryContainer` with a filled symbol.
- **The address bar.** `colLayer2`, 4 px in, radius 15, padding 6, spacing 8:
  a parent-folder button, the breadcrumb or a text field, and an edit toggle.
  The breadcrumb is one `SelectionGroupButton` per path part, 2 px apart,
  padding 12 × 8 around a 15 px text line; the current part is `colPrimary`
  with full round ends, the others round only at the outer ends. Going up
  keeps the deeper parts visible, so they can be clicked back into.
- **The grid.** 4 columns, cells `width / 4` wide and 4 : 3, clipped to the
  card's radius. A tile is 8 px inside its cell, radius 17, padding 6: the
  thumbnail (cover-cropped, radius 12, shadow, fading in over 200 ms) or a
  folder or file-type icon, then the name at 12 px, elided. The current tile
  is `colPrimary`, the tile of the current wallpaper `colSecondaryContainer`,
  the rest transparent. Hovering makes a tile current.
- **The toolbar**, floating 8 px above the bottom: the system picker button
  (right-click makes it the default), random, the light/dark switch applied
  with the next wallpaper, the 200 px search field, and the close FAB. The
  grid keeps 56 px of empty space below its last row for it.

A thumbnail progress bar sits in the 5 px gap above the grid: a sweeping one
while generation starts, the M3 bar once files are done.

## 3. The folder

The list is the folder's entries newest first, folders included, hidden files
left out. Files must have a wallpaper extension (`jpg jpeg png webp avif bmp
svg`) and contain the search words; only `jpg jpeg png webp tif tiff svg` get
thumbnails. Choosing a folder enters it, choosing a file runs
`switchwall.sh --mode <mode> --image <file>` and closes the selector. Back,
forward and up follow a history of visited folders.

Thumbnails follow the freedesktop spec: `~/.cache/thumbnails/<size>/<md5 of
the file URI>.png` with `Thumb::URI` and `Thumb::MTime`, the size chosen from
the cell (128, 256, 512 or 1024). They are generated for the folder each time
it is entered.

Keys: Escape closes; arrows move the current tile; Enter chooses it; Alt+Up,
Alt+Left and Alt+Right go up, back and forward, as do the mouse's back and
forward buttons; Ctrl+L edits the address; `/` or any typing goes to the
search field; Ctrl+V with a file on the clipboard opens its folder.

---

**Status (proscenio).** Done. `src/panels/wallpaperselector/` holds the
window (`mod.rs`), the address bar and the tiles; `src/services/wallpapers.rs`
the folder, history and apply, and `src/services/thumbnails.rs` the
thumbnails, which replace `thumbgen.py`. The card, the quick folders, the
breadcrumb, the grid and the toolbar match the QML pixel for pixel.

As in the QML: a search with several words looks for the words in order, an
invalid path typed into the address bar is ignored, entering a folder makes
its first tile current, and the search field has focus when the selector
opens.

Differences:

- **Unloading.** The window, the tiles and their textures are built on open
  and dropped on close, and tiles are built as the grid scrolls toward them.
  Only the folder and its history stay between openings.
- **Thumbnails** are written by proscenio itself, only for what the grid can
  show, without Python or GNOME Desktop.
