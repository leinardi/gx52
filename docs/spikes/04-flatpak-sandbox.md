# S4: device access inside the Flatpak sandbox

Run on 2026-10-28 with the spike binaries (`x52-probe`, `x52-input`) executed inside the GNOME 51
runtime the way the app will run on Flathub:
`flatpak run --user --device=all --unshare=network --filesystem=<spikes>:ro --command=<binary> org.gnome.Platform//51`.
The host has the `70-gx52.rules` udev rule from S1.

## Results

| Check | Result |
|---|---|
| Open the USB device and send a vendor control transfer (`nusb`) | OK; `usbhid` stays bound |
| Read sysfs (interface drivers, device attributes) | OK |
| Open the evdev node, read capabilities and key state (`evdev`) | OK: 39 key codes, axes, mode 1 held |
| `/dev/bus/usb/...` permissions | the user's `uaccess` ACL works (owner shows as `nfsnobody` through the user namespace) |
| `/run/udev` (libudev database) | not present, as expected: libudev-based enumeration or monitoring cannot be used |
| Hotplug events | **not verified yet** (needs an unplug / replug), see below |

So `--device=all` plus the host udev rule is enough for everything the app does at start-up. No
extra permission is needed.

## Hotplug: probable problem and fallbacks

`nusb::watch_devices` (0.2.9, `linux_usbfs/hotplug.rs`) listens on a `NETLINK_KOBJECT_UEVENT` socket
bound to the **udev multicast group** (`1 << 1`), i.e. messages re-broadcast by `udevd` after it
processed an event. Netlink multicast is per network namespace. A Flatpak without
`--share=network` runs in its own network namespace, so it most likely never receives these
messages; with `--share=network` it shares the host namespace and should.

To verify on a hands-on day: run a small watcher (`nusb::watch_devices` and a `gio::FileMonitor` on
`/dev/input` and `/dev/bus/usb/<bus>`) inside the sandbox with and without `--share=network`, and
unplug / replug the joystick.

Fallbacks, in order of preference:

1. **Watch device nodes with inotify** (`gio::FileMonitor` on `/dev/bus/usb/*` and `/dev/input`):
   `--device=all` exposes the host's devtmpfs, so node creation and removal should be visible
   without network access. Then enumerate with `nusb::list_devices()` / `evdev::enumerate()`.
2. **`--share=network`**: makes `nusb`'s udev netlink path work, at the cost of a broader permission
   (the X-Plane adapter in 1.x would need it anyway).
3. **Periodic rescan** (every few seconds) as a last resort.

Whatever is chosen, the device service treats "device appeared" as a trigger to enumerate and send
the full state (see the power-on findings in S2), so the event source is replaceable.

## Not done yet

- Hotplug verification (above).
- Running the real app build (Flatpak bundle from S6 with the device code) instead of the spike
  binaries.
