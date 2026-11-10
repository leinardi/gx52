# X52 protocol

What the Logitech (Saitek) X52 and X52 Pro do, as far as GX52 relies on it. This document is
normative: the app sends only the commands listed in
[Commands the app may send](#commands-the-app-may-send), and AGENTS.md's "never guess at the device"
rule points here.

Every value comes from a measurement on an X52 Pro (`06a3:0762`, firmware as shipped) recorded in
the spike documents, or from GX52 0.7 where the spikes confirmed it:

- [S1/S2 USB control](spikes/01-usb-control.md): vendor commands, power-on state, POV 1 LED;
- [S2 MFD character set](spikes/02-mfd-charset.md): how the glyph table was read;
- [S3 input map](spikes/03-input.md): evdev buttons, axes, hats, mode selector;
- [S4 Flatpak sandbox](spikes/04-flatpak-sandbox.md): access states, hotplug;
- [S9 virtual mouse](spikes/09-virtual-mouse.md): uinput.

The probe that sent the commands is `spikes/x52-probe` on the `spikes` branch.
[libx52](https://github.com/nirenjan/libx52) is the second reference used to cross-check (character
set, report order, X52 non-Pro differences); nothing is copied from it (GPL-2.0-only).
The X52 (non-Pro) has not been tested; see [X52 (non-Pro)](#x52-non-pro).

## Device IDs and access

| Vendor | Product | Device | Device type |
| --- | --- | --- | --- |
| `06a3` | `0762` | X52 Pro Flight Control System | `x52-pro` |
| `06a3` | `0255` | X52 Flight Controller | `x52` |
| `06a3` | `075c` | X52 Flight Controller | `x52` |

A device is only driven when its vendor and product ID are in this table.

The joystick has one interface, bound to the kernel's `usbhid` driver. Vendor control transfers go
to the device without claiming or detaching that interface, so the joystick keeps working for games
while the app writes to it (S1, and S3 for input during writes).

### udev rule

Without a rule the USB node is `root:root 0664` and opening it fails with `EACCES`. The in-app
guidance page shows this file, `/etc/udev/rules.d/70-gx52.rules` (the number must stay below 73
for `uaccess` to apply):

```
SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="06a3", ATTR{idProduct}=="0762|0255|075c", TAG+="uaccess"
KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"
```

- The first line gives the logged-in user an ACL on the raw USB node (S1). It is needed for LEDs,
  brightness, MFD text and clocks.
- The second line gives the same user write access to `/dev/uinput`, the line `steam-devices` uses
  (S9). It is needed only for the virtual mouse. Security note for the guidance page: write access
  to uinput lets any program of the logged-in user inject keyboard and mouse input.
- The evdev node (`/dev/input/event*`) needs no rule: systemd's joystick `uaccess` rule covers it
  (S1, S4).

After installing or restoring the rule, `udevadm control --reload && udevadm trigger` is enough: the
ACL comes back on the existing node **without replugging** and `open` works at once (S4). So the
guidance page says there is no need to unplug, and the app retries `open` while it is in the "no
permission" state (on each device poll, or when the user presses "Try again").

### Access states

The app tells these states apart without guessing (S4, host and sandbox behave the same):

| State | How it is detected | What works |
| --- | --- | --- |
| No device | No matching vendor/product ID in the USB device list (sysfs) | Nothing |
| Found, no permission | Listed, but opening the device fails with `EACCES` (`io::ErrorKind::PermissionDenied`) | Input (evdev); the udev guidance page says LEDs, MFD and clocks are unavailable, not "device not found" |
| Ready | Listed and `open` succeeds | Everything |

The uinput permission is a separate state, checked only when the virtual mouse is enabled:
`/dev/uinput` is `root:root 0600` by default, and creating the virtual device fails without the
second rule line. A missing uinput permission does not change the USB state above and does not
affect LEDs, MFD or clocks.

### libx52's x52d

libx52's `x52d` daemon drives the same LEDs, MFD and clocks, so the two would overwrite each other
(risk R11 of the rewrite analysis). GX52 does not stop it; it warns while it runs.

- **Socket:** `x52d` listens on a Unix stream socket, by default
  `$(prefix)/$(localstatedir)/run/x52d.cmd` (`daemon/constants.h`, `include/meson.build` and
  `daemon/protocol.dox` of [nirenjan/libx52](https://github.com/nirenjan/libx52) at `2da88f4`;
  `-s` overrides it, the shipped systemd unit does not). A distribution package (prefix `/usr`)
  has it at `/var/run/x52d.cmd`, which is `/run/x52d.cmd`; a source build with Meson's default
  prefix at `/usr/local/var/run/x52d.cmd`. GX52 checks all three.
- **Check:** a nonblocking `connect` to the socket. A successful connection (or a full backlog) is a
  running daemon; a refused connection is a stale file left by a daemon that died, not a running
  one. A daemon running as root under the default umask leaves a socket mode `0755`, which a
  normal user may not connect to (`EACCES`, seen on 2026-11-07 with a root `socat` listener). Then
  the pid file next to the socket (`x52d.pid`, also in `daemon/constants.h`) decides: running if
  `/proc/<pid>/comm` is `x52d`. Without a readable pid file, as in the sandbox, which exposes only
  the socket, a socket that refuses this user counts as a running daemon. The device watch checks
  at start and every 5 s.
- **Sandbox:** the sandbox has its own `/run`, so the Flatpak exposes the socket read-only with
  `--filesystem=/run/x52d.cmd:ro` (accepted by `flatpak-builder-lint`; a host socket exposed this
  way accepts connections from the sandbox, checked on 2026-11-03). Two limitations remain: Flatpak
  exposes a path only if it exists when the app starts, so a daemon started after GX52 is seen
  after GX52 restarts; and `/usr/local` cannot be exposed, so a source build's daemon is seen only
  by a GX52 that is not sandboxed.

## USB vendor commands

Every command is a vendor control transfer to the device with no data stage:

- `bmRequestType = 0x40` (host-to-device, vendor, recipient device), `bRequest = 0x91`;
- `wIndex` is the command, `wValue` its argument;
- timeout 5 s (GX52 0.7); `nusb::Device::control_out` with `ControlType::Vendor` and
  `Recipient::Device` (S1).

| Command | `wIndex` | `wValue` encoding | Values | Observed behaviour (X52 Pro, S2) |
| --- | --- | --- | --- | --- |
| MFD brightness | `0xb1` | level | `0`–`128` | `0` backlight off, `16` very dim, `128` full |
| LED brightness | `0xb2` | level | `0`–`128` | One value for every LED, stick and throttle, POV 1 included. `0` off, `128` maximum; `255` looks the same as `128`, so the app never sends more than `128` |
| Blink | `0xb4` | `0x51` on, `0x50` off |  | The **i** button LED (throttle) and the **POV 1** LED (lower stick hat) blink. Nothing else blinks |
| LED on/off | `0xb8` | `bit << 8 \| state` | `bit` from the [LED bit table](#led-bits); `state` `1` on, `0` off | Each bit is one colour of one LED; see the table below |
| Clock 1 | `0xc0` | `h24 << 15 \| hour << 8 \| minute` | `h24` `1` for 24 h, `0` for 12 h; `hour` `0`–`23`; `minute` `0`–`59` | Shown as written. 12 h shows an AM/PM indicator. The clock **does not tick**: 7 minutes later it still showed the time sent |
| Clock 2 offset | `0xc1` | `h24 << 15 \| negative << 10 \| minutes` | `negative` `1` for a negative offset; `minutes` the magnitude, `0`–`1023` (10 bits) | Offset relative to **clock 1**: with clock 1 at 20:46, `+60` showed 21:46 and `-120` showed 18:46. `h24` is per clock |
| Clock 3 offset | `0xc2` | as clock 2 | as clock 2 | as clock 2 |
| Date, first and second field | `0xc4` | `second << 8 \| first` | two-digit numbers `0`–`99` | Displayed as `first.second.third`, e.g. `28.10.26` |
| Date, third field | `0xc8` | `third` | `0`–`99` | See above. There is no format flag |
| MFD line 1 / 2 / 3, write | `0xd1` / `0xd2` / `0xd4` | `second_byte << 8 \| first_byte` | two bytes of the [character set](#mfd-character-set) per transfer | After a clear, consecutive writes fill the line from the left, two characters each, shown as written |
| MFD line clear | line command `\| 0x08` (`0xd9` / `0xda` / `0xdc`) | `0` |  | Blanks the line. The probe and GX52 0.7 always cleared a line before writing it, and so does the app |
| Shift indicator | `0xfd` | `0x51` on, `0x50` off |  | Lights the SHIFT label on the MFD |

Notes on the commands:

- **Brightness.** Both commands use the `0`–`128` wire scale. GX52 0.7 showed `0`–`32` and
  multiplied by 4; the app shows and stores the wire value (see [profiles.md](profiles.md)).
- **Clocks.** The app re-sends clock 1 every minute, on the minute, and whenever the time source
  changes (time zone, DST transition, suspend and resume), because the device keeps the last time it
  was sent. Clocks 2 and 3 are offsets from clock 1, not from UTC: to show a zone on clock 2 the app
  sends `zone offset − clock 1 offset` in minutes, recomputed whenever either side changes (a DST
  transition on either side changes it). The MFD clocks show only the time of day, so the app
  first normalises the difference into `−720`–`+720` minutes by adding or subtracting whole days
  (1440 minutes); the time shown is the same, and every normalised value fits the 10-bit field. For
  example clock 1 at UTC−5 and clock 2 at UTC+14 is `+1140`, sent as `−300`. S2 tested only
  differences that stay within the same day (`+60`, `−120`). On 2026-11-07, with clock 1 at 14:43,
  clock 2 at `+617` showed 01:00, past midnight forward, and clock 3 at `−719` showed 02:44 (the
  device-check `midnight`). A negative offset that crosses midnight backward needs clock 1 before
  noon and is still to be checked. The
  12 h / 24 h flag is independent per clock (clock 1 and 2 in 12 h with clock 3 in 24 h works).
- **Clock display.** The MFD shows one clock at a time, with the date below it by default.
  Start/Stop and Reset on the throttle cycle through clocks 1, 2 and 3 (a small `1` / `2` / `3`
  appears in the date area); after a few seconds the date comes back. This is the device's own
  behaviour; the app sends nothing for it.
- **Date.** The date format is chosen by which value goes into which field. GX52 0.7's formats:

  | Format | `0xc4` first (low byte) | `0xc4` second (high byte) | `0xc8` third |
  | --- | --- | --- | --- |
  | DD-MM-YY | day | month | year % 100 |
  | MM-DD-YY | month | day | year % 100 |
  | YY-MM-DD | year % 100 | month | day |

  S2 verified the field order (`first.second.third`) with DD-MM-YY; the other two rows are the same
  three fields in a different order.
- **MFD text.** A line holds 16 bytes after conversion through the character set. The app sends the
  clear command, then the bytes two per transfer; an odd last byte is padded with a space (`0x20`).
  Never raw UTF-8.
- **Shift and blink** take only the two values listed.

### LED bits

Bits for `0xb8` (GX52 0.7, all verified on the X52 Pro in S2). A bi-colour LED has a red and a green
bit; amber is both on, off is both off. A single-colour LED has one bit.

| LED | Location | Red bit | Green / on bit |
| --- | --- | --- | --- |
| Fire | stick, under the flip cover |  | `1` |
| A | stick | `2` | `3` |
| B | stick | `4` | `5` |
| D | throttle | `6` | `7` |
| E | throttle | `8` | `9` |
| T1/T2 | stick base, toggle switch 1 | `10` | `11` |
| T3/T4 | stick base, toggle switch 2 | `12` | `13` |
| T5/T6 | stick base, toggle switch 3 | `14` | `15` |
| POV 2 | stick head, top-left hat | `16` | `17` |
| i | throttle (clutch) | `18` | `19` |
| Throttle | throttle |  | `20` |

Every bi-colour LED shows red, green, amber and off as encoded (S2). To set a bi-colour LED the app
sends both bits; GX52 0.7 sent the green bit first, then the red bit.

**POV 1 LED: fixed green, not controllable.** The lower hat between B and C has a green LED that
stays green with every LED above switched off, follows the LED brightness (`0xb2`: dark at `0`,
back at `128`) and blinks with `0xb4`. It does not react to any `0xb8` bit (S2 scan, 2026-11-02).
The app shows it as a fixed, non-editable green LED with a short explanation (GitHub issue #3).

### Power-on state

As observed in S2, after plugging in: **the MFD backlight and the throttle LEDs are off, while the
stick LEDs are lit.** Both come on once the brightness commands (`0xb1`, `0xb2`) are sent. The
initial values of the device's registers (brightness, LED states, clocks, text) were not read and
are not inferred from this; the device has no read command the app uses.

Requirement: on **every connect** (start-up, hotplug, resume, permission granted) the app sends the
**full state**, brightness included, not only the values that changed since the last connect.

## MFD character set

The MFD shows one byte per character from this fixed 256-entry table. It was read from photos of
all 256 values on an X52 Pro (2026-10-28) and cross-checked against the glyph list in libx52's
`x52_char_map.cfg` for the ambiguous ones (`0x0E`, `0x0F`, `0x10`, `0x9B`–`0x9F`). The table is our
own transcription; libx52's file is GPL-2.0-only and is not copied. How it was read is in
[spike 02](spikes/02-mfd-charset.md).

Summary:

- `0x00`–`0x07` and `0xA0`: blank.
- `0x08`–`0x1F`: arrows, box corners, `·`, `®`, `©`, `™`, `†`, `§`, `¶` and Greek letters.
- `0x20`–`0x7F`: ASCII, except `0x5C` = `¥`, `0x7E` = `→`, `0x7F` = `←`.
- `0x80`–`0x9F`: Latin accented letters (`0x80`–`0x9A` as in code page 437), then `ñ Ñ ª º ¿`.
- `0xA1`–`0xDF`: JIS X 0201 half-width katakana (`U+FF61`–`U+FF9F`).
- `0xE0`–`0xFF`: more Latin letters, currency, fractions and math symbols.

|  | x0 | x1 | x2 | x3 | x4 | x5 | x6 | x7 | x8 | x9 | xA | xB | xC | xD | xE | xF |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| **0x** |  |  |  |  |  |  |  |  | ← | ┌ | ┐ | └ | ┘ | · | ® | © |
| **1x** | ™ | † | § | ¶ | Γ | Δ | Θ | Λ | Ξ | Π | Σ | Υ | Φ | Ψ | Ω | α |
| **2x** | SP | ! | " | # | $ | % | & | ' | ( | ) | * | + | , | - | . | / |
| **3x** | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | : | ; | < | = | > | ? |
| **4x** | @ | A | B | C | D | E | F | G | H | I | J | K | L | M | N | O |
| **5x** | P | Q | R | S | T | U | V | W | X | Y | Z | [ | ¥ | ] | ^ | _ |
| **6x** | `` ` `` | a | b | c | d | e | f | g | h | i | j | k | l | m | n | o |
| **7x** | p | q | r | s | t | u | v | w | x | y | z | { | \| | } | → | ← |
| **8x** | Ç | ü | é | â | ä | à | å | ç | ê | ë | è | ï | î | ì | Ä | Å |
| **9x** | É | æ | Æ | ô | ö | ò | û | ù | ÿ | Ö | Ü | ñ | Ñ | ª | º | ¿ |
| **Ax** |  | ｡ | ｢ | ｣ | ､ | ･ | ｦ | ｧ | ｨ | ｩ | ｪ | ｫ | ｬ | ｭ | ｮ | ｯ |
| **Bx** | ｰ | ｱ | ｲ | ｳ | ｴ | ｵ | ｶ | ｷ | ｸ | ｹ | ｺ | ｻ | ｼ | ｽ | ｾ | ｿ |
| **Cx** | ﾀ | ﾁ | ﾂ | ﾃ | ﾄ | ﾅ | ﾆ | ﾇ | ﾈ | ﾉ | ﾊ | ﾋ | ﾌ | ﾍ | ﾎ | ﾏ |
| **Dx** | ﾐ | ﾑ | ﾒ | ﾓ | ﾔ | ﾕ | ﾖ | ﾗ | ﾘ | ﾙ | ﾚ | ﾛ | ﾜ | ﾝ | ﾞ | ﾟ |
| **Ex** | á | í | ó | ú | ¢ | £ | ¥ | ₧ | ƒ | ¡ | Ã | ã | Õ | õ | Ø | ø |
| **Fx** | ˙ | ¨ | ° | `` ` `` | ´ | ½ | ¼ | × | ÷ | ≤ | ≥ | « | » | ≠ | √ | ¯ |

Empty cells are blank on the display. `SP` is the space character.

Rules for the app:

- Text is converted per character through a Unicode-to-byte table built from this one, in the `x52`
  crate.
- A character with no glyph gets a fallback: decomposition to the base letter where possible (for
  example `ő` to `o`), otherwise `?`.
- The 16-character limit is counted in bytes after conversion. The UI previews the converted text.
- Where a glyph appears twice (`¥` at `0x5C` and `0xE6`, `←` at `0x08` and `0x7F`, `` ` `` at `0x60`
  and `0xF3`), the conversion always picks the lower byte (`0x5C`, `0x08`, `0x60`); either shows the
  same glyph.

## evdev input map (X52 Pro)

Read through `/dev/input/event*` with the `evdev` crate, without grabbing, while `usbhid` stays
bound (S3). Device name "Logitech X52 Professional H.O.T.A.S.". Every control was pressed or moved
by hand, one at a time, on 2026-11-02.

### Buttons

39 key codes. The first 16 are on `BTN_JOYSTICK` (288–303), the rest on `BTN_TRIGGER_HAPPY1`...
(704–726), in HID report order (the same order as libx52's X52 Pro report parser).

| Code | Control | Code | Control |
| --- | --- | --- | --- |
| 288 | Trigger, first stage | 706 | Scroll wheel press (mouse secondary) |
| 289 | Fire (under the flip cover) | 707 | POV 2 up |
| 290 | A | 708 | POV 2 right |
| 291 | B | 709 | POV 2 down |
| 292 | C | 710 | POV 2 left |
| 293 | Pinkie switch | 711 | Throttle hat up |
| 294 | D | 712 | Throttle hat right |
| 295 | E | 713 | Throttle hat down |
| 296 | T1 (switch 1 up) | 714 | Throttle hat left |
| 297 | T2 (switch 1 down) | 715 | Mode 1 (held) |
| 298 | T3 (switch 2 up) | 716 | Mode 2 (held) |
| 299 | T4 (switch 2 down) | 717 | Mode 3 (held) |
| 300 | T5 (switch 3 up) | 718 | i (clutch) |
| 301 | T6 (switch 3 down) | 719 | Function (left MFD wheel press) |
| 302 | Trigger, second stage (288 stays held) | 720 | Start/Stop |
| 303 | Mouse button next to the ministick | 721 | Reset |
| 704 | Scroll wheel, one notch down | 722 | Left MFD wheel (Function) up: Page Up |
| 705 | Scroll wheel, one notch up | 723 | Left MFD wheel down: Page Down |
|  |  | 724 | Right MFD wheel (Select) up: Up |
|  |  | 725 | Right MFD wheel down: Down |
|  |  | 726 | Select (right MFD wheel press) |

The three toggle switches each have two positions (T1/T2, T3/T4, T5/T6), matching the three toggle
LEDs. The wheels (throttle scroll wheel and both MFD wheels) send a short press per notch, not a
held button. Clutch, the MFD buttons and the mode selector all come through evdev, so nothing in the
app needs USB reads.

### Axes

| evdev axis | Control | Range | Noise at rest |
| --- | --- | --- | --- |
| `ABS_X` / `ABS_Y` | Stick left-right / forward-back | 0–1023, centre ~511 | quiet |
| `ABS_RZ` | Stick twist | 0–1023 (reached 0–1010 on this unit) | quiet |
| `ABS_Z` | Throttle lever | 0–255 | ±1–2, ~40 events/s |
| `ABS_RX` | Rotary next to the i button | 0–255 | ±1–2, ~50 events/s |
| `ABS_RY` | Rotary next to the E button | 0–255 | ±1–2 |
| `ABS_THROTTLE` | Slider on the throttle | 0–255 | flickers between two adjacent values |
| `ABS_MISC` (40) / 41 | Ministick X / Y | 0–15, centre 8 | quiet; X grows to the right, Y downwards |
| `ABS_HAT0X` / `ABS_HAT0Y` | POV 1 | −1, 0, 1 | X −1 left, Y −1 up |

Because of the noise, nothing logs per input event, and a view of the axes shows values without
animating every ±1 change (or smooths them).

### Hats and names

POV 2 and the throttle hat are buttons, not hat axes. The names, used everywhere in the app (they
match GX52 0.7 and GitHub issue #3):

- **POV 1**: the lower hat between B and C; `ABS_HAT0X` / `ABS_HAT0Y`; LED fixed green.
- **POV 2**: the top-left hat on the stick head; buttons 707–710; LED bits 16/17.
- **Throttle hat**: buttons 711–714; no LED.

The MFD's "last button" line uses short forms that fit its 16 characters (`src/device/names.rs`):
"Trigger stage 1" and "Trigger stage 2", "Scroll up", "Scroll down" and "Scroll press" for the
scroll wheel, "Thr. hat up" (right, down, left) for the throttle hat.

### Mode selector

Each position is a held button (715 / 716 / 717). The current mode is in the key state at start-up
(`EVIOCGKEY`), so the app knows it without waiting for a change. Turning the selector bounces (S3
log): a position can pulse on and off several times within about 150 ms, there can be gaps of about
70 ms with no mode held, and going from 3 to 1 passes through 2.

Debounce rule: a mode becomes active only after **one** mode button has been held, alone, for
**200 ms**. Gaps with no mode held are ignored (the previous mode stays active), and a mode passed
through for less than 200 ms (2 on the way from 3 to 1) is never applied.

## Hotplug

One device-watch component with two sources (S4, verified on 2026-11-02):

- **On the host:** `nusb::watch_devices`. It reports every connect and disconnect, 0.3–0.6 s before
  polling would.
- **In the Flatpak sandbox:** polling `nusb::list_devices` and `evdev::enumerate` every **250 ms**.
  `nusb::watch_devices` is created without error in the sandbox but never yields, because the udev
  netlink multicast does not reach the sandbox's network namespace. Both lists are sysfs and `/dev`
  reads. The inotify fallback was not needed.
- **Lost events on the host:** the kernel drops udev netlink messages when the socket's buffer
  overflows (`ENOBUFS`, seen on resume from suspend on 2026-11-07); `nusb` logs it and carries on,
  so the host watch also re-lists every 5 s.

Devices are matched by **vendor and product ID** (later also serial or port path, for several
devices), **never by USB address**: the address changes on every replug (S4 saw `4`, `5`, `6`).
"Device appeared" is a trigger to enumerate, open and send the full state (see
[Power-on state](#power-on-state)).

## Virtual mouse (uinput)

The ministick drives a uinput pointer, as the Windows driver does in software (S9). No USB command
is involved. The joystick's evdev node is read **without grabbing**, so games still see every axis
and button.

uinput device "GX52 virtual mouse" with `BTN_LEFT`, `BTN_RIGHT`, `REL_X`, `REL_Y`, `REL_WHEEL`,
`REL_WHEEL_HI_RES`. The kernel registers it as a normal pointer (`mouse` and `event` handlers) for
libinput and the compositor, X11 or Wayland.

| Joystick input | Virtual mouse output |
| --- | --- |
| Ministick X (`ABS_MISC`) | `REL_X`, motion loop below; no inversion |
| Ministick Y (code 41) | `REL_Y`, motion loop below; no inversion |
| Key 303 (mouse button) press / release | `BTN_LEFT` 1 / 0 |
| Key 706 (scroll wheel press) press / release | `BTN_RIGHT` 1 / 0 |
| Key 705 (wheel notch up), press only | `REL_WHEEL` +1 and `REL_WHEEL_HI_RES` +120 |
| Key 704 (wheel notch down), press only | `REL_WHEEL` −1 and `REL_WHEEL_HI_RES` −120 |

Every batch of output events ends with `SYN_REPORT`.

Motion loop and tuning values (the 1.0.0 values, judged "already very good" on the device and in a
game):

| Value | Setting |
| --- | --- |
| Tick | 10 ms (100 Hz), independent of how often the device reports |
| Deadzone | 1 step: an offset of ±1 from the centre (8) is treated as centred |
| Gain | 0.6 px per tick |
| Curve | 1.6: speed per axis = `sign(offset) × gain × (abs(offset) − deadzone) ^ curve` px per tick |
| Sub-pixel | fractions carry over between ticks, so small offsets still move; dropped when the axis returns to the centre, so the pointer stops dead |

Release rule: when the joystick goes away (the read fails with `ENODEV`) or the feature is disabled,
the app sends `BTN_LEFT` and `BTN_RIGHT` release events followed by `SYN_REPORT`, whether or not it
keeps the uinput device alive. Destroying the uinput device also releases them (S9 unplug test), but
the app does not rely on that.

## X52 (non-Pro)

Not tested: the maintainer owns only an X52 Pro, and the X52 is supported "best effort". What the
app assumes, all unverified until a test on the device is recorded here:

- **IDs** `06a3:0255` and `06a3:075c` (GX52 0.7, libx52).
- **USB commands:** both brightness values, MFD text, clocks and date work as on the X52 Pro. GX52
  0.7 sent exactly these to the non-Pro (its `x52_profile` has brightness, clock and date fields but
  no LEDs). The individual LEDs (`0xb8`) are not controllable on the non-Pro, so the app never sends
  `0xb8` to it and hides the LED editor, as GX52 0.7 did.
- **Shift and blink** (`0xfd`, `0xb4`) were verified only on the X52 Pro (S2); GX52 0.7 never sent
  either to any device. They are not sent to the non-Pro until verified on one.
- **Character set:** assumed the same as the X52 Pro.
- **evdev map:** the candidate is GX52 0.7's non-Pro table (`X52EvdevKeyMapping` in
  `gx52/driver/x52_driver.py`): the first 15 buttons as on the Pro (288–302), then POV 2 303 and
  704–706, throttle hat 707–710, **mode 1/2/3 at 711/712/713**, MFD Function / Start/Stop / Reset at
  714 / 715 / 716, i at 717, mouse button 718, scroll wheel press / down / up at 719 / 720 / 721. It
  is to be compared with the non-Pro report order in [libx52](https://github.com/nirenjan/libx52)
  (which differs from the Pro's). Neither has been checked against a device, and the ministick axis
  codes on the non-Pro are not known.
- **Mode selector and virtual mouse** on the non-Pro use that candidate map (the per-type mode
  mapping exists for `x52` too). Confirming the map on a device, with an input tester, is an open
  item owned by the roadmap (see [roadmap.md](roadmap.md): the input tester verifies the map,
  and the `1.0.0` gate records the outcome); until then the non-Pro is documented as unverified.

## Commands the app may send

The complete list. Anything not in it is never sent, to any device.

| `wIndex` | Command | Allowed `wValue` | Device types |
| --- | --- | --- | --- |
| `0xb1` | MFD brightness | `0`–`128` | `x52-pro`, `x52` |
| `0xb2` | LED brightness | `0`–`128` | `x52-pro`, `x52` |
| `0xb4` | Blink | `0x50`, `0x51` | `x52-pro` |
| `0xb8` | LED on/off | `bit << 8 \| state`, `bit` in `1`–`20`, `state` in `0`, `1` | `x52-pro` |
| `0xc0` | Clock 1 | `h24` `0`/`1`, `hour` `0`–`23`, `minute` `0`–`59` | `x52-pro`, `x52` |
| `0xc1`, `0xc2` | Clock 2 / 3 offset | `h24` `0`/`1`, `negative` `0`/`1`, `minutes` `0`–`720` (normalised, see the clock notes) | `x52-pro`, `x52` |
| `0xc4` | Date, first and second field | each byte `0`–`99` | `x52-pro`, `x52` |
| `0xc8` | Date, third field | `0`–`99` | `x52-pro`, `x52` |
| `0xd1`, `0xd2`, `0xd4` | MFD line write | two character-set bytes | `x52-pro`, `x52` |
| `0xd9`, `0xda`, `0xdc` | MFD line clear | `0` | `x52-pro`, `x52` |
| `0xfd` | Shift indicator | `0x50`, `0x51` | `x52-pro` |

The `x52` crate encodes these as typed commands (an enum, not raw numbers), so a value outside the
allowed range cannot be built; the transport only ever sees encoded commands.

**Not part of the protocol, never sent by the app.** The S2 and hands-on experiments sent values
outside this list to find out what the device does:

- the `0xb8` scan with LED bits `0` and `21`–`255`, state on and off (accepted without error, no
  visible effect, done to look for a POV 1 control);
- `wValue` `255` on `0xb2`;
- raw bytes through the probe's `raw` command, and any other `wIndex` value.

These were one-off tests on the maintainer's device. They are not commands: unknown commands could
change persistent device settings, which is why no other command number was scanned. The app must
never send them, and a new command is added to this list only after it has been verified on a device
and recorded here.
