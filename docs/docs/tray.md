---
title: System tray
sidebar_label: System tray
description: The tray in the bar, how proscenio hosts StatusNotifierItem icons itself, which items sit in the bar and which in the overflow menu, and how item menus work.
---

# System tray

The tray shows the status icons apps publish over StatusNotifierItem: chat clients, input methods,
sync tools. It sits in the right region of the [bar](bar.md), just inside the sidebar button, or in
the bottom part of a vertical bar above it.

```text
… (⌄)  [icon] [icon] [icon]  •  (indicators)
```

From left to right: the overflow button when some items are folded away, the items kept in the bar,
and a `•` that closes the row whenever there is at least one item.

## The tray host

proscenio is the session's tray watcher itself. It owns `org.kde.StatusNotifierWatcher` on the
session bus, replacing any current owner, and serves it at `/StatusNotifierWatcher`. Another
program that wants to be the watcher has to go.

| Request or property | What the watcher does |
|---|---|
| `RegisterStatusNotifierItem` with a bus name | registers the item at that name and `/StatusNotifierItem` |
| `RegisterStatusNotifierItem` with an object path | registers the item at that path on the caller's own bus name |
| an item whose bus name has no owner | is not registered |
| a bus name that leaves the bus | every item it registered is dropped |
| `RegisterStatusNotifierHost` | accepted |
| `IsStatusNotifierHostRegistered` | `true` |
| `ProtocolVersion` | `0` |

Each registration and removal emits `StatusNotifierItemRegistered` or
`StatusNotifierItemUnregistered`, and the bar's tray rebuilds its row from
`RegisteredStatusNotifierItems`. The bar also registers itself as a host, under
`org.kde.StatusNotifierHost-<pid>`.

## Pinned and unpinned items

Every item is either pinned, shown in the bar, or unpinned, kept in the overflow menu. The list
`tray.pinnedItems` holds item IDs, and `tray.invertPinnedItems` decides what the list means:

| Make icons pinned by default (`tray.invertPinnedItems`) | Items in the bar | Items in the overflow menu |
|---|---|---|
| on (`true`, default) | every item not in the list | the items in the list |
| off (`false`) | only the items in the list | every other item |

With the defaults and an empty list, every item sits in the bar.

The quickest way to move an item is its menu: the first entry reads **Unpin** for an item in the bar
and **Pin** for one in the overflow, and choosing it adds or removes the item's ID in
`tray.pinnedItems`. The row updates at once.

To edit the list by hand, use the **Tray** section of **Appearance → Bar** in the
[settings window](settings-appearance.md). Its field is titled **Unpinned items** or **Pinned items**
to match the switch above it and takes comma-separated IDs. **Show item IDs in tooltips** helps find
an item's ID.

Two more rules decide what shows:

- **Hide passive items** (`tray.filterPassive`, on by default) leaves out items whose status is
  `Passive`, from the bar and from the overflow menu alike.
- On a horizontal bar on a screen 1200 logical pixels wide or narrower, every item moves into the
  overflow menu. A vertical bar never folds its tray.

## The overflow button

When at least one item is unpinned, a chevron button leads the row. Clicking it opens a popup with
the unpinned items in a grid, as close to square as the count allows: four items make two columns,
five make three. The chevron turns half a turn over 200 ms while the popup is open. On a vertical bar
it points sideways, away from the screen edge.

The items in the popup behave like the ones in the bar. A click outside the popup closes it.

## An item

Each item is a 20 px icon, taken from the item's properties in this order:

1. `AttentionIconName`, then `AttentionIconPixmap`, while the item's status is `NeedsAttention`;
2. otherwise `IconName`, looked up in the icon theme, with the item's `IconThemePath` added to the
   theme's search path when it sets one;
3. otherwise the largest image in `IconPixmap`;
4. otherwise `image-missing`.

Any signal the item emits, such as `NewIcon` or `NewStatus`, redraws its icon.

With **Tint icons** (`tray.monochromeIcons`, on by default) each icon is desaturated by 80 % and then
washed 10 % toward the bar's text color, so the tray reads as one muted row. Turn it off to show the
icons in their own colors.

Pointing at an item shows a tooltip at once, below the bar on a horizontal bar. It reads the item's
tooltip title, or its `Title`, or its ID, followed by ` • ` and the tooltip description when there
is one. With **Show item IDs in tooltips** (`tray.showItemId`) the ID follows on its own line in
brackets.

| Button | Action |
|---|---|
| Left | nothing; the item's actions are in its menu |
| Right | opens the item's menu, if it publishes one |

A click on a tray icon does not toggle the sidebar, as a click on the empty part of the right region
does.

## The menu

Item menus are read over `com.canonical.dbusmenu`. Before the menu opens, and before each submenu,
the shell calls `AboutToShow` for it and then `GetLayout`. The menu opens below the bar, or beside a
vertical bar, on a rounded card in the bar's color.

- The top level starts with the **Pin** or **Unpin** entry and a thin rule under it.
- A submenu starts with **Back** and the same rule. Right-click, or the back mouse button, also goes
  back one level.
- An entry with children shows a chevron and opens its submenu in place. The menu resizes to fit
  over 300 ms and each page fades in over 200 ms.
- Choosing any other entry sends the item a `clicked` event and closes the menu.
- A separator entry is a thin line.

Each row has a label and, when needed, two columns in front of it: one for a check mark or a radio
button, one for an icon. When one entry of a page needs a column, every entry of that page gets it,
so the labels line up. Icons come from the entry's `icon-name` or its `icon-data` image.

Entries with `visible` set to false are left out. The `enabled` property is not read: an entry the
app marks disabled looks and acts like the rest. Mnemonic underscores are removed from labels.

The menu and the overflow popup close when you click anywhere outside them.

## Settings

The tray options are on **Appearance → Bar → Tray**, under `[tray]` in
`~/.config/proscenio/config.toml`. Changes apply at once.

| Setting | Key | Default |
|---|---|---|
| Make icons pinned by default | `invertPinnedItems` | `true` |
| Tint icons | `monochromeIcons` | `true` |
| Hide passive items | `filterPassive` | `true` |
| Show item IDs in tooltips | `showItemId` | `false` |
| Unpinned items / Pinned items | `pinnedItems` | `[]` |

## Where it lives

| Source | Holds |
|---|---|
| `src/services/statusnotifierwatcher.rs` | the `org.kde.StatusNotifierWatcher` service |
| `src/panels/bar/tray.rs` | the row, the overflow button and popup, item icons and tooltips |
| `src/panels/bar/traymenu.rs` | the item menu |
| `src/platform/dbusmenu.rs` | the `com.canonical.dbusmenu` client |
