# S3: input map (X52 Pro, evdev)

Tool: `spikes/x52-input` on the `spikes` branch (`evdev` 0.13), reading
`/dev/input/event*` without grabbing while the kernel's `usbhid` / `hid-generic` driver stays bound.
Every control pressed or moved by hand on 2026-11-02, one at a time, in a fixed order; the log was
matched against that order. Device name: "Logitech X52 Professional H.O.T.A.S." (`06a3:0762`).

## Buttons (39)

| Code | Control | Code | Control |
|---|---|---|---|
| 288 | Trigger, first stage | 706 | Scroll wheel press (mouse secondary) |
| 289 | Fire (under the flip cover) | 707 | POV 2 (top-left hat on the stick head), up |
| 290 | A | 708 | POV 2 right |
| 291 | B | 709 | POV 2 down |
| 292 | C | 710 | POV 2 left |
| 293 | Pinkie switch | 711 | Throttle hat, up |
| 294 | D | 712 | Throttle hat, right |
| 295 | E | 713 | Throttle hat, down |
| 296 | T1 (switch 1 up) | 714 | Throttle hat, left |
| 297 | T2 (switch 1 down) | 715 | Mode 1 (held) |
| 298 | T3 (switch 2 up) | 716 | Mode 2 (held) |
| 299 | T4 (switch 2 down) | 717 | Mode 3 (held) |
| 300 | T5 (switch 3 up) | 718 | i (clutch) |
| 301 | T6 (switch 3 down) | 719 | Function (left MFD wheel press) |
| 302 | Trigger, second stage (288 stays held) | 720 | Start/Stop |
| 303 | Mouse button next to the ministick | 721 | Reset |
| 704 | Scroll wheel, one notch down | 722 | Left MFD wheel (Function) scrolled up: Page Up |
| 705 | Scroll wheel, one notch up | 723 | Left MFD wheel scrolled down: Page Down |
| | | 724 | Right MFD wheel (Select) scrolled up: Up |
| | | 725 | Right MFD wheel scrolled down: Down |
| | | 726 | Select (right MFD wheel press) |

The codes are the HID report order (first 16 on `BTN_JOYSTICK` 288–303, the rest on
`BTN_TRIGGER_HAPPY1`... 704–726), the same order libx52's X52 Pro report parser uses. The three
toggle switches are three physical switches with two positions each (T1/T2, T3/T4, T5/T6), matching
the three toggle LEDs. Wheels (throttle scroll wheel and both MFD wheels) send a short press per
notch, not a held button.

## Axes

| evdev axis | Control | Range | Notes |
|---|---|---|---|
| `ABS_X` / `ABS_Y` | Stick left-right / forward-back | 0–1023, centre ~511 | quiet at rest |
| `ABS_RZ` | Stick twist | 0–1023 | reached 0–1010 on this unit |
| `ABS_Z` | Throttle lever | 0–255 | jitters ±1–2 at rest, ~40 events/s |
| `ABS_RX` | Rotary next to the i button | 0–255 | jitters ±1–2 at rest, ~50 events/s |
| `ABS_RY` | Rotary next to the E button | 0–255 | jitters ±1–2 at rest |
| `ABS_THROTTLE` | Slider on the throttle | 0–255 | flickers between two adjacent values at rest |
| `ABS_MISC` (40) / 41 | Ministick (the small thumb stick, "mouse nub") X / Y | 0–15, centre 8 | X grows to the left (S9) |
| `ABS_HAT0X` / `ABS_HAT0Y` | POV 1 (lower hat, between B and C) | −1, 0, 1 | standard: X −1 left, Y −1 up |

POV 2 and the throttle hat are buttons, not hat axes. Names used from here on (they match GX52
0.7, whose only hat LED was called "POV 2", and the old issue #3 "No POV1 LED Control"):

- **POV 1**: the lower hat between B and C; hat axes; its LED is fixed green (see
  [01-usb-control.md](01-usb-control.md)).
- **POV 2**: the top-left hat on the stick head; buttons 707–710; LED bits 16/17 (red/green).
- **Throttle hat**: buttons 711–714; no LED.

## Mode selector

Each position is a held button (715/716/717); the current one is in the key state at start-up
(`EVIOCGKEY`), so the app knows the mode without waiting for a change. Turning the selector bounces:

```
seconds  code value
10.990   715  0     11.006 715 1     11.254 715 0
11.327   716  1     11.383 716 0 ... (4 more pulses) ... 11.471 716 1   settled in mode 2
17.079   716  0     17.151 717 1                                      70 ms with no mode held
22.016   716  1 ... 22.088 716 0     22.128 715 1                     3 → 1 passes through mode 2
```

Consequences for per-mode profiles: debounce, and act only when one mode button has been held for
about 200 ms; ignore the gaps with no mode held; don't apply mode 2 while passing from 3 to 1.

## Input while the app writes to the device

`x52-input --abs` ran while `x52-probe` sent both brightness commands and the full LED `cycle` (about
60 control transfers over 5 s). The rotaries' jitter gives a steady event stream to watch: 40–60
events per second before, during and after the transfers, largest gap 168 ms (the same as at rest),
`usbhid` still bound afterwards. Control transfers and evdev reads do not disturb each other, so the
device writer and the input reader can be independent.

## Consequences for the app

- The input tester and the LED editor share the names above; the axis noise means the tester should
  show values without animating every ±1 change (or smooth them), and nothing should log per event.
- Clutch (718), the MFD buttons and the mode selector all come through evdev, so features that react
  to them (per-mode profiles, MFD pages switched by the wheels) need no USB reads.
- The X52 (non-Pro) has a different report order (libx52's other table); its map needs a tester with
  that device.
