---
title: Media controls
sidebar_label: Media controls
description: The panel of player cards for every MPRIS media player, colored from the album art, with an audio wave from cava, seeking, and the shortcuts and IPC that drive it.
---

# Media controls

A panel with one card per media player: the album art, the title and artist, the time, transport
buttons and a seek bar. Each card takes its colors from its album art, and while music plays an
audio wave moves behind it. Players are found over MPRIS on the session bus.

The panel is `src/panels/mediacontrols.rs`; the player service is `src/services/mpris.rs`.

## Opening and closing

- A left click on the media module of the [bar](bar.md) toggles the panel.
- The global shortcuts `mediaControlsToggle`, `mediaControlsOpen` and `mediaControlsClose` (see
  [Shortcuts](shortcuts.md)) act on the focused monitor.
- Over [IPC](ipc.md), the `mediaControls` target has `toggle`, `open` and `close`. Opening it this
  way also times out the notification popups on screen.
- A click anywhere outside the panel closes it.

With a horizontal bar the panel opens just past the bar, on the bar's side of the screen, and ends
just left of the screen's center, beside the spot where the [on-screen display](osd.md) appears.
With a vertical bar it opens beside the bar, a little above the middle of the screen. Cards stack
downward, overlapping by their shadow margin.

## Which players show

Every bus name under `org.mpris.MediaPlayer2.` is a player. Two players count as one when their
titles contain one another and their positions and lengths are both within 2 seconds of each
other; the card goes to the one with album art, or else the first.

With no player left, the panel shows *No active player*.

## A card

| Part | What it shows |
|---|---|
| Art | The track's album art, faded in when it arrives. |
| Title | The track title, with a leading bracketed tag such as `(Official Video)`, `[MV]` or `【…】` removed. *Untitled* when there is none. |
| Artist | The first artist. |
| Time | `position / length`, or the position alone when the track has no length. Hours appear only past one hour. |
| Play button | Play or pause. Rounded square while playing, circle while paused. |
| Previous, next | Previous or next track, when the player allows it. |
| Seek bar | A wavy slider when the player can seek and the track has a length; otherwise a progress bar that waves while playing. |

The buttons act when pressed, not when released. Dragging the slider seeks on release: with
`SetPosition` and the track id when the player gives a valid one, with a relative `Seek` otherwise.

**Live streams.** A track with no `mpris:length`, or with a length of `i64::MAX` (the value an
endless stream reports) or one too large to fit in an `i64`, has no length. Its card shows the
position alone and a progress bar instead of the slider. When a player stops reporting the length
of a track it reported before, the card keeps using the last length it saw for that track.

**Players without metadata.** When a player gives neither a title nor an artist, the card shows the
player's name (`Identity`) as the title, its app icon in place of the art, and no time or seek row;
the play button moves to the bottom.

## Colors from the art

The art is downloaded with `curl` into `~/.cache/proscenio/coverart/`, named by the MD5 of its URL,
and reused from there. It is decoded once, off the main thread, at the smallest size that still
fills the card, and from the same decode the shell takes the image's average color.

That color, mixed with a fifth of the primary container color, seeds the card's own palette: the
shell's background, text and accent colors are each pulled toward it. So every card has its own
colors, matching its art. A card without art is seeded from the secondary container color.

Behind the content, the card shows its art again, cropped to fill, blurred, with saturation raised
by a fifth and the card's background color over it at 70% opacity. This backdrop is rendered once
per piece of art and card size, not on every frame.

## The audio wave

While the panel is open, the shell runs `cava` with a configuration it writes to
`$XDG_RUNTIME_DIR/proscenio/cava.conf`: 50 bars at 60 frames a second, mono, as plain numbers on
standard output. Every card that is playing draws those numbers as a smoothed, blurred wave along
its bottom, in a faint version of its accent color. Closing the panel stops `cava`. Without `cava`
installed the cards work, with no wave.

## Following the players

The service follows `PropertiesChanged` from the players and `NameOwnerChanged` for players coming
and going. The position is re-read on the shell's polling interval, `resources.updateInterval`
(default 3000 ms, **System › Services › Resources**; see [System settings](settings-system.md)),
and only while some player is playing.

## The active player

The `mpris` IPC target acts on one player, the active one: the player whose playback state changed
last. A player that appears while playing becomes the active one too. Without either, it is the
first player.

| Function | Action |
|---|---|
| `playPause` | Play or pause the active player. |
| `previous` | Previous track. |
| `next` | Next track; for a player that cannot skip but can seek, jump to the end of the track. |
| `pauseAll` | Pause every player that can pause. |

```bash
proscenio ipc call mpris playPause
```
