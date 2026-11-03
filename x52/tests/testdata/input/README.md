# Input fixtures

## `mode-bounce.evtest`

The mode selector of an X52 Pro (`06a3:0762`, "Logitech X52 Professional H.O.T.A.S."), recorded
on 2026-11-03 with `evtest` 1.36 for the mode debounce of the `x52` crate
(`x52/src/input/tests.rs`). It replaces the elided excerpt of
[spike S3](../../../../docs/spikes/03-input.md#mode-selector) as the debounce's fixture.

Procedure:

1. The selector was put in mode 1 before the capture started.
2. `evtest /dev/input/by-id/usb-Logitech_X52_Professional_H.O.T.A.S.-event-joystick` ran for 120
   seconds, reading the device without grabbing it (the user's session has read access through
   the `uaccess` ACL).
3. The selector was moved 1 → 2 → 3 → 1 at normal speed, three times, without touching the stick
   or the throttle.

Format: evtest's output. A header lists the device and its event codes, then one line per event:

```text
Event: time 1793716445.329008, type 1 (EV_KEY), code 715 (BTN_TRIGGER_HAPPY12), value 0
```

The time is the event's timestamp in seconds and microseconds; `value` is `1` for a press and `0`
for a release. Codes 715, 716 and 717 are modes 1, 2 and 3. Each key event follows a scan-code
event (`type 4 (EV_MSC)`).

Filtered: the raw capture was 1.2 MB, almost all of it the axis noise of the rotaries and the
throttle (docs/protocol.md, "Axes"). The axis events and the `SYN_REPORT` lines between events were
removed with

```sh
grep -v -e '(EV_ABS)' -e 'SYN_REPORT' raw.evtest > mode-bounce.evtest
```

Every key and scan-code event is as captured. The capture cannot show the key state at its start;
its first mode event is the release of 715, so it starts in mode 1.

What the debounce gives (200 ms held alone): 2, 3, 1 in the first two passes; in the third, mode 2
was held for only 160 ms on the way, so 3 follows 1. Every pass from 3 to 1 goes through 2 with
pulses of 8–64 ms, and 2 is never applied there.
