# S9: virtual mouse from the throttle ministick

Question (added 2026-10-28): can GX52 make the ministick on the X52 Pro throttle move the mouse
pointer, as the Windows driver does? Out of the box on Linux the ministick is only two joystick axes
and the mouse buttons are joystick buttons.

Spike `spikes/s9-virtual-mouse` on the `spikes` branch (`evdev` 0.13.2, no other dependencies).

## Prior art

- **libx52's `x52d`** (nirenjan/libx52, maintained, `GPL-2.0-only WITH Classpath-exception-2.0`)
  has a virtual mouse on Linux (`daemon/mouse_handler.c`, config section `[Mouse]`). It reads the
  device through its own HID parser (`libx52io`, hidraw) and writes to a uinput mouse through its
  `vkm` library. Its options: `Enabled`, `Sensitivity` (10–500 %), `ReverseScroll`,
  `IsometricMode` (speed from the combined X/Y distance like a laptop nub, instead of per axis),
  `CurveFactor` (1 linear to 5 aggressive), `Deadzone` (0–11). It runs a 10 ms motion loop with a
  sub-pixel accumulator and a radial deadzone. Read for understanding only; licence not compatible
  with copying into a GPL-3.0-or-later app, and nothing was copied. Its button table also confirmed
  the X52 Pro report order (below).
- **AntiMicroX** (generic joystick-to-mouse/keyboard mapper) can map the two ministick axes to the
  pointer; users on forums do this today.
- **Proton issue ValveSoftware/Proton#6537**: in Proton 7+ moving the ministick makes games see
  random button presses; a user's workaround is a python-evdev script that grabs the joystick and
  re-emits everything except the ministick axes through uinput. Shows the second, harder half of
  what the Windows driver does: hiding the mouse controls from the joystick.
- No USB protocol is involved: the Windows driver does this in software too. There is nothing to
  send to the device.

## Device facts (X52 Pro, from evdev on the host)

| Control | evdev |
|---|---|
| Ministick X / Y | `ABS_MISC` (40) / code 41, range `0..=15`, centre 8, no fuzz or flat |
| Mouse primary button (next to the ministick) | key 303 |
| Scroll wheel down / up (one press per detent) | keys 704 / 705 |
| Scroll wheel press (secondary button) | key 706 |
| Clutch (i button) | key 718 |

The 16-step resolution is the device's own (the HID report has 4 bits per axis), not an evdev
limit. The key codes follow the HID report order (first 16 buttons on `BTN_JOYSTICK` 288–303, the
rest on `BTN_TRIGGER_HAPPY1`... 704–726); libx52's report table gives the same order and puts
mode 1 at 715, which matches what S2 saw. Still to confirm by pressing each button (S3,
[hands-on.md](../hands-on.md)).

## What the spike does

- Opens the joystick's evdev node **without grabbing it**, so games still see every axis and button.
- Creates a uinput device "GX52 virtual mouse" with `BTN_LEFT`, `BTN_RIGHT`, `REL_X`, `REL_Y`,
  `REL_WHEEL`, `REL_WHEEL_HI_RES`.
- Every 10 ms turns the ministick offset into pointer speed: deadzone of 1 step, then
  `gain × offset^1.6` pixels per tick per axis, with fractions carried over between ticks (so small
  offsets still move slowly) and dropped when the stick returns to the centre.
- Primary button to left click, wheel press to right click, each wheel detent to one wheel step
  (`REL_WHEEL` ±1 and `REL_WHEEL_HI_RES` ±120).
- `--self-test` only creates the device and reports its node.

## Results

| Check | Result |
|---|---|
| Create the uinput mouse as the logged-in user, host | OK. `/dev/uinput` has an ACL for the user here because Steam's `steam-devices` rule (`60-steam-input.rules`) tags it `uaccess` |
| Kernel classification | Registered with `mouse1` and `event23` handlers (`EV=7`, `REL=903`, `KEY` left/right): a normal pointer for libinput and the compositor, X11 or Wayland |
| Virtual device's own event node | root-only, as for any input device; nothing else in the session can read it |
| GNOME 51 sandbox with `--device=all`, no network | OK, same as the host |
| Sandbox without `--device=all` | fails: `/dev/uinput` does not exist in the sandbox |
| Pointer moves, clicks and scrolls with the ministick (host, GNOME Wayland, 2026-10-28) | OK after inverting X: the ministick's X axis grows to the **left**. Left click, right click and wheel direction correct. Speed and curve felt "already very good" with the values above |

## Findings for the app

- **Feasible with what the app already has**: evdev reading (S3/S4) plus uinput writing, both
  through the `evdev` crate, both inside the existing `--device=all` permission. No portal, no
  extra Flatpak permission, no root daemon.
- **Permission on `/dev/uinput`**: by default it is `root:root 0600`. It works here only because
  `steam-devices` is installed. GX52's udev guidance (R4) would need a second rule line:
  `KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"` (the line
  `steam-devices` uses). Security note for the guidance page: write access to uinput lets any
  program of the logged-in user inject keyboard and mouse input; Steam's rule already grants exactly
  this on most gaming machines. The app should detect a missing permission and explain it, and the
  feature stays off by default.
- **Runs on its own thread** (blocking reads plus a 10 ms timer, or one `poll` loop), never on the
  GTK main thread; it must survive unplug/replug (re-open both, release held buttons on unplug).
- **The joystick still reports the ministick and mouse buttons.** Games that bind every axis may
  react to both the pointer and the axes (the Proton issue). Hiding them needs the joystick to be
  grabbed (`EVIOCGRAB`) and re-exposed as a virtual joystick without those controls, which is the
  same machinery as "clutch mode" and much bigger in scope (and changes the device games see).
  Not for the first version; maybe an option later.
- **Settings worth exposing** (the same knobs x52d and the Windows driver offer): enable, speed,
  curve (or acceleration), deadzone, invert wheel; maybe "isometric" (speed from the combined
  distance) vs per-axis.
- **Conflict with `x52d`**: if libx52's daemon runs with its mouse enabled, both move the pointer.
  Belongs to risk R11 (detect `x52d` and warn).
- **Alternative not chosen**: the RemoteDesktop portal (`NotifyPointerMotion`) needs no udev rule,
  but shows a "remote control" consent dialog and an on-screen indicator, depends on the desktop's
  portal backend, and is meant for remote desktop, not input devices. uinput is the standard way
  (Steam Input, AntiMicroX, input-remapper all use it).

## Decision

Taken 2026-10-28 after the hands-on test: **in 1.0.0**, with only an on/off switch and the spike's
values (deadzone 1 step, gain 0.6 px per tick, curve 1.6, 10 ms tick, X inverted). The switch stays
because the feature needs the uinput permission and some games may react to the ministick axes
too. Speed, curve, deadzone and wheel direction settings come in 1.x.

## Not done yet

- X11 session and a running game.
- Behaviour on unplug while a button is held.
- Whether the X52 (non-Pro) reports the ministick on the same codes (libx52's non-Pro table orders
  the mouse buttons differently).
