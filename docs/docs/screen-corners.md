---
title: Screen corners
sidebar_label: Screen corners
description: The black rounded corners drawn over each monitor's corners, and the corner regions that open the sidebar and scroll brightness and volume.
---

# Screen corners

proscenio draws a small black quarter-circle in each corner of every monitor, so the screen looks
rounded like the windows on it. Two of those corners can also be hot: hovering or clicking them
opens the [sidebar](sidebar.md), and scrolling over them changes brightness or volume.

The source is `src/panels/corners.rs`.

## The rounded corners

Each corner is its own layer surface on the overlay layer, anchored to that corner, reserving no
space. It is painted black, not in a theme color: it masks the screen's square corner, the way a
rounded display bezel would. The painted part takes no input, so clicks go through it.

The radius is Hyprland's `decoration:rounding`, the same rounding the windows get. The shell reads
it again whenever Hyprland reloads its config and redraws the corners if it changed.

| `appearance.fakeScreenRounding` | Corners |
|---|---|
| `0` | None. The corner regions below are gone too. |
| `1` | Always, also over fullscreen windows. |
| `2` (default) | Hidden while a fullscreen window is on the workspace the monitor shows. |

The setting is *Screen round corner* on the **Quick** page of the settings app, under *Bar &
screen*, and on **Appearance › Bar**.

## Corner regions

A corner region is a thin strip along the screen edge at a corner: 250 by 5 pixels by default,
running from the corner along the top or bottom edge. Regions sit on the two top corners, or the
two bottom ones with *Place at bottom*. They work wherever the bar is.

- **Right corners** open the sidebar. A click toggles it. With *Hover to trigger* on, entering the
  region opens it. With that off and *Force hover open at absolute corner* on, the sidebar opens
  when the pointer reaches the corner's very end: within 2 pixels of the screen's side edge, and
  more than *vertical offset* pixels from the top (or bottom) edge. It opens once on arriving there,
  not again while the pointer stays.
- **Left corners** do not open anything; they exist for scrolling.
- **Scrolling** over a region, with *Value scroll* on, changes brightness on the left and volume on
  the right. Scrolling up raises the value. Brightness moves in steps of 5%; volume moves in steps
  of 1% below 10% and 2% above. The matching [on-screen display](osd.md) closes once the pointer
  leaves the region or moves more than 20 pixels away from where the scroll happened.

The vertical offset decides which way into the corner counts: with a non-zero offset, sliding along
the top edge into the corner does not open the sidebar, while coming up along the side edge does.
The rest of the strip then stays free for scrolling.

These settings are on **Appearance › Panels › Sidebars**, under *Corner open*. Their keys are under
`sidebar.cornerOpen`:

| Setting | Key | Default |
|---|---|---|
| Enable | `enable` | on |
| Hover to trigger | `clickless` | off |
| Force hover open at absolute corner | `clicklessCornerEnd` | on |
| with vertical offset | `clicklessCornerVerticalOffset` | 1 |
| Place at bottom | `bottom` | off |
| Value scroll | `valueScroll` | on |
| Visualize region | `visualize` | off |
| Region width | `cornerRegionWidth` | 250 |
| Region height | `cornerRegionHeight` | 5 |

*Visualize region* paints the strips in the primary color, to see where they are.

A corner that carries a region grows to the region's size, keeping the rounded corner in its own
corner. Its input region is the strip alone, so the rest of the surface stays click-through.

## Dead pixel workaround

Hyprland can leave the last pixel column on the right and the last row at the bottom out of pointer
input, which makes the right-hand and bottom regions hard to reach. With
`interactions.deadPixelWorkaround.enable` on (**System › Advanced › Workarounds**, *Dead pixel
workaround*; see [System settings](settings-system.md)), the right and bottom corners extend one pixel past the screen edge and draw one pixel
in from it, so the edge pixel takes input.

Every setting on this page takes effect at once: a change rebuilds the corners.
