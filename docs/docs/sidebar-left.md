---
title: Left sidebar
sidebar_label: Left sidebar
description: The panel on the left edge of the screen with the notepad, which has tabs, autosave to plain files, find, and line and character counts, and a privacy section listing the applications using the microphone, screen or camera.
---

# Left sidebar

The left sidebar is a full-height panel on the left edge of the screen. From top to bottom it holds
the notepad, for scratch notes, and the [privacy section](#privacy), which is there while the
microphone, the screen or the camera is in use. The notepad is a row of tabs with a plain-text
editor under it, and everything typed is saved by itself. The source is in `src/panels/leftsidebar/`,
with the notes and their files in `src/services/session/notes/`.

## Opening and closing

Every monitor the shell runs on has a left sidebar of its own, and they show the same notes.

| To open or close it | Acts on |
|---|---|
| A left click anywhere on the bar's [left region](bar.md#the-left-region-brightness), privacy chips included, or on the top part of a vertical bar | The screen clicked |
| A left click on a left [screen corner](screen-corners.md#corner-regions) | The screen clicked |
| The notepad button in the [bar](bar.md#utility-buttons), off by default | Hyprland's focused monitor |
| `proscenio ipc call sidebarLeft toggle`, or `open`, `close` | Hyprland's focused monitor |
| The global shortcut `sidebarLeftToggle`, which has no default bind | Hyprland's focused monitor |
| The launcher [action](overview.md#actions) `/notepad` | Hyprland's focused monitor |

[IPC](ipc.md) and [Shortcuts](shortcuts.md) show how to call and bind these.

While the left sidebar is open, the privacy chips in the bar show their pressed fill, the way the
indicators button at the right end does while the [sidebar](sidebar.md) is open.

The left sidebar closes on:

- **Escape.** With the delete dialog open, the first Escape closes the dialog. Otherwise, it ends a
  tab rename, then closes the find bar, and then closes the left sidebar.
- **A click outside it**, on a window, the bar or the wallpaper. This is Hyprland's
  `hyprland-focus-grab-v1`; see [Architecture](architecture.md#focus-grabbing). Only one panel holds
  a grab at a time, so opening the left sidebar closes the right sidebar or the calendar if one is
  open, and opening one of them closes the left sidebar.

Closing saves every unsaved note at once. The content is built once and kept; about a second after
the left sidebar closes, its window gives back its surface and buffers.

## The window

| Property | Value |
|---|---|
| Layer | top |
| Namespace | `proscenio:sidebarLeft`, for Hyprland layer rules |
| Anchors | top, left and bottom |
| Width | 480 px: the panel, 5 px of gap above, below and to the left, and 10 px on the right for its shadow |
| Exclusive zone | none of its own; it keeps clear of the bar's, so it starts below a top bar |
| Keyboard focus | on demand |

The window does not animate itself. A layer rule makes it slide in from the left edge:

```lua
hl.layer_rule({ match = { namespace = "proscenio:sidebarLeft" }, animation = "slide left" })
```

## Privacy

Below the editor, a rounded section like the editor's page lists what is using the microphone, the
screen or the camera right now. It has one block for each kind in use, in the order microphone,
screen sharing, camera, each headed by a badge in the kind's color (see the
[privacy indicator](bar.md#privacy-indicator)), with a row for each application: its icon, its name,
and what it uses. While nothing is in use the section is hidden, the way the sidebar hides its update
buttons while there is nothing to update.

It is filled when the left sidebar opens, which also asks for a fresh scan of the cameras and the
shared screens, and follows changes while it stays open.

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

## Adding a note to the calendar

The calendar button after the tabs (tooltip **Add to calendar**) opens a dialog that puts the open
note in the calendar as an event in [Evolution Data Server](installing.md) (EDS), so
GNOME Calendar and synced calendars show it too.

| Field | Value |
|---|---|
| Date | Day, month and year, today at first. A day past the end of the month becomes its last day |
| All day | On at first. Off enables the time and the duration |
| Time | Hour and minute, the next full hour at first |
| Duration (minutes) | 30 at first, up to 1440, for a timed event |
| Calendar | The enabled calendars of EDS that can take events, the first selected one at first. The read-only ones (`webcal`, `http`, `contacts` and `weather` backends) are not listed |

**Create** sends the event to EDS and closes the dialog. If EDS refuses it, its message shows in
the dialog and nothing is created. **Cancel**, Escape or a click outside closes the dialog.

The event has the note's tab title as its summary, the first 500 characters of the note as its
description, and a `URL` of `proscenio://note/<id>` with an `X-PROSCENIO-NOTE` property holding the
same id. An all-day event ends on the next date; a timed one is in the system time zone. The
[calendar](calendar.md#events) opens the note from the event's card. Changing or deleting the event
is done in GNOME Calendar, and the note does not track its events.

## Keys

| Key | Action |
|---|---|
| Ctrl+N | New note |
| Ctrl+W | Close the open note |
| Ctrl+Page Up, Ctrl+Page Down | Previous or next tab |
| Ctrl+F | Find |
| Enter, Shift+Enter | Next or previous match, in the find field |
| Ctrl+Z, Ctrl+Shift+Z | Undo, redo |
| Escape | End a rename, close the find bar, or close the left sidebar |

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
  first keystroke of a stretch of typing, and when the left sidebar closes. Every write goes to a
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
