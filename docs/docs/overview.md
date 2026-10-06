---
title: Overview and launcher
sidebar_label: Overview and launcher
description: The overview's search field, which launches apps, calculates, runs commands and actions and searches the clipboard and emoji, and its grid of workspaces with live window pictures.
---

# Overview and launcher

The overview opens at the top of the focused monitor. It holds a search field and, under it while
the field is empty, a grid of workspaces with a live picture of every window. Typing hides the grid
and lists results instead.

## Opening it

| Global shortcut | `proscenio ipc call search …` | Does |
|---|---|---|
| `searchToggle`, `overviewWorkspacesToggle` | `toggle`, `workspacesToggle` | Opens or closes the overview with an empty field |
| — | `open` | Opens it |
| `overviewWorkspacesClose` | `close`, `workspacesClose` | Closes it |
| `overviewClipboardToggle` | `clipboardToggle` | Opens it with the clipboard prefix typed |
| `overviewEmojiToggle` | `emojiToggle` | Opens it with the emoji prefix typed |

A right-click on the bar's [workspaces](workspaces.md) and the [dock](dock.md)'s last button open it
too. Opening it clears the last search, except through the clipboard and emoji shortcuts; pressing
one of those again while the overview they opened is showing closes it. Escape or a click outside
closes the overview. [Shortcuts](shortcuts.md) and [IPC](ipc.md) explain how to bind and call these.

## Searching

The field reads "Search, calculate or run". The symbol at its left changes shape with the prefix the
query starts with, so the kind of search shows at a glance.

Outside the clipboard and emoji prefixes, the results come in this order:

1. the math result, when the query starts with a digit or the math prefix; or the command, when it
   starts with the command prefix;
2. apps whose name matches;
3. launcher actions, when the query starts with the action prefix;
4. with `search.prefix.showDefaultActionsWithoutPrefix` on (the default), the command and the math
   result that were not already first.

| Prefix | Default | Key | Results |
|---|---|---|---|
| App | `>` | `search.prefix.app` | Apps matched against the text after the prefix |
| Action | `/` | `search.prefix.action` | Launcher actions |
| Clipboard | `;` | `search.prefix.clipboard` | The clipboard history, and nothing else |
| Emoji | `:` | `search.prefix.emojis` | Emoji, and nothing else |
| Math | `=` | `search.prefix.math` | The math result first |
| Shell command | `$` | `search.prefix.shellCommand` | The command first |

The prefixes and the options below are on the **Search** page of the [settings](settings.md).

### Matching

Apps, emoji and clipboard entries are matched fuzzily: the letters of the query have to appear in
order, case and accents do not matter, and words separated by spaces are matched on their own. The
best matches come first, and the matched letters are underlined in the accent color.
`src/core/fuzzy.rs` implements the scoring of the fuzzysort library.

`search.sloppy` (**Use Levenshtein distance-based algorithm instead of fuzzy**) ranks by edit
distance instead. It forgives typos but handles acronyms badly, and it looks at the newest 100
clipboard entries only.

### Apps

Every desktop entry that is neither `NoDisplay` nor `Hidden` is an app. Up to four of its desktop
actions, such as "New Window", sit on its row as small buttons. An app that runs in a terminal opens
in `apps.terminal` (`kitty -1` by default) with `-e`. Apps start in a transient systemd scope of
their own.

### Math

`qalc -t` evaluates the query, without the math prefix, once no key has been typed for
`search.nonAppResultDelay` milliseconds (30 by default). The last line `qalc` prints becomes the
result, and choosing it copies it to the clipboard. Without `qalc` there is no math result.

### Commands

The command row runs the query, without the command prefix, through `bash -c`. A leading `sudo`
becomes `pkexec`, so the command asks for the password through polkit and runs as root in `/root`,
with the clean environment `pkexec` gives.

### Actions

An action is shown while its name, with the action prefix in front, starts with the query or the
query starts with it. Whatever follows the first space is the action's argument: `/todo buy milk`.

| Action | Does |
|---|---|
| `accentcolor` | Sets the accent color to a hex color such as `#ff6f00`, removes it with `clear`, or picks one from the screen with `hyprpicker` when given nothing |
| `dark`, `light` | Switches the color scheme, keeping the wallpaper |
| `superpaste` | Pastes the last N clipboard entries, oldest first, by copying each and pressing Ctrl+V with `ydotool`; `/superpaste 4i` pastes the last four images |
| `todo` | Adds a task to the to-do list of the [calendar](calendar.md) |
| `wallpaper` | Opens the [wallpaper selector](wallpaper-selector.md) |
| `wipeclipboard` | Deletes the whole clipboard history |

Every file in `~/.config/proscenio/actions/` whose name does not start with a dot is an action too,
named after the file without its extension. It runs as a program, so it has to be executable, and
it gets the argument's words as its arguments.

### Clipboard

The clipboard prefix lists the history that `cliphist` keeps, newest first; without `cliphist` it
finds nothing. Every entry has a **Copy** and a **Delete** button, the entry that is on the clipboard
carries a check mark, and an image entry shows the image. Choosing an entry copies it.

With `workSafety.enable.clipboard` on, and while the network's name contains one of the words in
`workSafety.triggerCondition.networkNameKeywords`, an image entry is blurred under "Image hidden"
when an entry next to it contains one of the words in `workSafety.triggerCondition.linkKeywords`.
These are the **Work safety** settings on the Privacy & Security page.

### Emoji

The emoji prefix searches the emoji listed after the `### DATA ###` line of
`~/.config/hypr/hyprland/scripts/fuzzel-emoji.sh`, one per line followed by its name. Without that
file it finds nothing. Choosing an emoji copies it.

## Keys

| Key | Does |
|---|---|
| Enter in the field | Runs the first result |
| Down in the field | Moves into the list, to the second result |
| Up and Down in the list | Move the selection; Up on the first result goes back to the field |
| Enter in the list | Runs the selected result |
| Tab | Writes the selected result's name into the field (the first one, from the field) |
| Shift+Delete | Deletes the selected clipboard entry |
| Any other typing | Goes to the field, wherever the selection is; Backspace and Ctrl+Backspace delete from it |
| Escape | Closes the overview |

Clicking a result runs it and closes the overview. The small buttons on a row run their own action
and leave the overview open.

## The workspace grid

The grid shows `overview.rows` × `overview.columns` workspaces, 2 × 5 by default: the group that
holds the focused workspace. With ten to a group, workspace 13 shows workspaces 11 to 20.

- Each workspace is drawn as its monitor's usable area, without what bars reserve, scaled by
  `overview.scale` (0.18 by default), with its number in the middle.
- Each window of those workspaces is drawn where it is, with a live picture and its app icon, in
  the middle with `overview.centerIcons` on (the default) or in its top-left corner. Windows on
  other monitors are dimmed.
- An outline in the secondary color marks the focused workspace and moves with the focus.
- Hovering a window shows its title and class, and `[XWayland]` for an XWayland window.

| Mouse | Does |
|---|---|
| Click on a workspace | Goes to it and closes the overview |
| Click on a window | Focuses it and closes the overview |
| Middle-click on a window | Closes the window |
| Drag a window onto another workspace | Moves it there; the focus stays where it is |
| Drag a floating window within its workspace | Moves it on screen |

`overview.orderRightLeft` and `overview.orderBottomUp` number the workspaces from the right or from
the bottom. With `overview.enable` off, the overview is the search field alone.

The grid follows Hyprland's events while it is shown. Its pictures come from the same capturer as
the dock's previews, described in [The dock](dock.md): the GPU path with a `wl_shm` fallback, at most
30 frames a second per window, never the cursor. A window whose capture fails three times in a row
keeps its last picture, or none. The capturer starts when the grid shows and stops when it hides.

## Settings

The grid's settings are under **Multitasking › Overview** in the
[settings](settings-multitasking.md), and the search's on the **Search** page. All of them apply at
once.

| Key in `config.toml` | Default | Setting |
|---|---|---|
| `overview.enable` | `true` | Enable |
| `overview.centerIcons` | `true` | Center icons |
| `overview.scale` | `0.18` | Scale (%) |
| `overview.rows` | `2` | Rows |
| `overview.columns` | `5` | Columns |
| `overview.orderRightLeft` | `false` | Left to right / Right to left |
| `overview.orderBottomUp` | `false` | Top-down / Bottom-up |
| `search.sloppy` | `false` | Use Levenshtein distance-based algorithm instead of fuzzy |
| `search.nonAppResultDelay` | `30` | Non-app result delay (ms) |
| `search.prefix.showDefaultActionsWithoutPrefix` | `true` | Show default actions without a prefix |
| `search.prefix.*` | see above | Prefixes |
| `apps.terminal` | `kitty -1` | Terminal, on the [Apps](settings-apps.md) page |

## How it works

The overview is one surface per monitor on the `top` layer, namespace `proscenio:overview`, that
takes the keyboard while it is open. `src/panels/overview/mod.rs` draws the field and the results,
`launcher.rs` next to it builds the results, and `grid.rs` draws the grid. Results are built 15 rows
at a time, and more as the list scrolls toward its end.

Opening the overview loads the app list and the clipboard history; the emoji table loads the first
time the emoji prefix is typed. Closing it drops all of that, along with the clipboard images, which
`cliphist decode` writes to `$XDG_RUNTIME_DIR/proscenio/cliphist/` while they are shown.
