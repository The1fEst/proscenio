---
title: Notepad
sidebar_label: Notepad
description: The panel on the left edge of the screen for quick text notes, with tabs, autosave to plain files, find, and line and character counts.
---

# Notepad

The notepad is a full-height panel on the left edge of the screen for scratch notes. Each note is a
tab with a plain-text editor under it, and everything typed is saved by itself. The source is in
`src/panels/notepad/`, with the notes and their files in `src/services/session/notes/`.

## Opening and closing

Every monitor the shell runs on has a notepad of its own, and they show the same notes.

| To open or close it | Acts on |
|---|---|
| A left click on the bar's [left region](bar.md#the-left-region-brightness), the top of a vertical bar, or a left [screen corner](screen-corners.md#corner-regions) | The screen clicked |
| The notepad button in the [bar](bar.md#utility-buttons), off by default | Hyprland's focused monitor |
| `proscenio ipc call notepad toggle`, or `open`, `close` | Hyprland's focused monitor |
| The global shortcut `notepadToggle`, which has no default bind | Hyprland's focused monitor |
| The launcher [action](overview.md#actions) `/notepad` | Hyprland's focused monitor |

[IPC](ipc.md) and [Shortcuts](shortcuts.md) show how to call and bind these.

The notepad closes on:

- **Escape.** With the delete dialog open, the first Escape closes the dialog. Otherwise, it ends a
  tab rename, then closes the find bar, and then closes the notepad.
- **A click outside it**, on a window, the bar or the wallpaper. This is Hyprland's
  `hyprland-focus-grab-v1`; see [Architecture](architecture.md#focus-grabbing). Only one panel holds
  a grab at a time, so opening the notepad closes the sidebar or the calendar if one is open, and
  opening one of them closes the notepad.

Closing saves every unsaved note at once. The content is built once and kept; about a second after
the notepad closes, its window gives back its surface and buffers.

## The window

| Property | Value |
|---|---|
| Layer | top |
| Namespace | `proscenio:notepad`, for Hyprland layer rules |
| Anchors | top, left and bottom |
| Width | 480 px: the panel, 5 px of gap above, below and to the left, and 10 px on the right for its shadow |
| Exclusive zone | none of its own; it keeps clear of the bar's, so it starts below a top bar |
| Keyboard focus | on demand |

The window does not animate itself. A layer rule makes it slide in from the left edge:

```lua
hl.layer_rule({ match = { namespace = "proscenio:notepad" }, animation = "slide left" })
```

## Tabs

Each note is a tab in the row at the top. The row scrolls sideways when the tabs do not fit, and the
mouse wheel scrolls it. The **+** button after the tabs, or Ctrl+N, adds an empty note and switches
to it. There is always at least one tab.

- **Title.** A tab shows the first line of its note that is not blank, trimmed and cut to 60
  characters, or **Untitled** for an empty note. It follows the text as it changes.
- **Rename.** A double click on a tab replaces its title with a text field. Enter or a click
  elsewhere keeps the new title, and Escape drops it. A renamed tab keeps its title whatever the
  note says; renaming it to nothing returns it to the first line.
- **Close.** The **×** on a tab, or Ctrl+W for the open one, deletes the note and its file. An
  empty note goes at once; any other asks first in a dialog: **Delete this note?** with **Cancel**
  and **Delete**. Closing the last tab leaves a fresh empty one.

## The editor

A plain-text editor in the monospace font, `appearance.fonts.monospace` (see
[Settings](settings.md)), with word wrap. Ctrl+Z undoes and Ctrl+Shift+Z redoes, and the history
of a note lasts until the shell restarts. Under the editor, a footer shows the number of lines and
characters.

A note holds at most 2 MiB of text. Typing or pasting past that is ignored, and the footer turns red
for a moment.

### Find

Ctrl+F, or the magnifier button after the tabs, opens the find bar under the tabs. With a selection
of one short line in the editor, the field starts with it. The match is case-insensitive.

- The first match from the cursor on is selected as you type, and the editor scrolls to it.
- Enter or the down arrow selects the next match, Shift+Enter or the up arrow the previous one, and
  both wrap around at the ends of the note. When the selection is not on a match, the search goes on
  from the cursor.
- The count reads `3 of 12`, or **No matches**. It follows edits to the note while the bar is open
  and stops at 50000 matches.
- Escape or the **×** closes the bar and returns to the editor.

## Keys

| Key | Action |
|---|---|
| Ctrl+N | New note |
| Ctrl+W | Close the open note |
| Ctrl+Page Up, Ctrl+Page Down | Previous or next tab |
| Ctrl+F | Find |
| Enter, Shift+Enter | Next or previous match, in the find field |
| Ctrl+Z, Ctrl+Shift+Z | Undo, redo |
| Escape | End a rename, close the find bar, or close the notepad |

## What is kept

The notes are in `~/.local/state/proscenio/notes/` (under `$XDG_STATE_HOME` when that is set):

| File | What |
|---|---|
| `<id>.txt` | The text of one note, UTF-8 |
| `index.json` | The order of the tabs, the open tab, and for each note its custom title, its last first-line title and the cursor position |

The index looks like this:

```json
{
  "active": 3,
  "next": 4,
  "notes": [
    { "id": 1, "cursor": 14, "derived": "Groceries" },
    { "id": 3, "cursor": 0, "title": "Plans" }
  ]
}
```

- **Autosave.** A note is written 500 ms after the last keystroke, and at the latest 5 s after the
  first keystroke of a stretch of typing, and when the notepad closes. Every write goes to a
  temporary file next to the target and replaces it by a rename, on a worker thread, so a crash
  leaves the old or the new file whole.
- **Restart.** The open tab and the cursor position of every note come back after a restart of the
  shell. Only the open note is read at start; the others are read when their tab is first opened.
- **Limits.** A note file over 2 MiB, or one that is not valid UTF-8 or contains a NUL byte, is not
  opened: its tab shows a message instead of the editor, and the file is never overwritten. Closing
  the tab deletes it. A note that could not be read is read again when its tab is clicked.
- **Recovery.** A missing or broken `index.json` is rebuilt from the `<id>.txt` files, in order of
  their number, which loses the custom titles and the open tab. A file the index does not list is
  added to it.

## Settings

| Setting | Key | Default |
|---|---|---|
| Notepad button in the bar | `bar.utilButtons.showNotepad` | off |

The switch is on **Appearance › Bar › Utility buttons**; see
[Appearance settings](settings-appearance.md).
