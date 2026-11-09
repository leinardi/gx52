# Hands-on session checklist

Everything in the preparation that needs a person at the device or the desktop: button presses,
plugging cables, looking at the screen. Collected here so it can be done in one sitting.
Tick items off and record the results in the linked spike documents.

Before starting: X52 Pro connected, udev rule installed (`/etc/udev/rules.d/70-gx52.rules`),
`spikes` branch checked out in the `gx52-spikes` worktree.

## 1. Input mapping (S3)

Run `spikes/x52-input` and, in order:

- [x] Press every button once, in a fixed order (stick, then throttle, then base); note the key code
      shown for each. Gives the button order map and the names for the input tester.
- [x] Move every axis to both ends: stick X/Y, twist, throttle, both rotaries, slider, mini-stick.
      Note the axis codes and ranges.
- [x] All three hats in the 8 directions (POV 1, POV 2, throttle hat).
- [x] Mode selector through 1, 2, 3: confirm it is a held key (715 = mode 1 seen at start).
- [x] MFD buttons (Start/Stop, Reset, function wheel up/down/press): do they appear on evdev at all?
- [x] Clutch / i button and the shift-related buttons, if any. (2026-11-02: results in
      [spikes/03-input.md](spikes/03-input.md).)
- [x] While `x52-input` runs, change LEDs with `x52-probe` and confirm input keeps flowing
      (control transfers and evdev at the same time).

Results go to a new `docs/spikes/03-input.md` and later to `docs/protocol.md`.

## 2. Hotplug (S3 and S4)

- [x] Host: unplug and replug while the hotplug watcher runs; does `nusb::watch_devices` see both
      events?
- [x] Flatpak (`--device=all`, no network): same test. Expected: the udev netlink events do not
      arrive; check whether the inotify fallback on `/dev/bus/usb` and `/dev/input` sees the change.
- [x] After replug, confirm the app re-sends brightness (MFD backlight and throttle LEDs come back).
      Checked on 2026-11-07 with the device service (`gx52 --device-check replug`, host and Devel
      Flatpak): brightness, every LED, the MFD lines and the clocks come back.
- [x] Remove the udev rule temporarily (or test as another user) and note the exact error, for the
      in-app guidance page.

Results go to [spikes/04-flatpak-sandbox.md](spikes/04-flatpak-sandbox.md).

## 3. Tray (S5)

- [x] Run `spikes/s5-tray`: icon visible in the panel, menu looks right, click opens/hides the window.
      (2026-11-02: single left-click opens the menu on GNOME, double-click toggles; see S5.)
- [ ] Same in the Flatpak build.
- [ ] Background portal: first request shows GNOME's permission dialog; accept, then check autostart
      after logging out and in.
- [ ] Optional: KDE Plasma (native watcher), if a session is at hand.

Results go to [spikes/05-tray.md](spikes/05-tray.md).

## 4. Virtual mouse (S9)

Run `spikes/s9-virtual-mouse` (from its directory: `cargo run --release`).

- [x] Ministick moves the pointer in the right directions; small pushes move slowly, full push fast.
      (2026-10-28: feel is good as is.)
- [x] Note whether speed and curve feel right compared with the Windows driver (adjust `GAIN`,
      `CURVE`, `DEADZONE` in the source and re-run).
- [x] Primary mouse button clicks; wheel scrolls (direction right?); wheel press right-clicks.
- [x] Works in a Wayland session (default) and, if at hand, in an X11 session.
- [x] With a game running: the pointer moves, and note whether the game also reacts to the
      ministick axes.
- [x] Unplug while holding the primary button: the spike exits; check no button stays stuck.

Results go to [spikes/09-virtual-mouse.md](spikes/09-virtual-mouse.md).

## 5. LED editor (S7)

Run `spikes/s7-led-editor` with the device connected (the spike does not drive the device yet; this
checks the UI only).

- [x] Click each LED: the popover opens, choosing a state changes the colour. (2026-11-02, also
      on the device, including catch-up after replug.)
- [-] (skipped: UI to be redesigned) Keyboard only: Tab moves between LEDs, Enter/Space opens the
      popover, arrows choose a state, Escape closes it.
- [-] (skipped: UI to be redesigned) Resize to the narrowest window (360×294): every LED stays
      visible and clickable.
- [-] (skipped: UI to be redesigned) Light and dark style (Settings > Appearance): the LED colours
      and labels stay readable.
- [-] (skipped: UI to be redesigned) Optional: Orca screen reader reads "<LED> LED: <state>".

Results go to [spikes/07-led-editor.md](spikes/07-led-editor.md).
