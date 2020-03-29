# GX52
GX52 is a GTK application designed to provide control for the LEDs and MFD of Logitech X52 and X52 Pro H.O.T.A.S.

## 💡 Features
<img src="/data/icons/hicolor/48x48@2x/apps/com.leinardi.gx52.png" width="96" align="right" hspace="0" />

* Set LEDs color
* Set LEDs status (on/off)
* Set LEDs brightness
* Set MFD brightness
* Update MFD date and time
* Show on MFD which button is currently pressed 
* Save and restore multiple profiles

<img src="/art/screenshot-1.png" width="800" align="middle"/>
<img src="/art/screenshot-2.png" width="800" align="middle"/>


## 📦 How to get GX52
### Install from Flathub
This is the preferred way to get GX52 on any major distribution (Arch, Fedora, Linux Mint, openSUSE, Ubuntu, etc).

If you don't have Flatpak installed you can find step by step instructions [here](https://flatpak.org/setup/).

Make sure to have the Flathub remote added to the current user:

```bash
flatpak --user remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
```

#### Install
```bash
flatpak --user install flathub com.leinardi.gx52
```

#### Run
```bash
flatpak run com.leinardi.gx52
```

### Install from source code
#### Build time dependencies
| Dependency            | Arch Linux            | Fedora                      | Ubuntu                 |
| --------------------- | --------------------- | --------------------------- | ---------------------- |
| pkg-config            | pkg-config            | pkgconf-pkg-config          | pkg-config             |
| Python 3.6+           | python                | python3                     | python3                |
| gobject-introspection | gobject-introspection | gobject-introspection-devel | libgirepository1.0-dev |
| meson                 | meson                 | meson                       | meson                  |
| ninja-build           | ninja                 | ninja-build                 | ninja-build            |
| appstream-util        | appstream-glib        | appstream-util              | appstream-util         |
| libusb-1.0-0          | libusb                | libusbx-devel               | libusb-1.0-0-dev       |
| libudev               | libudev0              | libudev-devel               | libudev-dev            |

#### Run time dependencies
| Dependency                         | Arch Linux                         | Fedora                             | Ubuntu                             |
| ---------------------------------- | ---------------------------------- | -----------------------------------| ---------------------------------- |
| Python 3.6+                        | python                             | python3                            | python3                            |
| pip                                | python-pip                         | python3-pip                        | python3-pip                        |
| gobject-introspection              | gobject-introspection              | gobject-introspection-devel        | libgirepository1.0-dev             |
| libappindicator                    | libappindicator3                   | libappindicator-gtk3               | gir1.2-appindicator3-0.1           |
| gnome-shell-extension-appindicator | gnome-shell-extension-appindicator | gnome-shell-extension-appindicator | gnome-shell-extension-appindicator |

plus all the Python dependencies listed in [requirements.txt](requirements.txt)

#### Clone project and install
If you have not installed GX52 yet:
```bash
git clone --recurse-submodules -j4 https://gitlab.com/leinardi/gx52.git
cd gx52
git checkout release
sudo -H pip3 install -r requirements.txt
meson . build --prefix /usr
ninja -v -C build
sudo ninja -v -C build install
```

#### Update old installation
If you installed GX52 from source code previously and you want to update it:
```bash
cd gx52
git fetch
git checkout release
git reset --hard origin/release
git submodule init
git submodule update
sudo -H pip3 install -r requirements.txt
meson . build --prefix /usr
ninja -v -C build
sudo ninja -v -C build install
```

#### Run
Once installed, to start it you can simply execute on a terminal:
```bash
gx52
```

## Running the app
To start the app you have to run the command `gx52` in a terminal. The app needs to access the USB interface of the Kranen that, normally,
is not available to unprivileged users. 

To allow normal users to access the Kraken's USB interface you can 
create a custom udev rule

### Adding Udev rule
#### Using GX52
Simply run:
```bash
gx52 --add-udev-rule
```
It will automatically refresh also the udev rules.

#### Manually
Create a new file in `/lib/udev/rules.d/60-gx52.rules` containing this text:
```bash
SUBSYSTEMS=="usb", ATTRS{idVendor}=="06a3", ATTRS{idProduct}=="0762", MODE="0666"
SUBSYSTEMS=="usb", ATTRS{idVendor}=="06a3", ATTRS{idProduct}=="0255", MODE="0666"
SUBSYSTEMS=="usb", ATTRS{idVendor}=="06a3", ATTRS{idProduct}=="075c", MODE="0666"
```

After that, run the following commands
```bash
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=usb --attr-match=idVendor=06a3 --action=add
```

## Command line options

  | Parameter                 | Description                                                 | Source | Flatpak |
  |---------------------------|-------------------------------------------------------------|:------:|:-------:|
  |-v, --version              |Show the app version                                         |    x   |    x    |
  |--debug                    |Show debug messages                                          |    x   |    x    |
  |--hide-window              |Start with the main window hidden                            |    x   |    x    |
  |--add-udev-rule            |Add udev rule to allow execution without root permission     |    x   |    x    |
  |--remove-udev-rule         |Remove udev rule that allow execution without root permission|    x   |    x    |
  |--autostart-on             |Enable automatic start of the app on login                   |    x   |         |
  |--autostart-off            |Disable automatic start of the app on login                  |    x   |         |

## 🖥️ Build, install and run with Flatpak
If you don't have Flatpak installed you can find step by step instructions [here](https://flatpak.org/setup/).

Make sure to have the Flathub remote added to the current user:

```bash
flatpak --user remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
```

### Clone the repo
```bash
git clone --recurse-submodules -j4 https://gitlab.com/leinardi/gx52.git
```
It is possible to build the local source or the remote one (the same that Flathub uses)
### Local repository
```bash
./build.sh --flatpak-local --flatpak-install
```
### Remote repository
```bash
./build.sh --flatpak-remote --flatpak-install
```
### Run
```bash
flatpak run com.leinardi.gx52 --debug
```

## ❓ FAQ
### The Flatpak version of GX52 is not using my theme, how can I fix it?
To fix this issue install a Gtk theme from Flathub. This way, Flatpak applications will automatically pick the 
installed Gtk theme and use that instead of Adwaita.

Use this command to get a list of all the available Gtk themes on Flathub:
```bash
flatpak --user remote-ls flathub | grep org.gtk.Gtk3theme
```
And then just install your preferred theme. For example, to install Yaru:
```
flatpak install flathub org.gtk.Gtk3theme.Yaru
``````

### Where are the settings and profiles stored on the filesystem?
| Installation type |                     Location                     |
|-------------------|:------------------------------------------------:|
| Flatpak           |        `$HOME/.var/app/com.leinardi.gx52/`        |
| Source code       | `$XDG_CONFIG_HOME` (usually `$HOME/.config/gx52`) |

## 💚 How to help the project
### Discord server
If you want to help testing or developing it would be easier to get in touch using the discord server of the project: https://discord.gg/HTCrmU8  
Just write a message on the general channel saying how you want to help (test, dev, etc) and quoting @leinardi. If you don't use discor but still want to help just open a new issue here.

### Can I support this project some other way?

Something simple that everyone can do is to star it on both [GitLab](https://gitlab.com/leinardi/gx52) and [GitHub](https://github.com/leinardi/gx52).
Feedback is always welcome: if you found a bug or would like to suggest a feature,
feel free to open an issue on the [issue tracker](https://gitlab.com/leinardi/gx52/issues).

## License
```
This file is part of gx52.

Copyright (c) 2019 Roberto Leinardi

gx52 is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

gx52 is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with gx52.  If not, see <http://www.gnu.org/licenses/>.
```