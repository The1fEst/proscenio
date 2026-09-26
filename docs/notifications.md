# Notifications

Source: `services/Notifications.qml`, `GlobalStates.qml`, and the widgets in
`modules/common/widgets/Notification*.qml`. Read
[foundations.md](foundations.md) first.

The shell is the notification daemon. It owns
`org.freedesktop.Notifications`, keeps the list on disk, and feeds three
consumers: the unread badge in the bar (§bar.md 8.2), the list in the right
sidebar (§sidebar-right.md 7) and the on-screen popups.

---

## 1. The server

`Quickshell.Services.Notifications.NotificationServer` with

```qml
actionsSupported: true          bodySupported: true
bodyHyperlinksSupported: true   bodyImagesSupported: true
bodyMarkupSupported: true       imageSupported: true
inlineReplySupported: true      persistenceSupported: true
keepOnReload: false
```

which is the capability set reported to `GetCapabilities`.

`keepOnReload: false`: a shell reload drops the live notifications; only the
file survives.

## 2. What a notification carries

```
notificationId   the server id plus idOffset
actions          [{ identifier, text }]
appIcon          appName       body        summary
image            the resolved image-path, or the saved image-data
time             Date.now() at arrival, milliseconds
urgency          the enum as a string: "0" low, "1" normal, "2" critical
isTransient      hints.transient
hasInlineReply   inlineReplyPlaceholder
popup            whether it is still on screen
```

### 2.1 The id offset

Quickshell's ids restart at 1 every run. On load the service takes the
largest stored id into `idOffset` and adds it to every incoming server id, so
new ids do not collide with stored ones.

## 3. Storage

`~/.cache/quickshell/notifications/notifications.json` — a JSON array, pretty
printed with two-space indent, of

```json
{ "notificationId": 123, "actions": [], "appIcon": "…", "appName": "…",
  "body": "…", "image": "", "summary": "…", "time": 1789952243180,
  "urgency": "0" }
```

The file is rewritten on every arrival, discard and discard-all. Actions are
always written as an empty array, so a restored notification has no actions.

If the file is missing it is created empty; any other read error is logged and
the list is left alone.

## 4. Popups, timeouts and the unread count

```
popupInhibited = GlobalStates.sidebarRightOpen || silent
```

On arrival, when not inhibited: the notification is marked `popup`, a timer is
started unless `expireTimeout == 0`, and **`unread` is incremented**. When
inhibited, none of that happens — a notification that arrives while the
sidebar is open or while silent never counts as unread.

The timer's interval is `expireTimeout` when the sender gave a positive one,
otherwise `notifications.timeout` (default **7000 ms**). When it fires, a
transient notification is discarded outright and any other one merely stops
being a popup and stays in the list.

`unread` is reset by exactly one thing: opening the right sidebar.
`GlobalStates.onSidebarRightOpenChanged` closes the calendar, calls
`timeoutAll()` — which clears the `popup` flag on everything at once — and
then `markAllRead()`.

`silent` is `Config.options.notifications.silent`; toggling it writes the
config file. Both the `notifications` quick toggle and the sidebar's status
row flip it.

## 5. Acting on one

- **discard(id)** removes it from the list, rewrites the file, and tells the
  server to dismiss it, which notifies the sender.
- **discardAll()** empties the list and dismisses every tracked notification.
- **attemptInvokeAction(id, identifier)** finds the live notification, invokes
  the matching action, and discards it afterward — invoking always dismisses.
- **sendInlineReply(id, text)** likewise, then discards.

## 6. Grouping

The list is grouped by `appName` into

```
{ appName, appIcon, notifications: [...], time }
```

where the group's `time` is the latest time seen for that app, held in a
side table `latestTimeForApp` that is refreshed whenever the list changes and
pruned of apps with none left. `appNameList` is the group keys sorted
by that time, descending: the app with the newest notification is first. The
stored time only grows, so dismissing a group's newest member does not move
the group down.

## 7. The popups

`modules/ii/notificationPopup/NotificationPopup.qml` is a `PanelWindow` on
`WlrLayer.Overlay`, namespace `quickshell:notificationPopup`, anchored top,
right and bottom, `exclusiveZone: 0`, transparent, width
`Appearance.sizes.notificationPopupWidth` (**410**). It is visible only while
`popupList` is non-empty and the screen is unlocked.

Its screen is the one named by `notifications.forceMonitor` when that is
enabled, otherwise Hyprland's focused monitor.

```qml
mask: Region { item: listview.contentItem }
```

limits input to the list's content item (§7.5). The list is inset 4 px from
the top and right and is `elevationMargin · 2` narrower than the window,
leaving room for the shadows.

### 7.1 The list

`NotificationListView` is a `StyledListView` with `spacing: 3` over the group
names — `popupAppNameList` for the popup, `appNameList` for the sidebar — with
one `NotificationGroup` per app.

### 7.2 A group card

Background `colBackgroundSurfaceContainer` in the popup, `colLayer2` in the
sidebar; radius `Appearance.rounding.normal` (17); padding 10; a drop shadow
only in the popup.

```
implicitHeight = expanded ? content + 20 : min(80, content + 20)
```

animated with `elementMoveFast` (200 ms) only when collapsing: the
`Behavior` is switched off just before expanding, so opening snaps and closing
animates.

The row is the app icon (top-aligned) and a content column, spacing 10.

**The top line** shows the app name when the group holds more than one
notification, at `pixelSize.smaller` (12) in `colSubtext`, and the single
notification's summary otherwise, at `pixelSize.small` (15) in `colOnLayer2`.
Beside it the friendly time, then the expand button.

**The expand button** is a `RippleButton`, fully rounded, height
`fontSize + 8`, minimum width 30, background `mix(colLayer2, colLayer2Hover,
0.5)`. It carries the count when there is more than one, and a
`keyboard_arrow_down` that rotates 180° when open, over `elementMoveFast`.

**The items** are a nested list of the group's notifications **reversed** —
newest first — showing all of them when expanded and the first two otherwise,
with the second at `opacity: 0.5` when there are more than two. The spacing
between them is 5 expanded, 3 collapsed.

### 7.3 An item

Collapsed, an item is one line: the summary, then the body squeezed onto a
single elided line in `colSubtext`. The summary is hidden when the group has
one notification; the card's top line shows it.

Expanded, the item gets its own background — `colLayer3`, or
`mix(colSecondaryContainer, colLayer2, 0.35)` when critical — radius
`small` (12), padding 8, the body wrapped as rich text, and a row of actions.

The action row is always `close`, then the notification's own actions, then
`content_copy`; the two icon buttons take half the width each when there are
no actions of the sender's own. Each is a `RippleButton` of height 34 with
15 px side padding, radius 12, `colLayer4` normally and
`colSecondaryContainer` when critical. Copying swaps the icon to `inventory`
for **1500 ms**.

### 7.4 The app icon

`NotificationAppIcon` is a `MaterialShape` of **38 px**: a circle normally, or
one of `VerySunny` / `SoftBurst` picked at random when any notification in the
group is critical. It is `colSecondaryContainer`, or `colPrimaryContainer`
when critical.

Inside it, in order of preference: the notification's own image (only when
the group holds one notification), cropped to a circle, with the app icon as
a small badge at the bottom right (scale **0.49**); the app icon alone
(scale **0.8**, whether it is an icon name or a file path); or a Material
symbol guessed from the summary (scale **0.57**).

The guess is a keyword table — `reboot`→`restart_alt`, `record`→
`screen_record`, `battery`/`power`→`power`, `screenshot`→
`screenshot_monitor`, `welcome`→`waving_hand`, `time`→`scheduleb`,
`installed`→`download`, `configuration reloaded`/`config`→`reset_wrench`,
`unable`/`couldn't`→`question_mark`, `update`→`update`, `ai response`→
`neurology`, `control`→`settings`, `upsca`→`compare`, `music`→`queue_music`,
`install`→`deployed_code_update`, `input`/`preedit`→`keyboard_alt`, a summary
starting with `file`→`folder_copy` — falling back to `chat`, which a critical
notification turns into `priority_high`.

### 7.5 Input

- **Hover** cancels the timeout of every notification in the group; leaving
  times them all out, which drops the popup at once. In the popup window none
  of this fires: the input region is `Region { item: listview.contentItem }`,
  and a vertical `ListView` gives its content item no width, so the surface is
  click-through and the pointer never reaches a card. A popup lives until its
  timer fires.
- **Right-click** on the card, or the expand button, toggles expansion.
- **Middle-click** dismisses the whole group.
- **Drag** sideways past **70 px** dismisses; the two cards nearest the
  dragged one follow it at 0.3 and 0.1 of the distance, and releasing short of
  the threshold springs everything back with `expressiveFastSpatial`. The
  dismissal itself slides the card out by its width plus 20 px over
  `elementMove` (500 ms) before discarding.

### 7.6 The friendly time

```
< 1 minute        "Now"
same day          "<n>h" when at least an hour old, else "<n>m"
yesterday         "Yesterday"
older             "MMMM dd"
```

---

## 8. Status (proscenio)

`src/services/notifications.rs` owns `org.freedesktop.Notifications` on the session bus
and exports the standard interface at `/org/freedesktop/Notifications`:
`Notify`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`, and
the `NotificationClosed` and `ActionInvoked` signals.

It stores the list in `~/.cache/proscenio/notifications.json` in the §3
shape, continues the id counter above the largest stored id, honors
`replaces_id`, applies the `popupInhibited` rule to the unread count, runs the
expiry timer with the `notifications.timeout` fallback, discards transient
notifications on expiry, and resets the unread count when the sidebar opens.
The silent flag is `notifications.silent` in
`~/.config/proscenio/config.toml`.

The name is requested without the replace flag: while another daemon, such as
the QML shell, owns it, proscenio waits in the queue and takes it when that
daemon exits.

Grouping by app is in `Notifications::groups`: a group's time is the latest
of its members, and the groups are ordered by that time, newest first.

### The popups

`src/panels/notifications/popup.rs` opens one layer surface per monitor on the
overlay layer, 410 px wide, anchored top and right, shown only on the popup
monitor: the forced monitor when configured, otherwise Hyprland's focused
one, re-checked on `focusedmon`. A forced monitor that is not connected falls
back to the focused one. qs does the same: its `screens.find` returns nothing,
and the `null` screen lands on the focused output.

The list follows the QML geometry: `410 − elevationMargin · 2` = 390 px wide,
right-aligned, 4 px from the top and the right. Popup cards take
`colBackgroundSurfaceContainer` and the `StyledRectangularShadow`, rendered as
`box-shadow: 0 1px 9px 1px colShadow`. The window is only as tall as its
cards; the list keeps `elevationMargin` below them for the shadow.

Popup and sidebar cards match qs to the pixel except glyph advances, which
differ by up to a pixel along a line.

`src/panels/notifications/card.rs` draws the group card: the 17 px radius, the
80 px collapsed cap, the app-name-or-summary top line, the friendly time with
its four cases, the count-and-chevron expand button, the two-item collapsed
view with the second at half opacity, and the expanded view with per-item
backgrounds, wrapped bodies and the close / actions / copy row with its
1500 ms `inventory` swap. Right-click and the button expand; middle-click
dismisses the group.

`src/panels/notifications/icon.rs` draws the 38 px icon: a circle, or for a
critical group `VerySunny` or `SoftBurst` (`src/ui/shapes.rs`) picked at
random, in `colSecondaryContainer` or `colPrimaryContainer`. Inside it, in
order:

- a group of one notification with an image: the image, filling the circle
  and clipped round, with the app icon at 0.49 of the size at the bottom
  right;
- an app icon given as an image file path (starting with `/`): drawn like an
  image, clipped to the circle at full size; the QML draws it at 0.8;
- an app icon given as a themed icon name: drawn at 0.8 of the size;
- no app icon: the Material symbol from the §7.4 keyword table at 0.57, in
  `colOnSecondaryContainer` or `colOnPrimaryContainer`.

The microphone mute and unmute notifications from `src/services/audio.rs` are
sent with `notify-send` as app `Microphone`, with
`-n $XDG_RUNTIME_DIR/proscenio/micgate.png` as the app icon and the
`transient` hint, so their icon is that picture at full size.

Divergences:

- The collapsed height cap is a `FixedHeight` (`src/ui/widgets/fixedheight.rs`)
  pinned to `min(80, natural)`: it lays the row out at its natural height and
  cuts off the rest, so the card has that height, as the QML `implicitHeight`
  does, and the layer surface grows with it. It does not take the wheel; the
  wheel goes to the list. It animates over 200 ms in both directions; the QML
  animates only on collapse.
- The body is rendered as Pango markup with everything but `b`, `i`, `u` and
  `s` stripped, so inline images and links in a body become their text. The
  QML renders full rich text with images.
- The window is sized to its content, not masked to the cards. The cards take
  input, and so do the margins around them: 16 px on the left, 4 px above and
  to the right, and 10 px below.
- Entering a card holds the expiry timers of its group; leaving restarts each
  with the interval its notification arrived with. The QML leave handler
  times the group out at once (§7.5).

Popups, like sidebar cards, dismiss on a sideways drag and on a middle click.
A drag starts only once the pointer has moved past GTK's drag threshold, so a
press on the expand button or an action button stays a click, as in the QML,
where the buttons sit above the `DragManager`.

The slide wrapper around each card reports its child's request mode
(height-for-width). In GTK's default constant-size mode a wrapping body is
measured at its unwrapped width and the expanded card cuts off its button row.

The hint handling follows Quickshell. An empty `app_icon` falls back to the
icon of the `desktop-entry` hint's desktop file. Raw pixels in `image-data`,
`image_data` or `icon_data` are saved as
`~/.cache/proscenio/notifications/<id>.png`, which survives a restart
(Quickshell keeps the image in memory); the file is deleted when the
notification is discarded. Otherwise `image-path` or `image_path` is used: a
`file:` URI becomes a path, and anything else not starting with `/` is an icon
name.

Not implemented: inline reply.
