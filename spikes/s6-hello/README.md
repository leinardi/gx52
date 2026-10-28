# GX52

Control the LEDs and MFD of the Logitech X52 and X52 Pro.

Built with Rust, GTK 4 and libadwaita ([gtk-rs](https://gtk-rs.org)).

## Building

### GNOME Builder / Flatpak (recommended)

Open the project in GNOME Builder and press Run, or:

```sh
flatpak-builder --user --install --force-clean _flatpak build-aux/com.leinardi.gx52.Devel.json
flatpak run com.leinardi.gx52.Devel
```

### Meson

Requires Rust >= 1.92, GTK >= 4.22, libadwaita >= 1.9 and blueprint-compiler.

```sh
meson setup _build -Dprofile=development
meson compile -C _build
meson devenv -C _build ./src/gx52   # run uninstalled
meson test -C _build                      # cargo test + desktop/metainfo/schema validation
ninja -C _build cargo-clippy
```

`cargo check` and `cargo clippy` also work without Meson (build constants fall back to Devel defaults),
but running the app needs the GResource bundle and the compiled GSettings schema that Meson produces.
