# S7: LED editor

Spike `spikes/s7-led-editor` on the `spikes` branch: an `adw::Application` with one page per part of
the device (stick, stick base, throttle) in an `adw::ViewStack`. Each page placed one round button
per LED over a placeholder outline of the device; the button opens a popover with an
`adw::ToggleGroup` of the LED's states. Built against the required versions (`gtk4` 0.11.5
`gnome_50`, `libadwaita` 0.9.2 `v1_9` + `gtk_v4_22`) and run on the Ubuntu 26.04 host (GTK 4.22.4)
on 2026-10-28.

Run it with `cargo run`; set `GX52_SPIKE_SCREENSHOT_DIR=<dir>` to render every page to
`<dir>/<page>.png` and quit (used to check the result without a person at the screen).

## Decision: no device artwork (2026-11-09)

The spike drew the LEDs over a picture of the device, and the plan was to replace the placeholder
with drawings traced from photos. That plan is dropped: drawing and maintaining artwork for every
view costs more than it gives for 10 editable LEDs, and the editor needs a palette, hotspots and
coordinates that work at every size and in both styles. The app ships **no pictures or drawings of
the device**: no photos, no SVG artwork, no coordinate tables. The LED editor is a **schematic
layout** of stock widgets (below), chosen from three mockups on 2026-11-09 over a list of preference
rows and a grid of LED cards.

What the spike proved and the app keeps: the round LED button (a `gtk::MenuButton` with a popover of
states, styled by state), the LED data (bits, states, the fixed POV 1 LED) and the device worker
(live preview, replay on connect). The `HotspotView` widget that positioned buttons on the artwork
is not used.

## LED button

Each LED is a `gtk::MenuButton` with an empty child. Its colour comes from CSS classes on the
`menubutton` node (`led-off`, `led-on`, `led-green`, `led-red`, `led-amber`) styled with libadwaita's
palette variables (`--green-3`, `--red-3`, `--yellow-3`; the app uses the prototype's `--green-3`,
`--red-2`, `--yellow-4`, see [ui.md](../ui.md#leds-view)); the tooltip and the accessible label say
"<LED> LED: <state>".

Findings:

- **A `MenuButton` is two CSS nodes**: `menubutton` (gets the widget's classes) wrapping `button`
  (gets the look). The first version styled `button.led` and showed default buttons; the selector has
  to be `menubutton.led > button`.
- **A `MenuButton` without label, icon or child draws a drop-down arrow** even with
  `always-show-arrow` false. Setting an empty `gtk::Box` as child gives a plain round button.

## Design for the app

- **One card per part of the device**: Stick, Stick base, Throttle. Inside each card the LED
  buttons sit in a `gtk::Grid` that roughly follows where the LEDs are on the device, each with its
  name below it:

  | Card | Rows |
  | --- | --- |
  | Stick | POV 2, Fire; A, B; POV 1 |
  | Stick base | T1/T2, T3/T4, T5/T6 |
  | Throttle | D, E; i, Throttle |

  The cards sit side by side when the window is wide and stack when it is narrow (a breakpoint or a
  wrapping box); the grid inside a card does not change.
- A click (or Enter / Space) on an LED opens a popover with its states: Off / Green / Amber / Red for
  the bi-colour LEDs, Off / On for Fire and Throttle. Choosing a state closes the popover.
- **POV 1** is shown as a fixed green LED that does not open a popover; its tooltip and accessible
  label explain that it cannot be changed and follows brightness and blink
  ([protocol.md](../protocol.md#led-bits)).
- **Brightness and blink** are regular rows below the cards.
- **The LED shown follows the brightness.** When LED brightness is `0`, every LED is dark on the
  device (POV 1 included), so the editor shows every LED as off, whatever its state. The states are
  kept: the popover still shows and changes the chosen state, the accessible label says
  "<LED> LED: <state>, brightness 0", and the LEDs light again as soon as brightness goes above `0`.
  Above `0` the button shows the state colour at full strength; it does not dim with brightness.
- The editor edits the active profile; changes go to the device immediately and are saved
  automatically ([ui.md](../ui.md#profile-editor)).
- The grid positions are data (LED, card, row, column) next to the LED bits, so moving an LED does
  not touch widget code.

## Input tester (test page)

Requested 2026-10-28, modelled on the Test tab of the Windows driver's control panel: live view of
every axis, button, toggle, hat and the mode selector, read from evdev without grabbing the device
(games keep receiving input). S4 already showed evdev works in the sandbox with `--device=all`.

Layout (decided 2026-11-09 from the maintainer's mockup): a "Test input" row in the sidebar opens
the page in the content area, in place of the profile editor; the active profile stays on the
device.
Grouped by kind of input, not by part of the device:

- **Axes**: stick X/Y as a square with a dot, throttle Z as a vertical bar, X/Y/Z rotation and the
  slider as horizontal bars, each with its raw value.
- **Ministick**: a second square (mouse X / mouse Y).
- **Toggles**: T1–T6 in pairs; **Mode**: 1 / 2 / 3, the current one highlighted.
- **Buttons**: one indicator per button that lights up while pressed, labelled with the name on the
  device and the button number games show (for example "Fire · 2"); the evdev code in the tooltip.
  The MFD buttons and wheels are included (they come through evdev, S3).
- **POVs**: POV 1, POV 2 and the throttle hat as 3×3 direction grids.

Out of scope or later:

- Deadzones (the old Windows driver had per-axis deadzones): evdev exposes `flat` per axis and it can
  be changed with `EVIOCSABS`, but most games read their own deadzones and some ignore `flat`; it would
  also have to be re-applied on every connect. Possible 1.x item after S3 checks what games honour.
- Clutch mode: needs a virtual input device (`uinput`) to swallow and re-map buttons, which means
  `/dev/uinput` access and a much bigger scope. Not planned.

Release: moved into 1.0.0 on 2026-11-09. It is also how the X52 non-Pro input map gets
verified.

## Hands-on check (2026-11-02)

With the editor wired to the device (worker thread with an `mpsc` channel; it opens the device, sends
brightness and replays the full LED state on every connect): every LED follows the editor at once,
and after unplugging, changing LEDs and plugging back in, the device catches up within about a second
(brightness included). The popover UI worked on all pages.

Missing in the spike: the POV 1 LED (lower hat between B and C). It cannot be controlled
([01-usb-control.md](01-usb-control.md)); the app shows it as a fixed green LED.

## Not done yet

- Keyboard navigation, screen reader labels, narrow window, light/dark: skipped on purpose
  (2026-11-02) and checked on the app's editor instead ([roadmap M5](../roadmap.md)). This spike
  provides the LED button, the data (LED bits, states) and the device logic (worker thread, replay
  on connect).
- Input tester prototype: depends on S3 (key codes, mode selector, MFD buttons on evdev).
