# S6: build and release dry run

Throwaway app generated with the `gtk-rs-new-app` scaffold (gtk-rs-skills 0.1.0) in
`spikes/s6-hello` on the `spikes` branch: app id `com.leinardi.gx52`, Devel id
`com.leinardi.gx52.Devel`, crate `gx52`, GPL-3.0-or-later. Run on 2026-10-28.

## Versions

| Component | Host (Ubuntu) | GNOME 51 SDK (Flathub) |
|---|---|---|
| GTK | 4.22.4 | 4.24.1 |
| libadwaita | 1.9.1 | 1.10.0 |
| GLib | 2.88 | 2.90.1 |
| blueprint-compiler | 0.19.0 | 0.22.2 |
| Meson | 1.10.1 | 1.12.0 |
| Rust | 1.98.0 | rust-stable 26.08 extension |

Crate features available today: `gtk4` 0.11.5 has `v4_24` (GTK 4.24) but its newest bundle is
`gnome_50`; `libadwaita` 0.9.2 has `gtk_v4_24` but stops at `v1_9`. So GNOME 51 can be targeted
(`gtk` features `gnome_50` + `v4_24`, `adw` features `v1_9` + `gtk_v4_24`), but libadwaita 1.10 APIs
are not usable from Rust until the crate adds `v1_10`. The GNOME 51 SDK ships `blueprint-compiler`;
no extra module needed.

## Results

| Step | Result | Time |
|---|---|---|
| Host build (`meson setup` + `compile`, template as generated: GNOME 50 features) | OK | 51 s cold |
| Host `meson test` (desktop, metainfo, schema validation + `cargo test`) | 4/4 OK | <1 s |
| Host `cargo check` after raising to GTK 4.24 features / Meson `gtk4 >= 4.24` | fails: host GTK is 4.22 | |
| Devel Flatpak build (GNOME 51, network allowed, runs `meson test`) | OK, 4/4 tests | 57 s |
| Flathub-style Flatpak build (offline, crates from `cargo-sources.json`, inline `.cargo/config.toml`) | OK | 58 s |
| `flatpak-cargo-generator.py` on `Cargo.lock` | 101 crate archives for 102 packages (the other is the app), 56 KB | 1 s |
| `flatpak-builder-lint manifest` (stable and Devel) | no findings, including `--device=all` and `--talk-name=org.kde.StatusNotifierWatcher` | |
| `flatpak-builder-lint repo` | only missing screenshots (expected for a hello world; mirroring is done by Flathub's builder) | |

Notes:

- `flatpak-builder` run from the `org.flatpak.Builder` Flatpak needs `--disable-rofiles-fuse`
  (otherwise `Failure spawning rofiles-fuse`). The `flatpak-github-actions` container already passes
  it.
- The generator runs with `uv run --script` (it declares its dependencies inline); no virtualenv to
  maintain in the release workflow.
- The Meson setup keeps `CARGO_HOME` inside the build directory, and with `builddir: true` cargo
  finds the vendored-sources `.cargo/config.toml` in the source root, so the offline build needs no
  extra configuration.

## Decision needed: host development vs latest runtime

Raising the crate features and Meson checks to GNOME 51 means the project no longer builds on hosts
with GTK 4.22 (the maintainer's Ubuntu), so all development would happen inside the SDK (Flatpak
builds, GNOME Builder, or an SDK shell). Two options:

1. **Required = GNOME 50, runtime = GNOME 51.** Crate features `gnome_50` / `v1_9`, Meson
   `gtk4 >= 4.22`, `libadwaita-1 >= 1.9`; the Flatpak runs on GNOME 51. Host builds work, CI tests on
   the GNOME 51 runtime, features are raised only when an API needs it. Recommended.
2. **Required = runtime = GNOME 51.** Matches "latest everything" literally, but forces SDK-only
   development and gains nothing until the code uses a GTK 4.24 API (and libadwaita 1.10 isn't
   reachable from Rust yet anyway).

## Not done yet

- GitHub Actions run of the scaffold's workflow (Flatpak build on `ubuntu-24.04` and
  `ubuntu-24.04-arm`, clippy in the SDK build shell, timings with cache). Needs a push of a workflow
  to `leinardi/gx52`.
- The scaffold's CI template uses `gnome-50` images; switch to `gnome-51`.
