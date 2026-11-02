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
| Hotplug events | `nusb::watch_devices` gets **nothing** in the sandbox (no events, no error); polling works, see below |

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

## Hotplug: verified 2026-11-02

`spikes/x52-hotplug` prints three signals with timestamps: `nusb::watch_devices` events, and
changes seen by polling `nusb::list_devices` and `evdev::enumerate` every 250 ms. It ran on the host
and in the GNOME 51 sandbox (`--device=all`, no network) at the same time while the joystick was
plugged in and out three times.

| Signal | Host | Sandbox, no network |
|---|---|---|
| `nusb::watch_devices` | every connect and disconnect, 0.3–0.6 s before polling notices | silent: the stream is created without error but never yields |
| Polling the USB device list (250 ms) | OK | OK |
| Polling the evdev device list (250 ms) | OK, same poll as USB or one later | OK |

This confirms the netlink analysis above. Decision for the app: one device-watch component with two
sources, `nusb::watch_devices` when not sandboxed and polling (250 ms to 500 ms, both lists are
sysfs/dev reads) in the sandbox. The inotify fallback was not needed; polling is simpler and proven.
The USB address changes on every replug (`addr 4`, `5`, `6`), so devices are matched by vendor and
product ID (and later serial or port path for several devices), never by address.

## Without the udev rule: verified 2026-11-02

Rule moved away, `udevadm control --reload && udevadm trigger`, joystick replugged.

| Check | Host | Sandbox |
|---|---|---|
| Device listed by `nusb::list_devices` (sysfs), name and IDs readable | yes | yes |
| `/dev/bus/usb/001/007` | `root:root 0664`, no user ACL | same |
| `DeviceInfo::open` | `permission denied (errno 13)` | same |
| evdev node (`/dev/input/event*`) | still readable (systemd's joystick `uaccess`) | same |

So the app can tell the three states apart without guessing: **no device** (not listed), **device
without permission** (listed, `open` fails with `EACCES` / `io::ErrorKind::PermissionDenied`) and
**ready**. The second state shows the udev guidance page. In that state input still works (input
tester, virtual mouse if uinput is allowed), only LEDs, MFD and clocks are unavailable, so the page
should say exactly that rather than "device not found".

After restoring the rule with `udevadm control --reload && udevadm trigger` and **without
replugging**, the user ACL came back on the existing node and `open` worked at once. So the guidance
page can say "no need to unplug" and the app only has to retry `open` while in the "no permission"
state (on each device poll, or when the user presses "Try again").

## Not done yet

- Running the real app build (Flatpak bundle from S6 with the device code) instead of the spike
  binaries.
