---
title: Calendar
sidebar_label: Calendar
description: The panel the bar's clock opens, with a month view, a to-do list, and a pomodoro timer and stopwatch.
---

# Calendar

Clicking the clock on the [bar](bar.md) opens a panel with a navigation rail on the left and three
pages: **Calendar**, **To Do** and **Timer**. The source is in `src/panels/calendar/`, with the
timer in `src/services/timer.rs` and the list in `src/services/todo.rs`.

## Opening and closing

- A left click on the bar's clock toggles the panel. With a horizontal bar it hangs from the top
  edge, one bar height down, centered on the clock; with a vertical bar it opens beside the bar,
  centered on the clock vertically.
- The global shortcut `calendarToggle` (see [Shortcuts](shortcuts.md)) toggles it on the focused
  monitor, and so do the `calendar` functions `toggle`, `open` and `close` over
  [IPC](ipc.md), for example `proscenio ipc call calendar toggle`.
- Escape closes it, and so does a click anywhere outside it.

The page that was open last is remembered in `~/.local/state/proscenio/states.json` (under
`$XDG_STATE_HOME` when that is set), so the panel opens on it again, also after a restart. Switching
pages slides the old one out and the new one in, and resets the page switched to: the calendar
returns to the current month, the to-do list and the timer to their first tab.

## Keys

| Key | Page | Action |
|---|---|---|
| Ctrl+Page Up, Ctrl+Page Down | any | Previous or next page |
| Escape | any | Close the panel, or the *Add task* dialog when it is open |
| Page Up, Page Down | Calendar | Previous or next month |
| Page Up, Page Down | To Do | *Unfinished* or *Done* |
| N | To Do | Add a task |
| Page Up, Page Down | Timer | *Pomodoro* or *Stopwatch* |
| Space or S | Timer | Start, pause or resume the timer on screen |
| R | Timer | Reset the timer on screen |
| L | Timer | Record a lap, while the stopwatch runs |

## Calendar

A month grid of six weeks, starting on Monday, with today filled in the primary color and the days
of the neighboring months dimmed. The header shows the month and year; the two chevrons, the mouse
wheel and Page Up / Page Down move a month at a time. When the grid is away from the current month,
the title starts with `•`, and clicking it jumps back to today's month.

## To Do

Two lists, **Unfinished** and **Done**, under a tab bar; drag sideways to move between them. Each
task is a card with its text, a button that marks it done (or not done again) and a button that
deletes it. An empty list shows a placeholder.

The round **+** button in the corner, or N, opens *Add task*: type the task and press Enter or
**Add**. A new task goes to *Unfinished*, and the list switches there.

The tasks are kept in `~/.local/state/proscenio/todo.json`, an array of
`{"content": "…", "done": false}` objects.

## Timer

Two timers under a tab bar, **Pomodoro** and **Stopwatch**; drag sideways to move between them.
Both keep running with the panel closed.

### Pomodoro

A ring shows the time left in the current phase, with the time as `MM:SS`, the phase (*Focus*,
*Break* or *Long break*) and, in a badge, the number of the cycle. Under it are **Start** (which
reads **Pause** while running and **Resume** after a pause) and **Reset**, which goes back to the
first focus phase.

Focus and break alternate, and each return to focus starts the next cycle. The break of the last
cycle, the fourth with the defaults, is a long break, and the count then starts over at 1.

When a phase ends, a *Pomodoro* notification names the next phase and its length. With the
pomodoro sound on, the shell also plays `alarm-clock-elapsed` from the sound theme `sounds.theme`,
found under `/usr/share/sounds/<theme>/stereo/` and played with `paplay`.

| Setting | Key | Default |
|---|---|---|
| Focus | `time.pomodoro.focus` | 1500 seconds |
| Break | `time.pomodoro.breakTime` | 300 seconds |
| Long break | `time.pomodoro.longBreak` | 900 seconds |
| Cycles before long break | `time.pomodoro.cyclesBeforeLongBreak` | 4 |
| Pomodoro sound | `sounds.pomodoro` | off |

The durations are on **System › Date & Time**, under *Pomodoro*, where they are entered in minutes
(see [System settings](settings-system.md)); the sound switch is on the **Sound** page (see
[Sound settings](settings-sound.md)). The config file holds seconds.

### Stopwatch

The elapsed time as `MM:SS`, followed by hundredths in a lighter color. The buttons are **Start**
(**Pause**, **Resume**) and a second one that records a **Lap** while running and reads **Reset**
when stopped. Once there is a lap, the time moves to the top and the laps list fills the space
under it, newest first. Each lap shows its number, the total time and, in the primary color, the
time since the previous lap.

### What survives a restart

The timer state is kept in `states.json` under `timer.pomodoro` and `timer.stopwatch`, with start
times taken from the wall clock. A running pomodoro or stopwatch carries on across a restart of the
shell and counts the time in between. A stopwatch that was not running starts again from zero, and
its laps are cleared.
