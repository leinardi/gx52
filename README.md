<h1 align="center">
  <img alt="GX52 icon" src="data/icons/com.leinardi.gx52.svg" width="96">
  <br>GX52
</h1>

<p align="center"><strong>Configure the LEDs, the display and the profiles of the Logitech (Saitek)
X52 and X52 Pro joysticks on Linux.</strong></p>

<p align="center">
  <a href="https://github.com/leinardi/gx52/actions/workflows/ci.yaml"><img alt="CI" src="https://img.shields.io/github/actions/workflow/status/leinardi/gx52/ci.yaml?style=flat-square"></a>
  <a href="https://flathub.org/apps/com.leinardi.gx52"><img alt="Flathub" src="https://img.shields.io/flathub/v/com.leinardi.gx52?style=flat-square"></a>
  <a href="https://github.com/leinardi/gx52/actions/workflows/docs.yaml"><img alt="Docs" src="https://img.shields.io/github/actions/workflow/status/leinardi/gx52/docs.yaml?label=docs&style=flat-square"></a>
  <a href="COPYING.txt"><img alt="License" src="https://img.shields.io/github/license/leinardi/gx52?style=flat-square"></a>
</p>

<p align="center">
  <a href="https://leinardi.github.io/gx52/">Documentation</a> ·
  <a href="#-installation">Install</a> ·
  <a href="CONTRIBUTING.md">Contributing</a>
</p>

<p align="center">
  <img alt="The LEDs of a per-mode profile" src="data/screenshots/leds.png" width="800">
</p>

GX52 is a desktop app for setting up your X52 on Linux. Pick the LED colours and brightness, put
your own text and three time zones on the display, and save it all as profiles that can follow the
joystick's mode selector. Changes reach the joystick as you make them, and your games keep working
while GX52 runs.

## ✨ Features

- **LEDs and display:** LED colours and blinking, the MFD text and three clocks in real time zones.
- **Profiles:** one set of settings or one per mode, with custom control names and TOML files.
- **Input tester and virtual mouse:** every axis and button live, and a ministick pointer.
- **Runs in the background:** a tray icon, starting on login, with guidance for the udev rule.

## 📦 Installation

```bash
flatpak install flathub com.leinardi.gx52
flatpak run com.leinardi.gx52
```

<a href="https://flathub.org/apps/com.leinardi.gx52"><img width="200" alt="Get it on Flathub" src="https://flathub.org/api/badge?locale=en"></a>

Flathub is the only official channel; new versions reach its beta channel first. The beta
remote and every other install detail are in [Getting started][start], and building GX52
yourself is described below.

## 🚀 Usage

GX52 needs a udev rule before it can set anything. The first time it cannot open the joystick it
shows the rule, the file to put it in (`/etc/udev/rules.d/70-gx52.rules`) and the commands that
reload udev; the virtual mouse's page in Preferences does the same for `/dev/uinput`. GX52 never
asks for root or installs the rule itself.

From the command line, `--hidden` starts it in the background without a window, the way the
login autostart does, and `RUST_LOG` raises the log level:

```bash
flatpak run --env=RUST_LOG=debug com.leinardi.gx52 --hidden
```

Where the settings and profiles are stored, and how to bring the
profiles of a GX52 0.7 source install into the Flatpak, are in [Getting started][start].

## 🛠️ Building from source

Meson and Cargo, with GTK 4, libadwaita and blueprint-compiler at the versions of the GNOME 51
runtime:

```bash
meson setup _build
meson compile -C _build
meson test -C _build
```

The packages to install first are in [Building from source][build]. The development build
(`com.leinardi.gx52.Devel`, installable next to the stable app) and its Flatpak are in
[CONTRIBUTING.md](CONTRIBUTING.md).

## 🤝 Contributing

Bug reports and feature requests go to the [issue tracker](https://github.com/leinardi/gx52/issues).
GX52 is developed on an X52 Pro, so test reports from plain X52 owners are especially welcome.
[CONTRIBUTING.md](CONTRIBUTING.md)
covers building, checks, branches, commits and translations (`po/`);
[SECURITY.md](SECURITY.md) explains how to report a vulnerability privately.

## 📝 License

GX52 is free software under the [GNU General Public License, version 3 or
later](COPYING.txt). Copyright (c) 2019 Roberto Leinardi.

[start]: https://leinardi.github.io/gx52/getting-started/
[build]: https://leinardi.github.io/gx52/building/
