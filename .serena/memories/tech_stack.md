# Tech Stack

- Rust edition 2024, exact toolchain in `rust-toolchain.toml`; workspace `rust-version` matches.
- gtk4 0.11 (`gnome_50`), libadwaita 0.9 (`v1_9`, `gtk_v4_22`): features, Meson `dependency()`
  checks and the Flatpak `runtime-version` move together. Flatpak runtime: GNOME 51.
- No tokio. Async: glib main loop, `async-io`/`futures-lite`; ksni and ashpd on zbus 5 async-io.
- USB: `nusb` (control transfers, hotplug). Input: `evdev` + `rustix` poll. Virtual mouse: uinput.
- Profiles: `toml`; legacy 0.7 import: `rusqlite` linked to system SQLite (not bundled).
- Time: `jiff` (system zone, MFD clocks). Errors: `thiserror`. Logs: `tracing`, `RUST_LOG`.
- i18n: `gettext-rs` (`gettext-system`), `po/POTFILES.in`, `po/LINGUAS`.
- Build: Meson (GNOME Rust app layout) drives cargo; Blueprint compiled by `blueprint-compiler`;
  `meson test` runs cargo tests plus desktop/metainfo/schema validation.
- Flathub offline build: `build-aux/cargo-sources.json` from `make cargo-sources`.
- Pre-commit: rustfmt, clippy, cargo test, cargo-deny, cargo-machete, shellcheck, checkmake,
  markdownlint (100 cols), actionlint, yamllint,
  blueprint and meson format, flatpak-builder-lint, license headers, conventional-pre-commit.
