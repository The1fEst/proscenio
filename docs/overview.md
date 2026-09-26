# The overview

Source: `modules/ii/overview/`, six files, over `services/LauncherSearch.qml`,
`services/AppSearch.qml`, `services/Cliphist.qml` and `services/Emojis.qml`.

One window holding two things stacked with **spacing −8** so they overlap: the
search box, and under it the workspace grid, which is hidden as soon as
anything is typed.

## 1. The window

Anchored to all four edges, `WlrLayer.Top`, transparent, masked to the column,
keyboard `OnDemand` while open and `None` while closed. Dismissible member of
the focus grab; Escape closes it.

Opening it normally clears the search first. `toggleClipboard` and
`toggleEmojis` instead set the query to the clipboard or emoji prefix and
raise `dontAutoCancelSearch`, so a second press of the same shortcut closes it
rather than clearing the prefix.

## 2. The search box

`colBackgroundSurfaceContainer`, radius `searchBar.height / 2 + 4`, clipped,
with the height animating over `elementMove` once results exist. Inside, a
column of: the search bar (left margin 10, right 4, vertical padding 4), a
1 px `colOutlineVariant` separator, and the results list — capped at 600 px,
10 px top and bottom margins, spacing 2, showing at most 15 rows.

The bar's leading icon is a `MaterialShape` whose **shape and glyph both
follow the prefix**: Pill/`settings_suggest` for an action, Clover/`apps` for
an app, Gem/`content_paste_search` for the clipboard, Sunny/`add_reaction`
for emoji, PuffyDiamond/`calculate` for math, PixelCircle/`terminal` for a
shell command, and Cookie7Sided/`search` otherwise.

Typing anywhere in the window focuses the input and inserts the character,
Backspace focuses and deletes (Ctrl+Backspace a whole word), and Tab
completes the selected result's name into the box.

## 3. The results

`LauncherSearch.results` is rebuilt from the query:

- `;` — the whole list is `Cliphist.fuzzyQuery`, each with Copy and Delete
  actions, images previewed inline and blurred when the work-safety rule
  matches;
- `:` — the whole list is `Emojis.fuzzyQuery`, copying the emoji;
- otherwise, in this order: the math result **first** when the query starts
  with a digit or `=`, or the shell command first when it starts with `$`;
  then the apps, fuzzy-matched on name; then any `/action` whose name is a
  prefix of the query or the other way round; and finally, when
  `showDefaultActionsWithoutPrefix`, whichever of the command and the math
  result has not already been shown.

Math goes through `qalc -t`, debounced by `search.nonAppResultDelay` (30 ms).
The actions are `accentcolor`, `dark`, `light`, `superpaste`, `todo`,
`wallpaper` and `wipeclipboard`, plus every executable in
`~/.config/illogical-impulse/actions/` (`~/.config/proscenio/actions/` in
`proscenio`).

A row is a `RippleButton` of radius `normal`, padding 10×6, holding a 35 px
system icon or a 30 px Material symbol or the emoji itself, then a column with
the result type at `smaller` (12) — hidden when it is just "App" — and the
name at `small` (15), and on the right the verb, shown only while the row is
selected. Selected rows are `colPrimaryContainer`.

## 4. The workspace grid

`OverviewWidget` draws the workspaces in a grid, each holding a scaled
rectangle per window positioned from `HyprlandData`, with the app icon in the
middle, an XWayland marker, and drag-and-drop to move a window between
workspaces.

---

**Status (proscenio).** Done: the search and the workspace grid.

The service side follows the QML file for file:

- `src/core/fuzzy.rs` ports `fuzzysort.js` (`prepare` and `go` with a key),
  UTF-16 code units, accent folding, the space-separated search and the
  min-heap included. Its test checks the result order against the shell's own
  `fuzzysort.js` under node.
- `src/panels/overview/launcher.rs` is `LauncherSearch.qml` and `AppSearch.qml`: the same
  priority order, every prefix from the config, the `nonAppResultDelay` math
  timer, apps with their desktop actions, the built-in actions including
  `superpaste` and `todo`, the user scripts, clipboard entries with the
  work-safety blur rule, and emoji from `fuzzel-emoji.sh`.
- `src/services/cliphist.rs` is `Cliphist.qml`: list, copy, delete, wipe and superpaste,
  refreshed `arbitraryRaceConditionDelay` after the clipboard changes.
- `src/platform/desktop.rs` holds the desktop-entry lookup and the `AppLaunch` scope
  launches, shared with the dock.

`src/panels/overview/mod.rs` is `Overview.qml`, `SearchWidget.qml`, `SearchBar.qml` and
`SearchItem.qml`:

- the prefix glyph in its `MaterialShape`;
- the field growing from 210 to 360 px over 300 ms;
- the result list easing its height over 500 ms, capped at 600 px;
- rows built as they come into reach: 15 first, then more as the list scrolls
  or the selection moves past the last one;
- the fuzzy match underlined in `colPrimary`, and the verb on the selected row;
- up to four action buttons with tooltips;
- the clipboard check mark and the image previews, blurred when work safety
  says so;
- keys: Down into the list (landing on the second row, as the QML's
  `currentIndex = 1` does), Up back to the field, Enter, Tab completion,
  Shift+Delete, and type-anywhere with Backspace and Ctrl+Backspace.

`src/panels/overview/grid.rs` is `OverviewWidget.qml` and `OverviewWindow.qml`,
drawn by one widget:

- the rows × columns of workspaces in the active group, with their numbers,
  corner radii and the `colSecondary` indicator easing between them;
- a live capture of every window in the group, dimmed on other monitors,
  with its icon, hover and press tints and the title tooltip;
- a click on a workspace focuses it, a click on a window focuses it,
  a middle click closes it;
- dragging a window onto another workspace moves it there, and dragging a
  floating window inside its own workspace moves it on screen.

The shortcuts are `searchToggle`, `overviewWorkspacesToggle`,
`overviewWorkspacesClose`, `overviewClipboardToggle` and `overviewEmojiToggle`.

Rendering differences from qs:

- Some workspace numbers and one window icon hint 1 px differently.
- The emoji glyph is drawn at 20/23 of its size: Pango scales the color-emoji
  bitmap about 15 % larger than Qt at the same pixel size.

Other differences from qs:

- **Action buttons.** They are overlaid on a placeholder in the row, not
  nested in it. GtkButton's own click gesture runs in the capture phase, so a
  row that is a button takes every click on a button inside it.
- **Unloading.** Everything the search loads is dropped when it hides: the
  results, the app list, the emoji table, the clipboard entries and the
  decoded images. All of it is reloaded on the next open. In qs, the
  clipboard and emoji lists stay loaded for the whole session.
- **Image previews** are decoded directly at the size they are shown. qs
  first decodes them at full size while the item has no width, then at the
  shown size.

Captures run only while the grid is shown and leave the cursor out, as
`ScreencopyView` does by default. They take the GPU path described in
[dock.md](dock.md) and are drawn with trilinear filtering. As in qs, a
thumbnail dropped on another workspace moves there, and a plain click on a
floating window leaves it where it is.
