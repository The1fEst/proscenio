# The calendar panel

Source: `modules/ii/calendarPanel/`, twelve files, plus
`services/TimerService.qml`. Read [foundations.md](foundations.md) first.

The popup that opens from the bar's clock: a navigation rail on the left and
one of three pages — a month calendar, a to-do list and a timer.

## 1. The window — `CalendarPopup.qml`

A layer surface on the Overlay layer, anchored top and left, with
`margins.top = barHeight` and `ExclusionMode.Ignore`. It is centered on the
clock's layout cell: `margins.left = anchor.x + (anchor.width − 420) / 2`.
The background is inset by the 10 px elevation margin on every side, so the
visible panel sits 10 px right of true center.

The panel is `colLayer0` with a 1 px `colLayer0Border`, radius 17 and the
elevation shadow. Inside it, a `colLayer1` rectangle of radius 17 holds the
rail and the page. Escape closes it; Ctrl+PgUp and Ctrl+PgDn step the tab.

## 2. `CalendarPanelContent.qml`

Height `max(350, page.implicitHeight)`. The rail sits in an item with left and
top margins of 10, inset another 5 and centered with the anchors `half()` rule;
the page starts 20 px after it. The current tab is kept in
`Persistent.states.calendarPanel.tab`.

A tab switch fades the page out while shifting it 10 px (200 ms,
`expressiveEffects`), swaps it through a `Loader`, then slides and fades the
new page in from the other side (200 ms, `emphasizedDecel`). The `Loader`
recreates the page, so every tab opens on its first sub-tab.

**The rail** is a collapsed `NavigationRailTabArray`: 56 px slots, a 56×32
highlight pill in `m3secondaryContainer` that moves in 350 ms
(`expressiveFastSpatial`), 24 px icons whose fill eases with the selection,
and a 14 px label under each.

## 3. The pages

**Calendar.** A header button with the month title at 19 px and two 30 px
chevrons, then a 7-column grid of 38 px cells with 5 px gaps. The weekday row
is at 0.4 opacity; today is a filled `colPrimary` cell. The wheel and PgUp /
PgDn change the month. When the view is off the current month, the title gets
a "•" and a tooltip, and clicking it jumps back.

**To Do.** A `SecondaryTabBar` (Unfinished / Done) over a `SwipeView` with
10 px gaps. Each list is cards of wrapped text over a check and a delete
button, with a placeholder symbol and line when empty. A 56 px FAB opens an
"Add task" dialog over a scrim; N opens it from the keyboard.

**Timer.** Another `SecondaryTabBar` (Pomodoro / Stopwatch) over a
`SwipeView`:

- *Pomodoro* — a 200 px `CircularProgress` with an 8 px line, 20° gaps and
  an 800 ms `OutCubic` ease. It shows `MM:SS` at 40 px and the phase, and a
  36 px badge in the corner shows the cycle. Under it are Start / Pause /
  Resume and Reset buttons, 90×35. The `ColumnLayout` gives spare height
  to its two rows in proportion to their size.
- *Stopwatch* — `MM:SS` at 40 px followed by `:cs` in `colSubtext`. qs
  renders the `<sub>` markup at full size; proscenio uses plain text. The
  line centers in the page until the first lap, then moves to the top (200 ms,
  `expressiveEffects`). The laps list below pops new laps in and slides the
  others down (500 ms, `expressiveDefault`); Reset slides them out to the
  right.

Space or S toggles the timer on the current sub-tab, R resets it, L records
a lap and PgUp / PgDn switch sub-tabs.

## 4. `TimerService.qml`

Settings come from `time.pomodoro` (`focus` 1500, `breakTime` 300,
`longBreak` 900, `cyclesBeforeLongBreak` 4). State lives in
`Persistent.states.timer`:

- `pomodoro { running, start, isBreak, cycle }`, where `start` is in seconds;
- `stopwatch { running, start, laps }`, where `start` is in centiseconds.

The pomodoro refreshes every 200 ms. When a lap runs out it flips to
the next phase and sends a `notify-send Pomodoro` message. With
`sounds.pomodoro` set, it also plays `alarm-clock-elapsed` from the
`sounds.theme` theme through `paplay`. The cycle advances on each return to
focus. The stopwatch refreshes every 10 ms and is reset at start-up unless it
was running.

---

**Status (proscenio).** Everything above is in `src/panels/calendar/mod.rs`, the widgets
`navrail`, `month`, `secondarytabs`, `swipe`, `todo`, `pomodoro` and `laps`,
and the services `src/services/timer.rs` and `src/core/persistent.rs`. It shares
`states.json` with qs, key for key.

As in qs:

- the tab slide direction follows the tab index;
- the page-level keys (the month keys, N, the timer keys and the sub-tab
  PgUp / PgDn) are handled on the panel in the capture phase;
- the pomodoro's track arc is not drawn when it would be empty;
- L records a lap only while the stopwatch runs.

The pages follow the pointer as a `SwipeView` does
(`src/ui/widgets/swipe.rs`): a drag takes over once it has gone 10 px
sideways, and a mostly vertical one is left to the list underneath. The pages
stop at the first and the last. On release, a drag still moving faster than
30 px/s goes one page on in its direction, and a slower one settles on the
nearest page; either way it glides there over the same 250 ms as a tab
switch, and the tab bar follows.

The laps scroll as a `StyledListView` does (`src/ui/widgets/flickable.rs`, a
port of Qt 6.11's `Flickable` for one vertical list). A wheel notch flicks
72 px at a 15000 px/s² deceleration, faster notches stack through a
three-sample velocity average, and a touchpad moves the list one to one and
settles back at the ends. A pointer drag takes over after 10 px, goes half
as far past an end, and on release flicks on at 1500 px/s² (at most
3500 px/s) or springs back to the end in 400 ms. With
`fasterTouchpadScroll` on, every wheel event moves the list by
`angleDelta / mouseScrollDeltaThreshold` times the mouse or touchpad
factor, eased over 200 ms (`standardDecel`); every other vertical scroll view
(settings, to-do, dialogs, lists, the launcher, the wallpaper grid) gets the
same eased step from `follow_scroll_settings`, and keeps GTK's own scrolling
while the switch is off. The scroll bar is 4 px wide in
`colOnSurfaceVariant`, inset 17 px at both ends; it fades to 0.5 opacity in
350 ms while the pointer is over its 8 px strip or holds it, and dragging it
scrolls the list.
