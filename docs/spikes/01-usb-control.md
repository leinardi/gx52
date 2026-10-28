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
| MFD text (`0xd1`/`0xd2`/`0xd4`, clear with `\| 0x08`) | Lines 1 to 3 show ASCII text as written. |

## Power-on state

After plugging in, the MFD backlight and the throttle LEDs are off; the stick LEDs are lit. Both come
on only after the brightness commands are sent (the reporter of the "throttle LEDs off" observation
remembered GX52 0.7 turning them on when applying a profile). Consequence for the app: on every
connect (start-up, hotplug, resume) the full state must be sent, brightness included, not only the
values that changed.
