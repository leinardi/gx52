# Getting started

Install GX52 and give it access to the joystick, that's it. Building it
yourself is described in [Building from source](building.md).

## Install

### Flathub

GX52 is on [Flathub](https://flathub.org/apps/com.leinardi.gx52). With
[Flatpak set up](https://flatpak.org/setup/):

```bash
flatpak install flathub com.leinardi.gx52
flatpak run com.leinardi.gx52
```

Flathub is the only official channel: there are no `.deb` or Snap packages. Distribution packagers
are welcome; the build is plain Meson and Cargo (see [Building from source](building.md)).

### Beta

New versions are published on the Flathub beta channel first, for testing:

```bash
flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo
flatpak install flathub-beta com.leinardi.gx52
```

## Supported devices

The X52 Pro is the device GX52 is developed and tested with. The plain X52 is supported on a best
effort basis and has not been tested on a device: its individual LEDs cannot be set, GX52 leaves
out the LED blinking and the SHIFT indicator until they are verified on one, and its input codes
come from the documentation, not from a device on the desk. Reports from X52 owners are welcome.

## Features

- LED colours and on/off states per LED, and the blinking of the i and POV 1 LEDs (X52 Pro), on a
  schematic of the stick, its base and the throttle, plus the LED and MFD brightness.
- MFD text per line, the SHIFT indicator (X52 Pro), and the three clocks with real time zones
  (daylight saving included) instead of minute offsets.
- Custom names for the controls, per mode, shown by the MFD's "Last button" line ("Emerg. Lights"
  for E).
- Profiles: a single set of settings, or one per position of the mode selector (1/2/3). Import and
  export as TOML files; the profiles of GX52 0.7 are imported on first start.
- A tray icon, running in the background and starting on login.
- An input tester: a live view of the axes, buttons, hats, toggles and the mode selector.
- A virtual mouse: the throttle's ministick moves the pointer, with left click, right click (press
  the wheel) and the wheel, with speed, curve, deadzone and wheel direction settings.
- Guidance in the app for the udev rule the joystick (and the virtual mouse) needs.

## Device access (udev)

Without a udev rule the joystick's USB node belongs to root and GX52 cannot set anything. When it
cannot open the device, GX52 shows a page with the rule, where to put it and the commands to reload
udev; the virtual mouse's page in Preferences does the same for `/dev/uinput`. The rule uses
`TAG+="uaccess"`, so only the user of the active seat gets access, and GX52 never asks for root or
installs anything itself. The rule, line by line, is in
[protocol.md](protocol.md#udev-rule).

If libx52's `x52d` daemon is running, it writes the same LEDs and MFD as GX52 does; GX52 shows a
warning while it sees it.

## Running

`gx52 --hidden` starts in the background, without a window (the login autostart uses it). Logs go to
the terminal; set `RUST_LOG=debug` for more.

The GX52 0.7 command line options are gone: `--add-udev-rule` and `--remove-udev-rule` (GX52 no
longer writes host files; the guidance page shows the rule instead), `--autostart-on` and
`--autostart-off` (the switch in Preferences, through the Background portal), `--hide-window`
(now `--hidden`), and `--debug` and `--version` (set `RUST_LOG`; the About dialog shows the version).

## Where settings and profiles are stored

| Installation | Location |
| --- | --- |
| Flatpak | `~/.var/app/com.leinardi.gx52/config/gx52/` |
| From source | `$XDG_CONFIG_HOME/gx52/` (usually `~/.config/gx52/`) |

Profiles are TOML files in its `profiles/` directory; the preferences are GSettings keys.

**Moving from a source install of GX52 0.7 to the Flatpak:** the old profiles are in
`~/.config/gx52/gx52.db`, which the sandbox cannot read. Copy the file before the first start, and
GX52 imports it:

```bash
mkdir -p ~/.var/app/com.leinardi.gx52/config/gx52
cp ~/.config/gx52/gx52.db ~/.var/app/com.leinardi.gx52/config/gx52/gx52.db
```
