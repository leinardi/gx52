# Call for testers: GX52 0.99 on the plain Saitek/Logitech X52

GX52 has been rewritten in Rust with GTK 4 and libadwaita, and the first development build, 0.99.0,
is on the Flathub beta channel. I develop it on an X52 Pro and don't own a plain X52 (USB IDs
`06a3:0255` or `06a3:075c`), so everything GX52 does with the non-Pro comes from documentation and
from GX52 0.7, not from a device. If you have a plain X52, half an hour of testing would help a lot
before 1.0.0 replaces 0.7.6 on Flathub stable.

## Install

```bash
flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo
flatpak install flathub-beta com.leinardi.gx52
flatpak run com.leinardi.gx52
```

To go back to the stable version afterwards, run `flatpak uninstall com.leinardi.gx52//beta` and,
if it is not installed any more, `flatpak install flathub com.leinardi.gx52`. If GX52 can't open the
joystick, it shows the udev rule to install; please follow it and mention it in the report if the
instructions were unclear.

Please start it from a terminal with `RUST_LOG=debug flatpak run com.leinardi.gx52` and keep the
output: it is the most useful part of a report.

## What to test

1. **Detection.** Does GX52 recognise the joystick as an X52 (not an X52 Pro)? Please include your
   `lsusb | grep 06a3` line.
2. **Input tester** (the input tester page). This is the part I'm least sure of. Press each control
   and write down what the tester shows:
   - every button and the two trigger stages;
   - all three hats in all four directions: POV 1 (the lower hat on the stick), POV 2 (the upper
     hat on the stick) and the throttle hat ("Thr. hat" in the tester);
   - the mode selector in positions 1, 2 and 3;
   - the MFD buttons: Function, Start/Stop and Reset;
   - the i button on the throttle ("i (clutch)" in the tester);
   - the ministick, the mouse button and the scroll wheel (press, up and down);
   - every axis, including the rotaries and the slider.

   Anything shown under the wrong name, not shown at all, or shown twice is what I'm looking for.
3. **Mode selector and profiles.** With one profile per mode, does switching the mode selector
   switch the profile on the MFD?
4. **MFD.** Set text on all three lines; set both brightness values. Does the display show exactly
   what the app's preview shows?
5. **Character set.** Type accented letters and symbols on an MFD line (for example
   `àéîöü ß ñ € £ ° ±`) and compare the display with the preview. A photo of the MFD next to a
   screenshot of the preview is ideal.
6. **Clocks and date.** Set clock 1 to the local time, clocks 2 and 3 to other time zones, in 12-
   and 24-hour format. Are they right on the MFD?
7. **Virtual mouse** (Preferences). Turn it on: does the ministick move the pointer, and do the
   mouse button, the wheel press and the wheel work?

The LEDs, the LED blinking and the SHIFT indicator are hidden on the plain X52 on purpose (the
individual LEDs can't be set, and the other two are not verified on one), so they are not part of
this test.

## Reporting

Please reply here with:

- your distribution and desktop (GNOME, KDE, ...) and Wayland or X11;
- the `lsusb` line;
- for each numbered item above: works, doesn't work, or not tested, with what you saw;
- the terminal output from `RUST_LOG=debug`, and photos of the MFD if the text or the characters
  look wrong.

Thanks! The results are recorded in the GX52 roadmap (milestone M8) and in the X52 (non-Pro)
section of the protocol documentation, and the plain X52 is listed as "tested by a user" or
"unverified" in the 1.0.0 release notes accordingly.
