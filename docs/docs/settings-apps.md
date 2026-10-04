---
title: Apps settings
sidebar_label: Apps
description: The Apps page and its File types and Window rules subpages, covering default applications, which app opens each file type, per-application Hyprland window rules and the commands the shell runs.
---

# Apps settings

The **Apps** page chooses the application for each kind of file and link, gives single
applications their own window behavior, and sets the commands the shell's own buttons run. Two
subpages hang off it: **File types**, for every file type one at a time, and **Window rules**.

## Default Apps

One box per kind of file or link: Web, Mail, Calendar, Music, Video, Photos, Text and Files. A box
lists the installed applications that the association files or the MIME cache name for the kind's
main type or one of its parent types, leaving out removed associations, each by the name its desktop
file gives. The one in use is the main type's default, or failing that the first application listed;
the box starts with **Not set** when there is none, and ends with **Other…**.

**Other…** opens a dialog over the settings window with every application that shows in menus,
sorted by name, a search field that matches the name, the desktop ID and the executable, and
**Cancel**. Picking one sets it for the kind.

| Kind | Main type | Also owns |
|---|---|---|
| Web | `x-scheme-handler/http` | `x-scheme-handler/https`, `text/html`, `application/xhtml+xml` |
| Mail | `x-scheme-handler/mailto` | |
| Calendar | `text/calendar` | |
| Music | `audio/mpeg` | a list of common audio types, and every other `audio/*` type |
| Video | `video/mp4` | a list of common video types, and every other `video/*` type |
| Photos | `image/png` | a list of common image types, and every other `image/*` type |
| Text | `text/plain` | a list of text and data types, every other `text/*` type, and every type that descends from `text/plain`, such as JSON, YAML, XML and scripts |
| Files | `inode/directory` | |

A type another kind lists stays with that kind. The "every other" types come from the installed
shared-mime-info database (`mime/types` and `mime/subclasses` under each XDG data folder).

### What a choice writes

The page reads the association files the way the XDG MIME applications spec lays them out:
`mimeapps.list` and `<desktop>-mimeapps.list` in `$XDG_CONFIG_HOME`, in each `$XDG_CONFIG_DIRS`
folder and in each `applications` data folder, and the `mimeinfo.cache` files.

A choice is written to `~/.config/mimeapps.list` in one pass. Every type the kind owns gets the
application as its only entry under `[Default Applications]` and as the first under
`[Added Associations]`, whether or not the application's desktop file declares that type. In the
same pass, entries in those two groups that name a desktop file that is not installed are dropped,
and a type left with none loses its line. Other lines, and `[Removed Associations]`, stay as they
were.

The page reads off the main thread, again after each choice, and again a second after the set of
installed applications changes.

## File types

Under **File types**, the **Every file type** link row opens the subpage. Its search field, which
has focus when the subpage opens, looks through every type GIO has registered with a `/` in its
name. Each typed word has to appear in the type or in its description.

Until something is typed, a line counts the known types. Then up to 40 matches show, each with the
type's icon, its description over the type, the application that opens it now (or "Nothing opens
it") and a chevron. A line under them counts the matches left out, or says that nothing matches.

Pressing a type opens **Open DESCRIPTION with**:

- **Recommended** lists GIO's recommended applications for the type, and **Also opens it** the
  rest of its fallback applications, the one in use marked with a check. A pick makes it the default
  for that type alone, in `~/.config/mimeapps.list`.
- **Reset** drops the user's associations for the type.
- **Other application…** opens the application dialog of Default Apps, for any installed
  application.
- **Cancel** closes the dialog.

A type no installed application declares shows "No installed application says it opens this type"
in place of the two lists. After a change, the list shows the new default.

## Window rules

Under **Window rules**, the **Every window rule** link row opens the subpage. Its rules are the ones
the settings window owns: one line each in `~/.config/hypr/settings/apps.lua`, in this shape.

```lua
hl.window_rule({ match = { class = "^(firefox)$" }, float = true })
```

**What each application's windows do** lists them, each with the window class, what the rule does,
and a remove button; "No rules yet" stands in while there are none. Lines of any other shape in the
file are left alone and not listed.

**Add a rule** takes:

1. the window class, the one `hyprctl clients` reports as `class`; a box under the field lists the
   classes of the windows open now and fills the field with the one picked, and follows windows
   opening and closing;
2. the kind of rule;
3. a value, for the two kinds that take one;
4. **Add rule**, live once there is a class.

| Kind | Rule written |
|---|---|
| Always floating | `float = true` |
| Always tiled | `tile = true` |
| Pinned to every workspace | `pin = true` |
| Opens fullscreen | `fullscreen = true` |
| No blur behind it | `no_blur = true` |
| No shadow | `no_shadow = true` |
| Square corners | `rounding = 0` |
| Draw without waiting for the screen | `immediate = true` |
| Takes focus when it asks | `focus_on_activate = true` |
| Never takes focus when it asks | `focus_on_activate = false` |
| Opacity (%) | `opacity = 0.01` to `opacity = 1`, from 1–100 %; 100 % when the field is empty, zero or not a number |
| Opens on workspace | `workspace = 3`, or a quoted name such as `workspace = "special:magic"` |

A class has at most one rule of each name: adding one replaces the class's earlier line with the
same rule, so **Takes focus when it asks** and **Never takes focus when it asks** replace each
other. Adding or removing a rule reloads Hyprland. The two focus rules make an exception for one
application to the **Let apps take focus when they ask for it** switch on
[Multitasking settings](settings-multitasking.md).

## Commands

**What the shell's own buttons open**:

| Field | Key | Default | Used by |
|---|---|---|---|
| Terminal | `apps.terminal` | `kitty -1` | the launcher, for applications whose desktop entry asks for a terminal and for `sudo` commands |
| Task manager | `apps.taskManager` | | stored only |
| System update | `apps.update` | | the bar's System updates button |

The keys are in `~/.config/proscenio/config.toml`. When a command's program is not installed, a
notice names the program and its field, and it updates as the fields change.
