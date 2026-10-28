# S1/S2: USB control and protocol verification (X52 Pro)

Hardware: X52 Pro `06a3:0762` ("X52 Professional H.O.T.A.S."), firmware as shipped, on Ubuntu
(kernel 7.0), bus 1. Tool: `spikes/x52-probe` on the `spikes` branch (`nusb` 0.2). Results are
observations on this one device.

## S1: control transfers without detaching the HID driver

- The single interface `1-11:1.0` is bound to `usbhid` and stays bound before, during and after the
  transfers (checked through sysfs).
- `nusb::Device::control_out` with `ControlType::Vendor`, `Recipient::Device`, `bRequest = 0x91`,
  no data stage, works without claiming the interface. No `rusb` / libusb fallback needed.
- Access: without a rule the USB node is `root:root 0664` and `open` fails with `EACCES` (errno 13).
  With `/etc/udev/rules.d/70-gx52.rules`:
  `SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="06a3", ATTR{idProduct}=="0762|0255|075c", TAG+="uaccess"`
  the logged-in user gets an ACL entry (`user:<name>:rw-`) and everything works. This is the rule the
  in-app guidance will show.
- The evdev node `/dev/input/event*` is readable without any rule (systemd joystick `uaccess`).

## S2: commands verified on the device

| Command | Result |
|---|---|
| LED on/off and colour (`0xb8`, `bit << 8 \| state`) | All stick LEDs (Fire, A, B, POV 2, T1/T2, T3/T4, T5/T6) and throttle LEDs (D, E, i, Throttle) follow red / green / amber / off as encoded in GX52 0.7. |
| LED brightness (`0xb2`) | One value for every LED, stick and throttle. `0` = off, `128` = maximum; `255` looks the same as `128`. |
| MFD brightness (`0xb1`) | `0` = backlight off, `16` = very dim, `128` = full. |
| MFD text (`0xd1`/`0xd2`/`0xd4`, clear with `\| 0x08`) | Lines 1 to 3 show text as written, one byte per character, 16 per line. Character set in [02-mfd-charset.md](02-mfd-charset.md). |
| Clock 1 (`0xc0`, `24h << 15 \| hour << 8 \| minute`) | Shown as written. 12h mode shows an AM/PM indicator next to the time. The clock does **not** advance by itself: 7 minutes later it still showed the time that was sent, so the app must re-send it every minute. |
| Clock 2 / 3 offset (`0xc1` / `0xc2`, `24h << 15 \| negative << 10 \| minutes`) | Offsets are relative to **clock 1**: with clock 1 at 20:46, `+60` showed 21:46 and `-120` showed 18:46. The 12h/24h flag is per clock (clock 1 and 2 in 12h with clock 3 in 24h works). |
| Date (`0xc4` = `high << 8 \| low`, `0xc8` = third field) | Displayed as `low.high.third`, e.g. `28.10.26`. There is no format flag: the date format is chosen by which value the app puts in which field. |
| Shift indicator (`0xfd`, `0x51` / `0x50`) | Lights the SHIFT label on the MFD. |
| Blink (`0xb4`, `0x51` / `0x50`) | Makes the **i** button LED (throttle) and the **POV 2** LED (stick) blink. |

## Using the MFD clocks

The MFD shows one clock at a time, with the date below it by default. The Start/Stop and Reset
buttons on the throttle cycle through clocks 1, 2 and 3 (a small `1` / `2` / `3` appears in the date
area); after a few seconds it goes back to the date.

## Power-on state

After plugging in, the MFD backlight and the throttle LEDs are off; the stick LEDs are lit. Both come
on only after the brightness commands are sent (GX52 0.7 turned them on when it applied a profile). Consequence for the app: on every
connect (start-up, hotplug, resume) the full state must be sent, brightness included, not only the
values that changed.
