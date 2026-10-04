---
title: Design system
sidebar_label: Design system
description: The tokens every surface draws with (the wallpaper palette, transparency, the derived layer colors, rounding, fonts, sizes and motion) and the reusable widgets built on them.
---

# Design system

Every surface draws from one theme, built in `src/ui/theme.rs`: a Material 3 palette taken from the
wallpaper, a set of colors derived from it for stacked layers and their states, and fixed tokens for
rounding, type and size. The motion curves live in `src/ui/anim.rs`, and the widgets that put all of
it together in `src/ui/widgets/`.

Config keys on this page are paths into `~/.config/proscenio/config.toml`:
`appearance.transparency.enable` is the `enable` key of the `[appearance.transparency]` table.

## The palette

The palette is a Material 3 scheme read from `~/.local/state/proscenio/generated/colors.json`, a JSON
object that maps role names to colors: `primary`, `on_surface_variant`, `surface_container_low` and
so on. [Colors](colors.md) covers how it is generated from the wallpaper. Next to the Material 3 roles
the theme reads four success roles: `success`, `on_success`, `success_container` and
`on_success_container`. A role missing from the file, or a missing file, falls back to a built-in dark
gray scheme.

Dark mode follows the palette itself: the theme is dark when `background` has an HSL lightness below
0.5.

The shell watches the file and rebuilds the theme when it changes, and also when
`appearance.transparency`, `appearance.extraBackgroundTint` or `appearance.fonts` change. A rebuild
reloads the stylesheet and redraws every window. Widgets that paint themselves read the theme when
they draw, so a palette change needs no other work.

### In the stylesheet

`src/ui/style.css` is the shell's stylesheet. The theme puts every color in front of it as a CSS
custom property on `window`, `tooltip` and `popover`:

- palette roles as `--m3` plus the role in camel case: `--m3surfaceContainerLow`, `--m3onBackground`;
- derived colors under their own names: `--colLayer1`, `--colSubtext`;
- the font families as `--fontMain`, `--fontTitle`, `--fontMonospace`, `--fontReading`,
  `--fontExpressive` and `--fontNerd`.

Each color also gets a class `fg-<name>` that sets the text color, which is how labels are colored:
`fg-colSubtext`, `fg-m3onBackground`. The stylesheet is installed one step above GTK's user priority,
so its rules win over those of a `gtk.css`.

## Transparency

Two factors make the layers translucent.

| Key | Default | Meaning |
|---|---|---|
| `appearance.transparency.enable` | `false` | With this off the bottom layer is opaque |
| `appearance.transparency.automatic` | `true` | Take the background transparency from the wallpaper and use 0.9 for content |
| `appearance.transparency.backgroundTransparency` | `0.11` | The bottom layer's transparency when not automatic |
| `appearance.transparency.contentTransparency` | `0.57` | The content transparency when not automatic |

```
background = enable ? (automatic ? from the wallpaper : backgroundTransparency) : 0
content    = automatic ? 0.9 : contentTransparency
```

The content factor alone does not let the wallpaper through. Each layer above the bottom one is drawn
at `1 − content` opacity, in a color solved so that over the layer beneath it shows exactly its
palette color (`solve_overlay_color`, below). Over an opaque bottom layer the result is the plain
palette; once the bottom layer is translucent, the wallpaper shows through every layer above it too.

The automatic background value comes from the wallpaper, or from its thumbnail when the wallpaper is
a video. The image is scaled to 10×10 pixels and averaged into one color:

```
vibrancy = (HSL saturation + HSL lightness) / 2
y        = 0.5768·vibrancy² − 0.759·vibrancy + 0.2896
result   = clamp(y, 0, 0.22) − (dark mode ? 0 : 0.12)
```

## Color math

Three operations in `src/ui/theme.rs` produce every derived color.

| Function | Result |
|---|---|
| `mix(a, b, part)` | `part·a + (1 − part)·b` per channel, alpha included |
| `transparentize(color, amount)` | `color` with its alpha multiplied by `1 − amount` |
| `solve_overlay_color(base, target, opacity)` | The color that, drawn at `opacity` over `base`, looks like `target`: `clamp((target − base·(1 − opacity)) / opacity, 0, 1)` per channel, with alpha `opacity` |

`mix` weights its first argument: `mix(colLayer1, colOnLayer1, 0.92)` is 92 % layer and 8 % ink,
a hover tint.

## Layer colors

Surfaces stack in layers. Layer 0 is the background of a bar or panel, layer 1 the groups on it, and
each further layer sits on the one before. Every layer has a background (`colLayerN`), the ink drawn
on it (`colOnLayerN`), and hover and pressed variants (`colLayerNHover`, `colLayerNActive`).

In the table, "solved over X" means `solve_overlay_color(X, color, 1 − content)`.

| Color | Value |
|---|---|
| `colLayer0Base` | `mix(background, primary, 0.99)`, or plain `background` when `appearance.extraBackgroundTint` is `false` (it is `true` by default) |
| `colLayer0` | `transparentize(colLayer0Base, background transparency)` |
| `colOnLayer0` | `on_background` |
| `colLayer0Border` | `mix(outline_variant, colLayer0, 0.4)` |
| `colLayer1` | `surface_container_low`, solved over `colLayer0Base` |
| `colOnLayer1` | `on_surface_variant` |
| `colOnLayer1Inactive` | `mix(colOnLayer1, colLayer1, 0.45)` |
| `colLayer1Hover` | `transparentize(mix(colLayer1, colOnLayer1, 0.92), content)` |
| `colLayer1Active` | `transparentize(mix(colLayer1, colOnLayer1, 0.85), content)` |
| `colLayer2` | `surface_container`, solved over `surface_container_low` |
| `colLayer3` | `surface_container_high`, solved over `surface_container` |
| `colLayer4` | `surface_container_highest`, solved over `surface_container_high` |
| `colOnLayer2` to `colOnLayer4` | `on_surface` |
| `colLayer2Hover` to `colLayer4Hover` | The layer's palette color mixed 0.90 with its ink, solved over the layer below |
| `colLayer2Active` to `colLayer4Active` | The same at 0.80 |
| `colSubtext` | `outline` |

Layer 1 tints less on hover and press than the layers above it: 0.92 and 0.85 against 0.90 and 0.80.

The accent colors carry the same pair of variants:

| Color | Hover | Pressed (`…Active`) |
|---|---|---|
| `colPrimary` | `mix(primary, colLayer1Hover, 0.87)` | `mix(primary, colLayer1Active, 0.7)` |
| `colPrimaryContainer` | `mix(primary_container, on_primary_container, 0.9)` | the same at 0.8 |
| `colSecondary` | `mix(secondary, colLayer1Hover, 0.85)` | `mix(secondary, colLayer1Active, 0.4)` |
| `colSecondaryContainer` | `mix(secondary_container, on_secondary_container, 0.9)` | the same at 0.54 |
| `colTertiary` | `mix(tertiary, colLayer1Hover, 0.85)` | `mix(tertiary, colLayer1Active, 0.4)` |
| `colTertiaryContainer` | `mix(tertiary_container, on_tertiary_container, 0.9)` | `mix(tertiary_container, colLayer1Active, 0.54)` |
| `colError` | `mix(error, colLayer1Hover, 0.85)` | `mix(error, colLayer1Active, 0.7)` |
| `colErrorContainer` | `mix(error_container, on_error_container, 0.9)` | the same at 0.7 |
| `colSurfaceContainerHighest` | `mix(surface_container_highest, on_surface, 0.95)` | the same at 0.85 |

A few more complete the set:

| Color | Value |
|---|---|
| `colSurfaceContainerLow` to `colSurfaceContainerHighest` | Each surface container solved over the one below it, starting from `background` |
| `colBackgroundSurfaceContainer` | `surface_container` with the background transparency |
| `colTooltip`, `colOnTooltip` | `inverse_surface`, `inverse_on_surface` |
| `colScrim` | `scrim` at 50 % alpha |
| `colShadow` | `shadow` at 30 % alpha |
| `colOutline`, `colOutlineVariant` | `outline`, `outline_variant` |

The media controls tint part of this set toward the album art: `Theme::adapted` gives the accent
colors the art color's hue and saturation at their own lightness, then mixes the layer, ink and
accent colors with the art color.

## Rounding

| Token (`rounding::`) | Radius |
|---|---|
| `UNSHARPEN` | 2 px |
| `UNSHARPENMORE` | 6 px |
| `VERYSMALL` | 8 px |
| `SMALL` | 12 px |
| `NORMAL` | 17 px |
| `LARGE` | 23 px |
| `VERYLARGE` | 30 px |
| `FULL` | 9999 px |
| `WINDOW_ROUNDING` | 18 px |
| `SCREEN_ROUNDING` | 23 px |

`SMALL` is the default radius of buttons and of the groups in the bar. Tooltips use `VERYSMALL`,
dialogs pad their content by `LARGE`, the session screen's buttons are `VERYLARGE`, and the floating
bar is `WINDOW_ROUNDING`. A radius never exceeds half the shorter side, so `FULL` makes pills and
circles.

The screen's own rounding is Hyprland's: where the bar hugs the screen edge, and in the screen
corners, the radius is Hyprland's `decoration:rounding`, read over IPC (23 px if it cannot be read)
and read again whenever Hyprland reloads its config.

## Fonts

| Role | Key | Default |
|---|---|---|
| Main | `appearance.fonts.main` | Google Sans |
| Titles | `appearance.fonts.title` | Google Sans |
| Monospace | `appearance.fonts.monospace` | JetBrains Mono NF |
| Nerd Font icons | `appearance.fonts.iconNerd` | JetBrains Mono NF |
| Reading | `appearance.fonts.reading` | Readex Pro |
| Expressive | `appearance.fonts.expressive` | Space Grotesk |
| Symbols | fixed | Material Symbols Rounded |

Changing a font rebuilds every per-monitor surface and refreshes the settings window if it is open.

Sizes are in pixels. Ordinary text is the main family at 15 px with weight 450; titles are the title
family at 22 px with weight 550. Text fonts get an optical size (`opsz`) of 18 unless the caller sets one.

| Size (`pixel_size::`) | Pixels |
|---|---|
| `SMALLEST` | 10 |
| `SMALLER` | 12 |
| `SMALLIE` | 13 |
| `SMALL` | 15 |
| `NORMAL` | 16 |
| `LARGE` | 17 |
| `LARGER` | 19 |
| `HUGE`, `TITLE` | 22 |
| `HUGEASS` | 23 |

Icons are glyphs of Material Symbols Rounded, written as their ligature names (`settings`, `wifi`)
and shaped through three variable axes: `opsz` follows the pixel size, `FILL` runs from 0 (outlined)
to 1 (filled), and the weight follows the fill as `400 + 200·FILL`. A change of fill animates over
200 ms, and the fill is rounded to a tenth on every frame, so an animation passes through at most
eleven steps.

Application icons come from the icon theme named by `Theme=` in the `[Icons]` group of
`~/.config/kdeglobals`, or `breeze` when it names none. The shell follows that file and rebuilds the
surfaces that show icons when the theme changes.

## Sizes

| Element | Size |
|---|---|
| Bar, horizontal | 40 px tall |
| Bar, vertical | 46 px wide |
| Floating bar (`bar.cornerStyle = 1`) | The same plus a 5 px gap on each side |
| Groups on either side of the bar's center | 360 px each with `bar.verbose` (the default), 140 px without; 280 px on a screen up to 1200 px wide, 190 px up to 1000 px |
| Gap between a panel and the bar or screen edge | 10 px |
| Sidebar | 460 px wide |
| Calendar | 420 px wide |
| Notification popups | 410 px wide |
| On-screen display | 180 px wide |
| Media controls | 440×160 px |
| Dialogs inside a window | 350 px wide |
| Settings window | 1100×750 px by default |

## Motion

Animations run on the frame clock of the widget they move, as tick callbacks that stop once the
value arrives. A changed target always starts from the value on screen: no animation finishes its
previous run first.

| Curve (`src/ui/anim.rs`) | Control points | Usual duration | Used for |
|---|---|---|---|
| `EXPRESSIVE_EFFECTS` | 0.34, 0.80, 0.34, 1.00 | 200 ms | Color and opacity fades, radius changes, icon fill, tooltips |
| `EXPRESSIVE_FAST` | 0.42, 1.67, 0.21, 0.90 | 350 ms | Small moves: switch knobs, highlights in navigation rails |
| `EXPRESSIVE_DEFAULT` | 0.38, 1.21, 0.22, 1.00 | 500 ms; 400 ms for a press | Moving and resizing whole elements, the bounce of a pressed group button |
| `EMPHASIZED_DECEL` | 0.05, 0.7, 0.1, 1 | 400 ms | Things arriving: revealed rows, incoming notifications, progress values |
| `EMPHASIZED` | two segments, below | 300 ms | Resizing |
| `STANDARD_DECEL` | 0, 0, 0, 1 | 200 ms | Wheel scrolling, and ripples over 1200 ms |
| Emphasized accelerate | 0.3, 0, 0.8, 0.15 | 200 ms | Dialogs leaving |
| `Ease::OutSine` | — | 100 ms and 300 ms | Two-speed indicators |

`EXPRESSIVE_FAST` and `EXPRESSIVE_DEFAULT` have a control point above 1, so they pass the target and
settle back.

`EMPHASIZED` is two cubic segments joined at (1/6, 0.4):

```
(0, 0)    → (1/6, 0.4)   control points (0.05, 0)   and (2/15, 0.06)
(1/6, 0.4) → (1, 1)      control points (5/24, 0.82) and (0.25, 1)
```

The two-speed indicators move their leading edge on the short duration and their trailing edge on
the long one, so the indicator stretches toward its target and then catches up. The workspace
indicator and `SecondaryTabs` use 100 and 300 ms, the toolbar's `TabBar` 50 and 200 ms.

A label that animates its text change (`animate_change`) slides 6 px up while fading out over 150 ms,
swaps the text, and comes back in from 6 px below over another 150 ms.

## Interaction states

Buttons pick their background from a `Look`, a set of color tokens with one entry per state, and fade
to the target color over 200 ms with `EXPRESSIVE_EFFECTS` whenever the state changes.

| State | `RippleButton` default | `GroupButton` default |
|---|---|---|
| At rest | `colLayer1Hover`, fully transparent | `colLayer1Hover`, fully transparent |
| Hovered | `colLayer1Hover` | `colLayer1Hover` |
| Pressed | A ripple of `colLayer1Active` | `colLayer1Active` |
| Toggled | `colPrimary` | `colPrimary` |
| Toggled and hovered | `colPrimaryHover` | `colPrimaryHover` |
| Toggled and pressed | A ripple of `colPrimaryActive` | `colPrimaryActive` |
| Disabled | Transparent background, whole button at 40 % opacity | The rest color; hover and press are ignored |

The ripple is a soft-edged disc: solid to 30 % of its radius and clear by 50 %. It starts at the
pointer and grows over 1200 ms (`STANDARD_DECEL`) until it covers the button, then fades out over
2400 ms after the release.

Both kinds of button show the pointer cursor and take separate actions for the right and the middle
button. Holding a `GroupButton` for 800 ms acts as a right click.

## Widgets

The widgets in `src/ui/widgets/` are what the panels are built from. Many are GTK widget subclasses;
the others are structs that own the widgets they draw with, such as a drawing area or a popover.

### Buttons and inputs

| Widget | File | What it is |
|---|---|---|
| `RippleButton` | `ripple.rs` | The standard button, a `gtk::Button` subclass: a rounded background from its `Look` (radius `SMALL`, at least 30 px tall), the ripple, and `connect_alt` and `connect_middle` for the other buttons. A press on a control nested inside it is left to that control. |
| `GroupButton`, `ButtonGroup` | `group.rs` | Connected buttons. A pressed button widens by 20 px, or 10 px at either end of the group, over 400 ms, and can take a different radius while held. The group keeps its length, so the neighbors give way. |
| `Selection` | `selection.rs` | A wrapping set of `GroupButton`s that picks one value, each with an icon and a label. |
| `Switch`, `ConfigSwitch` | `controls.rs` | A 39×24 px switch whose knob grows and slides over 350 ms; `ConfigSwitch` is a full-width row with an icon, a label and the switch. |
| `ComboBox` | `controls.rs` | A 40 px pill on `colSecondaryContainer` that opens a list of 40 px items. |
| `icon_button` | `controls.rs` | A 35 px button on `colLayer2` with a symbol and a label. |
| `SpinBox` | `spinbox.rs` | A number field between a minus and a plus button; holding a button repeats after 300 ms, every 100 ms. |
| `Slider` | `slider.rs` | A 30 px track with a 3 px handle that thins while pressed, an icon inside the track, optional stop markers, and a wavy variant; `colPrimary` on a `colSecondaryContainer` track by default. |
| `TextField` | `textfield.rs` | An outlined or filled text field whose label floats up while it has focus or text; a secret variant hides the text. |
| `SecondaryTabs` | `secondarytabs.rs` | Tabs with a two-speed underline. |
| `TabBar`, `paired_fab` | `toolbar.rs` | The floating toolbar: pill-shaped tabs over a two-speed highlight, and a 48 px action button on `colTertiaryContainer`. |

### Indicators

| Widget | File | What it is |
|---|---|---|
| `Ring` | `ring.rs` | A 20 px circular gauge: a disc of half-transparent ink with a wedge swept clockwise from the top by the value. The icon in the middle is cut out of the ring, not drawn on it. The ink turns `colError` when the caller flags a warning. |
| `ProgressBar` | `progress.rs` | A 4 px bar whose value eases over 400 ms; it can be wavy, with the wave moving. |
| `LoadingIndicator` | `loading.rs` | A `colPrimaryContainer` disc holding a `colOnPrimaryContainer` shape at 70 % of its size. The shape turns once every 12 s, and every 800 ms it morphs into the next of seven shapes (200 ms), turns another 90° (350 ms) and swells to 120 % and back (750 ms). |
| `MaterialShape` | `materialshape.rs` | One of the Material 3 shapes in `src/ui/shapes.rs` (circle, pill, cookies, clover, soft burst and more) filled with a color token. `src/ui/morph.rs` turns one shape into another by matching their corners. |

### Overlays

| Widget | File | What it is |
|---|---|---|
| `Tooltip` | `tooltip.rs` | A bubble in a popover that never takes input: `colTooltip`, radius 8 px, padding 10×5 px, 12 px text in `colOnTooltip`. It grows from its bottom center while fading in over 200 ms. `hover_delay` shows it after a delay. |
| `Popup` | `popup.rs` | The bar's hover popups: a `m3surfaceContainer` card with a 1 px `colLayer0Border` border, radius 12 px and a soft shadow, placed 10 px off the bar. |
| `WindowDialog` | `windowdialog.rs` | A modal dialog inside a window: a 350 px `m3surfaceContainerHigh` card over `colScrim`, padded by 23 px. It slides 60 px into place while growing to its height over 200 ms and leaves the same way with emphasized accelerate. Escape or a click outside the card dismisses it. |
| `Hint` | `scrollhint.rs` | Up and down arrows around an icon, revealed beside a bar area that responds to scrolling. |

### Motion containers

| Widget | File | What it is |
|---|---|---|
| `reveal::wrap`, `with_gap`, `vertical` | `reveal.rs` | Show or hide a child by animating its length over 400 ms (`EMPHASIZED_DECEL`), with a 15 px gap that opens or closes over 200 ms. The child is hidden once its length reaches zero. |
| `Slide`, `DragList` | `slide.rs` | Cards dragged sideways. While one card is dragged, its neighbors follow at 30 % and 10 % of the distance; released past 70 px, the card slides out and its height collapses. |
| `Swipe` | `swipe.rs` | Pages side by side, switched by dragging or from code, over 250 ms. |
| `Flickable` | `flickable.rs` | A kinetic scroller with flicks, overshoot at the ends and a 4 px scroll bar that fades out. `follow_scroll_settings` gives a plain scrolled window the same wheel behavior. |
| `Shift`, `animate_change` | `text.rs` | `Shift` offsets a child without changing its size; `animate_change` uses it for the text change animation. |

`src/ui/anim.rs` holds the pieces these are made of: `Tween` (a value between two points on a curve),
`Motion` (a tween that drives a widget's redraws), `Fader` (opacity, hiding the widget at zero) and
`Fade` (a color).

### Layout

| Widget | File | What it is |
|---|---|---|
| `Centred` | `centred.rs` | Centers one child, optionally rotated. A label is placed by its font's line metrics, so text sits at the same height whatever its glyphs; `integral` rounds to whole pixels and `optical` centers digits by their ink. |
| `Column`, `Row` | `column.rs`, `row.rs` | Vertical and horizontal boxes with fixed spacing that skip hidden children. |
| `Flow` | `flow.rs` | Wraps its children onto lines and reports where the lines break. |
| `FixedWidth`, `FixedHeight` | `fixedwidth.rs`, `fixedheight.rs` | Clip a child to a set width or height; `FixedHeight::follow` tracks the child's height up to a cap. |
| `Viewport` | `viewport.rs` | Holds several children and shows one of them at a fixed size. |
| `TrimmedBin` | `trimmedbin.rs` | A layout manager that reports a natural width a few pixels under the child's. |
| `Paint` | `paint.rs` | A widget drawn by a closure. |
| `customicon::build` | `customicon.rs` | Draws one of the SVG icons built into the binary from `assets/icons/`, cached per size. |
