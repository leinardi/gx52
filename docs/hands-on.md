# Hands-on session checklist

Everything in the preparation that needs a person at the device or the desktop: photos, button
presses, plugging cables, looking at the screen. Collected here so it can be done in one sitting.
Tick items off and record the results in the linked spike documents.

Before starting: X52 Pro connected, udev rule installed (`/etc/udev/rules.d/70-gx52.rules`),
`spikes` branch checked out in the `gx52-spikes` worktree.

## 1. Photos for the device artwork (S7)

The LED editor and the input tester draw the device as line art traced from photos. The photos are
only a tracing source and are not shipped; the traced SVGs are. Use only your own photos, no
Logitech or Saitek product images or manual scans (copyright).

### Shots needed

No single viewpoint shows every LED and button, so the app uses several views. Take each shot twice:
once with all LEDs **on**, to mark their exact positions, and once with them **off**. After
plugging in, the throttle LEDs and the MFD backlight stay off until brightness is sent, so start
with `x52-probe bright led 128`, `x52-probe bright mfd 128`, then `x52-probe leds green`,
`x52-probe led fire on` and `x52-probe led throttle on`.

| # | View | Must show |
|---|---|---|
| 1 | Stick, front-top (looking down at the head, trigger side away) | Buttons A, B, Fire (with cover), POV 1 hat, POV 2 hat and its LED, C button |
| 2 | Stick, left side | Trigger (both stages), pinkie switch, side buttons |
| 3 | Stick base, front | Toggles T1–T6 and the three LEDs (T1/T2, T3/T4, T5/T6), mode selector |
| 4 | Throttle, right/top | Buttons D, E, i, mini-stick, scroll wheel, rotaries, MFD and its buttons |
| 5 | Throttle, left (thumb side) | Thumb buttons and hat, slider, throttle-axis LED |
| 6 | Whole device, both parts side by side | Proportions between stick and throttle |

Add any extra angle where a button or LED is hidden in the shots above; better one photo too many.

### How to shoot

- **Distance and zoom**: stand back and use 2× to 5× optical zoom on the phone, with the lens square
  to the main face of the part. Less perspective makes the traced shapes look like a drawing.
- **Light**: diffuse daylight or two lamps at about 45°, **no flash**. The glossy black plastic
  turns direct light into highlights that hide edges.
- **Background**: plain, light and matte (a sheet of white paper or card), so the black device
  stands out.
- **Camera settings**: full resolution, no filters, no portrait mode / background blur, no HDR
  "beautify" processing if it can be turned off.
- **Stability**: rest the phone on something or use a timer; sharp edges matter more than framing.
- **Files**: keep the originals, put them in one folder (for example `~/Scaricati/x52-artwork/`)
  named by view number (`1-stick-front-leds-on.jpg`, ...).

## 2. Input mapping (S3)

Run `spikes/x52-input` and, in order:

- [ ] Press every button once, in a fixed order (stick, then throttle, then base); note the key code
      shown for each. Gives the button order map and the names for the input tester.
- [ ] Move every axis to both ends: stick X/Y, twist, throttle, both rotaries, slider, mini-stick.
      Note the axis codes and ranges.
- [ ] All three hats in the 8 directions (POV 1, POV 2, throttle hat).
- [ ] Mode selector through 1, 2, 3: confirm it is a held key (715 = mode 1 seen at start).
- [ ] MFD buttons (Start/Stop, Reset, function wheel up/down/press): do they appear on evdev at all?
- [ ] Clutch / i button and the shift-related buttons, if any.
- [ ] While `x52-input` runs, change LEDs with `x52-probe` and confirm input keeps flowing
      (control transfers and evdev at the same time).

Results go to a new `docs/spikes/03-input.md` and later to `docs/protocol.md`.

## 3. Hotplug (S3 and S4)

- [ ] Host: unplug and replug while the hotplug watcher runs; does `nusb::watch_devices` see both
      events?
- [ ] Flatpak (`--device=all`, no network): same test. Expected: the udev netlink events do not
      arrive; check whether the inotify fallback on `/dev/bus/usb` and `/dev/input` sees the change.
- [ ] After replug, confirm the app re-sends brightness (MFD backlight and throttle LEDs come back).
- [ ] Remove the udev rule temporarily (or test as another user) and note the exact error, for the
      in-app guidance page.

Results go to [spikes/04-flatpak-sandbox.md](spikes/04-flatpak-sandbox.md).

## 4. Tray (S5)

- [ ] Run `spikes/s5-tray`: icon visible in the panel, menu looks right, click opens/hides the window.
- [ ] Same in the Flatpak build.
- [ ] Background portal: first request shows GNOME's permission dialog; accept, then check autostart
      after logging out and in.
- [ ] Optional: KDE Plasma (native watcher), if a session is at hand.

Results go to [spikes/05-tray.md](spikes/05-tray.md).

## 5. LED editor (S7)

Run `spikes/s7-led-editor` with the device connected (the spike does not drive the device yet; this
checks the UI only).

- [ ] Click each LED: the popover opens, choosing a state changes the colour.
- [ ] Keyboard only: Tab moves between LEDs, Enter/Space opens the popover, arrows choose a state,
      Escape closes it.
- [ ] Resize to the narrowest window (360×294): LEDs stay on the drawing and remain clickable.
- [ ] Light and dark style (Settings > Appearance): the outline and colours stay readable.
- [ ] Optional: Orca screen reader reads "<LED> LED: <state>".

Results go to [spikes/07-led-editor.md](spikes/07-led-editor.md).
