# S7: visual LED editor

Spike `spikes/s7-led-editor` on the `spikes` branch: an `adw::Application` with one page per side of
the device (stick, stick base, throttle) in an `adw::ViewStack`. Each page draws placeholder SVG
artwork and places one round button per LED on it; the button opens a popover with an
`adw::ToggleGroup` of the LED's states. Built against the required versions (`gtk4` 0.11.5
`gnome_50`, `libadwaita` 0.9.2 `v1_9` + `gtk_v4_22`) and run on the Ubuntu 26.04 host (GTK 4.22.4)
on 2026-10-28. The artwork is placeholder outlines; real drawings come later (see "Artwork").

Run it with `cargo run`; set `GX52_SPIKE_SCREENSHOT_DIR=<dir>` to render every page to
`<dir>/<page>.png` and quit (used to check the result without a person at the screen).

## Technique

`HotspotView` (`src/hotspot_view.rs`) is a small `gtk::Widget` subclass:

- Holds a `gdk::Paintable` (the artwork) and draws it scaled to fit with its aspect ratio kept
  ("contain"), centred in the allocation. It redraws when the paintable invalidates its contents and
  disconnects that handler when the paintable changes.
- Children are added with a position in artwork coordinates (`0.0..=1.0` of width and height). In
  `size_allocate` each child gets its natural size, centred on its point inside the artwork rectangle,
  so the LEDs stay on the drawing at any window size while the buttons keep their size.
- Height-for-width, natural size from the paintable's intrinsic size, small minimum, so it works at
  the 360×294 minimum window size.
- Accessible role `Group` with the page title as label; the children are normal focusable widgets, so
  keyboard navigation and screen readers work without extra code.

The artwork is loaded with `gtk::Svg::from_bytes` (GTK 4.22, part of `gnome_50`): GTK's own SVG
paintable, no `librsvg` or `resvg` dependency, and it scales without blurring. In the app the SVG
files go into the GResource.

Each LED is a `gtk::MenuButton` with an empty child. Its colour comes from CSS classes on the
`menubutton` node (`led-off`, `led-on`, `led-green`, `led-red`, `led-amber`) styled with libadwaita's
palette variables (`--green-3`, `--red-3`, `--yellow-3`); the tooltip and the accessible label say
"<LED> LED: <state>".

## Results

| Check | Result |
|---|---|
| Builds with `clippy -D warnings` (pedantic) against GNOME 50 features | OK |
| Runs on the host, all three pages render, no GTK criticals or warnings | OK |
| LED buttons placed on the artwork at the given points, coloured by state | OK (after the CSS fix below) |
| Popover state selection, keyboard navigation, live resize | to check by hand on the hands-on day |

Findings:

- **A `MenuButton` is two CSS nodes**: `menubutton` (gets the widget's classes) wrapping `button`
  (gets the look). The first version styled `button.led` and showed default buttons; the selector has
  to be `menubutton.led > button`.
- **A `MenuButton` without label, icon or child draws a drop-down arrow** even with
  `always-show-arrow` false. Setting an empty `gtk::Box` as child gives a plain round button.
- Placing hotspots by fractional coordinates keeps the artwork data-driven: a view is an SVG plus a
  table of `(LED, x, y)`. The same `HotspotView` can host the input tester's indicators (below).

## Design for the app

- **One view per side**, because no single photo shows every LED (and every button, for the tester):
  stick (front/top: Fire, A, B, POV 2), stick base (T1/T2, T3/T4, T5/T6), throttle (D, E, i,
  throttle-axis LED). Switched with a view switcher, or shown side by side when the window is wide
  (breakpoint). Exact split decided when the photos exist.
- The editor edits the profile in memory; changes go to the device immediately (live preview) and are
  saved with the profile.
- Brightness and the blink toggle stay as regular rows next to the picture; the picture is only for
  per-LED state.
- Placeholder outlines are fine for development; the views and coordinates are data, so replacing
  the artwork does not touch code.

## Artwork

Source: line drawings traced (Inkscape) from the maintainer's own photos. No Logitech or Saitek
product images or manual scans (copyright). Released with the app under GPL-3.0-or-later (or
CC-BY-SA-4.0 for the SVGs, decide when adding them). Style: monochrome outline drawn with
`currentColor`-like neutral grey so it works in light and dark mode, no shading.

Photo guide (one session, about 6 to 8 photos):

- **Views**: stick from the front-top (trigger side away, buttons A, B, Fire cover, POV 2 visible);
  stick from the left side (trigger, pinkie, side buttons); stick base from the front (the three
  toggle pairs and their LEDs); throttle from the right/top (D, E, i, mouse stick, scroll wheel,
  MFD); throttle from the left (thumb-side buttons, throttle axis LED); one wide shot of the whole
  device for proportions.
- **Camera**: from a distance with optical zoom (2× to 5× on a phone), lens square to the main
  face of the part. This reduces perspective so traced shapes look like a drawing, not a photo.
- **Light**: diffuse daylight or two lamps at 45°, no direct flash (black glossy plastic shows
  highlights that hide edges). LEDs **on** for one set (to mark their exact position), off for
  another.
- **Background**: plain, light and matte (a sheet of paper), contrasting with the black device.
- **Resolution**: full sensor resolution, no filters, no portrait-mode blur. Keep the originals.

## Input tester (test page)

Requested 2026-10-28, modelled on the Test tab of the Windows driver's control panel: live view of every axis, button, toggle, hat and the mode selector, read
from evdev without grabbing the device (games keep receiving input). S4 already showed evdev works
in the sandbox with `--device=all`.

Layout proposal:

- **Axes**: X/Y as a square with a dot (stick), Z rotation (twist), throttle Z, rotary X/Y, slider,
  each as a bar with its raw value; the ministick ("mouse") as a second square.
- **Buttons**: the button positions on the same `HotspotView` artwork, lighting up when pressed (the
  same coordinates idea as the LED editor), plus a compact numbered grid as a fallback that does not
  depend on artwork.
- **Hats**: POV 1 (stick), POV 2 and the throttle hat as 8-direction indicators.
- **Toggles and mode**: T1–T6 and the mode selector (1/2/3) as their own indicators.
- **MFD buttons** (Start/Stop, Reset, function wheel) if they come through evdev (S3 checks it).

Out of scope or later:

- Deadzones (the old Windows driver had per-axis deadzones): evdev exposes `flat` per axis and it can
  be changed with `EVIOCSABS`, but most games read their own deadzones and some ignore `flat`; it would
  also have to be re-applied on every connect. Possible 1.x item after S3 checks what games honour.
- Clutch mode: needs a virtual input device (`uinput`) to swallow and re-map buttons, which means
  `/dev/uinput` access and a much bigger scope. Not planned.

Release: the tester was a 1.x item; with the hotspot widget shared with the LED editor it is cheap
enough to consider for 1.0.0 (decide in the roadmap, Stage C).

## Not done yet

- Hands-on check: popover selection, keyboard navigation (Tab between LEDs, Enter opens the popover),
  screen reader labels, narrow window.
- Photos and traced artwork.
- Input tester prototype: depends on S3 (key codes, mode selector, MFD buttons on evdev).
