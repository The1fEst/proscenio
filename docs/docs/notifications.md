---
title: Notifications
sidebar_label: Notifications
description: proscenio as the session's notification server — what it accepts and keeps, where it saves them, the popups and their timeouts, the unread count, Do Not Disturb, per-app rules, and the cards notifications are shown on.
---

# Notifications

proscenio is the session's notification server. A notification pops up in the top-right corner of
the screen, stays in the [sidebar](sidebar.md)'s list until it is dismissed, and is saved to disk,
so the list survives a restart.

## The server

proscenio owns `org.freedesktop.Notifications` on the session bus and serves the Desktop
Notifications interface, version 1.2, at `/org/freedesktop/Notifications`:

| Member | Does |
|---|---|
| `Notify` | adds a notification, or replaces the one whose ID is given as `replaces_id` |
| `CloseNotification` | removes a notification, as its sender asks |
| `GetCapabilities` | returns `actions`, `body`, `body-hyperlinks`, `body-images`, `body-markup`, `icon-static`, `inline-reply` and `persistence` |
| `GetServerInformation` | returns `proscenio`, `fEst`, `0.1` and `1.2` |
| `NotificationClosed` | signals a removal, with reason 1 when it expired, 2 when the user dismissed it, 3 when `CloseNotification` closed it |
| `ActionInvoked` | signals that the user clicked one of the notification's actions |
| `NotificationReplied` | signals the text the user sent from the notification's reply field |

The name is requested without taking it from its owner: while another notification daemon holds it,
proscenio waits in the bus queue and takes over when that daemon exits.

:::note

Of the body markup, the cards show bold, italic, underline, strikethrough, links and images. A link
keeps only its `href`, shows in the primary color and opens in the default handler for its address;
a link without an `href` appears as its text. An `<img>` reads as its `alt` text in the line, and an
expanded card shows the image itself under the body, scaled down to fit 300 × 200 px, when its `src`
is a local file, given as an absolute path or a `file:` URI; images from anywhere else are not
fetched. A notification's own picture comes through the `image-data` or `image-path` hint instead,
in the icon.

:::

## What a notification carries

| Field | Comes from |
|---|---|
| ID | proscenio counts on from the highest ID in the saved list; a notification sent with `replaces_id` keeps that ID and moves to the top |
| app name | `app_name`; it also groups the notifications and keys the per-app rules |
| app icon | `app_icon`; when that is empty, the icon of the desktop file named by the `desktop-entry` hint, else of a desktop file matching the app name by file name, `StartupWMClass` or `Name` |
| summary and body | as sent, with surrounding whitespace trimmed |
| image | pixels in the `image-data`, `image_data` or `icon_data` hint, saved as a PNG; otherwise the `image-path` or `image_path` hint, where a `file:` URI becomes a path and anything else is a path or an icon name |
| actions | the identifier and label pairs from `actions`, except `inline-reply` |
| reply | the `inline-reply` action, if sent, with the `x-kde-reply-placeholder-text` hint |
| urgency | the `urgency` hint: 0 low, 1 normal (the default), 2 critical |
| transient | the `transient` hint |
| timeout | `expire_timeout` |
| time | the moment it arrived |

## Storage

The list is saved to `~/.cache/proscenio/notifications.json` (`$XDG_CACHE_HOME/proscenio/` when that
variable is set) and rewritten on every arrival and every removal. It is a JSON array:

```json
[
  {
    "actions": [{ "identifier": "default", "text": "Open" }],
    "appIcon": "org.gnome.Nautilus",
    "appName": "Files",
    "body": "3 files copied",
    "image": "",
    "notificationId": 12,
    "summary": "Copy finished",
    "time": 1791158400000,
    "urgency": "1"
  }
]
```

`time` is in milliseconds since the Unix epoch, and `urgency` is the number as a string. The image of
a notification that sent raw pixels is `~/.cache/proscenio/notifications/<id>.png`, deleted with its
notification.

When the shell starts, it reads the list back. Restored notifications do not pop up and have no
action buttons, and those saved without an icon get the desktop file lookup again.

## Popups

Each monitor has a popup surface on the overlay layer, namespace `proscenio:notificationPopup`,
anchored to the top-right corner, 410 px wide and as tall as its cards. Only one monitor shows
popups: Hyprland's focused monitor, following the focus as it moves. With
`notifications.forceMonitor.enable` on, it is the monitor named by `notifications.forceMonitor.name`
instead, as long as that monitor is connected.

Popups are hidden while the screen is locked. They are grouped by app like the sidebar's list, and
hold only the notifications that are still popping up.

A notification does not pop up while a sidebar is open, while Do Not Disturb is on, while the screen
is shared with `notifications.hideWhileSharing` on, or when its app has popups turned off. It still
goes to the sidebar's list, unless it is transient: then it is dropped.

The screen counts as shared while Hyprland reports a screencast, its `screencast` event: an app
capturing the screen through the desktop portal, as meeting apps, browsers and OBS do, or a
recorder such as `wf-recorder`, the shell's own recordings included. When sharing starts, every
popup ends at once, as when a sidebar opens. Hyprland reports a screencast off between frames, so
sharing counts as over only once it has been off for 3 s.

```toml
[notifications]
silent = false        # Do Not Disturb
hideWhileSharing = true
timeout = 7000        # ms, for senders that ask for no time of their own
quietApps = []        # apps that never pop up
forgottenApps = []    # apps whose notifications are dropped once their popup ends

[notifications.forceMonitor]
enable = false
name = "DP-1"
```

These are on the **Notifications** page of the [settings window](settings.md): **Do not disturb**,
**Hide popups while sharing the screen**, **Stays on screen for (ms)** (1000 to 60000), **Always on one display** with its monitor list, and a
pair of switches per app.

### Timeouts

| `expire_timeout` | The popup stays |
|---|---|
| `-1`, the usual value, or anything below 0 | `notifications.timeout` ms, 7000 by default |
| above 0 | that many ms |
| 0 | until it is dismissed or a sidebar opens |

When the time runs out, a notification leaves the screen and stays in the sidebar's list; a
transient one is removed, with reason 1. Hovering a popup card pauses the timers of every
notification in it, and leaving the card restarts each timer with its whole timeout.

Opening a sidebar ends every popup at once, and removes the transient ones.

## The unread count

Every notification that pops up adds one to the unread count, unless it is transient. A notification
that cannot pop up never counts. Opening a sidebar sets the count back to zero.

The [bar](bar.md) shows a bell while there are unread notifications or Do Not Disturb is on. Unread
notifications put a dot on the bell, or their number with
`bar.indicators.notifications.showUnreadCount` on: **Unread indicator: show count** under
**Appearance › Bar**, see [Appearance settings](settings-appearance.md). With Do Not Disturb on, the
bell is paused and carries no badge.

## Do Not Disturb

Do Not Disturb is `notifications.silent`. It can be switched in three places: the **Do not disturb**
switch in the settings window, the `notifications_paused` button under the sidebar's notification
list, and the Notifications [quick toggle](quick-toggles.md). While it is on, notifications still
arrive and are kept, but they do not pop up and do not count as unread. A transient notification that
arrives then is dropped at once.

## Per-app rules

The Notifications page lists every app that has ever sent a notification, by app name; the names are
kept in `~/.local/state/proscenio/states.json` under `notificationApps`. Each app has two switches:

| Switch | Turned off |
|---|---|
| **Pop up** | the app goes in `notifications.quietApps`: its notifications go to the sidebar's list only |
| **Keep** | the app goes in `notifications.forgottenApps`: its notifications are treated as transient, so they are removed when their popup ends, never count as unread, and are dropped at once when they cannot pop up |

## Grouping

Notifications are grouped by app name. A group's time is that of its newest notification, and the
newest group comes first. Inside a group the newest notification comes first. The group's icon is
the app icon of the first notification that has one.

## The cards

The popups and the sidebar's list show the same cards, one per app. A popup card has a shadow and a
background color of its own.

### A collapsed card

A card starts collapsed and is at most 80 px tall, or 117 px when its newest notification takes a
reply. Its top line holds the summary when the group has one notification, or the app name when it
has several; how long ago the newest arrived; and the expand button, which shows the count when
there are several. Under it are the two newest notifications, one line each: the summary, then the
body. A group of one shows only the body there, since its summary is on the top line. The second
line is faded when the group has more than two. The newest notification's reply field, if it takes
one, sits under its line.

### An expanded card

An expanded card lists all its notifications, each with its body wrapped in full, the images of the
body under it, a row of buttons, and its reply field if it takes one. The buttons are `close`, the
sender's own actions, and `content_copy`, which copies the body text and turns into `inventory` for
1.5 s. With several notifications, each sits on its own background, tinted for a critical one.

### Replying

A notification sent with an `inline-reply` action has a reply field with a `send` button, shown
without expanding the card. Its placeholder is the `x-kde-reply-placeholder-text` hint, else the
action's label, else **Reply**. Enter or `send` signals `NotificationReplied` with the text, unless
it is blank, and removes the notification with reason 2. Escape empties the field; in an empty field
it does what it does elsewhere, such as closing the sidebar.

A popup never takes the keyboard by itself, not even under the pointer: a click in a reply field
gives it the keyboard, and it gives the keyboard back when the pointer leaves the popups, Escape
empties the field, or the field goes. While a field holds text, its notification's popup timer
stops, and hovering no longer restarts it. The text is kept while the notification exists, so the
field still holds it when the card is rebuilt, as when the same app sends another notification.

### The icon

The icon is a 38 px circle in the secondary container color. When the group holds a critical
notification, it is instead a burst shape, picked at random, in the primary container color. Inside
it, the first of these that applies:

1. for a group of one notification with an image, the image, cropped round, with the app icon, if
   any, as a small badge at the bottom right;
2. an app icon given as a file path, filling the circle;
3. an app icon given as an icon name;
4. a Material symbol guessed from the summary: `power` for "battery" or "power", `screen_record` for
   "record", `screenshot_monitor` for "screenshot", `update` for "update", `queue_music` for
   "music", `folder_copy` for a summary that starts with "file", and a few more; otherwise `chat`,
   or `priority_high` for a critical notification.

### Text

How long ago a notification arrived reads `Now` within the first minute, `5m` or `3h` the same day,
`Yesterday`, and the month and day before that.

A notification from a Chromium-based browser (Brave, Chrome, Chromium, Vivaldi, Opera, Microsoft
Edge) whose body starts with a link to the sending site loses that first paragraph.

### Input

| Input | Does |
|---|---|
| Click the expand button, or right-click the card | expands or collapses the card |
| Middle-click the card | dismisses the whole group |
| Drag a collapsed card sideways past 70 px | dismisses the whole group |
| Drag one notification of an expanded card sideways past 70 px | dismisses that notification |
| `close` | dismisses that notification |
| An action button | sends `ActionInvoked` to the sender, then removes the notification |
| Click a reply field, on a popup | gives the popups the keyboard until the pointer leaves them |
| A link in the body | opens it in the default handler |
| `content_copy` | copies the body text |
| Hover, on a popup | pauses the group's timers |

A drag starts only once the pointer has moved past GTK's drag threshold, so a press on a button stays
a click. The cards next to the dragged one follow it part of the way, and letting go short of 70 px
springs them all back. A dismissed card slides out before it is removed. Dismissing a notification
signals `NotificationClosed` with reason 2 and deletes its saved image.

The `delete_sweep` button under the sidebar's list dismisses every notification.

## Notifications from the shell

proscenio sends its own notifications, such as low battery, the Pomodoro timer, screen recordings,
the microphone, the weather and the wallpaper switcher, with the same `Notify` call on the session
bus, so they reach whichever server owns the name.

It also reports USB devices. When a removable USB device is plugged in or pulled out, it sends
"USB Device Detected" or "USB Device Removed" naming the vendor and model, read from sysfs or the
udev hardware database. These are low-urgency notifications from the app `Shell`; each one replaces
the last, and each plays the sound theme's `device-added` or `device-removed` sound while
`sounds.devices` is on, the default.
