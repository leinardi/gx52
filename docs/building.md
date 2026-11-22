# Building from source

On Ubuntu 26.04 (other distributions: the same libraries at the versions the GNOME 51 runtime
ships):

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev libsqlite3-dev blueprint-compiler meson \
    desktop-file-utils appstream
git clone https://github.com/leinardi/gx52.git
cd gx52
meson setup _build
meson compile -C _build
meson test -C _build
sudo meson install -C _build
```

Rust comes from `rustup`; `rust-toolchain.toml` pins the release. The development build
(`com.leinardi.gx52.Devel`, installable next to the stable app) and its Flatpak are described in
[CONTRIBUTING.md](https://github.com/leinardi/gx52/blob/main/CONTRIBUTING.md).
