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

## CI on GitHub Actions

Throwaway workflow on a temporary `ci-spike` branch (deleted afterwards), three runs on 2026-10-28.
A `push` trigger works on any branch; only `workflow_dispatch` needs the workflow on the default
branch.

| Job | Runner | Result | Time |
|---|---|---|---|
| rustfmt | `ubuntu-24.04` | OK | 5 s |
| Flatpak build + `meson test` (`flatpak/flatpak-github-actions/flatpak-builder@v6`, container `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51`, `--privileged`) | `ubuntu-24.04` (x86_64) | OK | 5.5–6 min |
| Same | `ubuntu-24.04-arm` (aarch64) | OK | 4.7–5.5 min |
| clippy in the SDK build shell | `ubuntu-24.04`, same container | OK on the third run | 4.6 min |

Findings:

- The free `ubuntu-24.04-arm` runners build the aarch64 Flatpak natively, as fast as x86_64; no
  QEMU needed.
- The flatpak-builder cache (keyed on `Cargo.lock`) is restored, but the app module is rebuilt
  every time because its source (the checkout) changes, and cargo's target directory is not part of
  that cache. Most of the time is compiling the gtk-rs crates. Worth trying later: a separate
  cache of cargo's target directory, or `sccache`. Fine as it is for a start.
- The clippy job runs `flatpak-builder --stop-at=gx52`, then `--build-shell` with network allowed
  (`flatpak build-finish --share=network`) and pipes `cargo clippy` into it. The container image
  ships the GNOME SDK but **not** the Rust SDK extension and has **no `flathub` remote**: the job
  must run `flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo`
  and `flatpak-builder --install-deps-from=flathub` (system installation, no `--user`). The first
  two runs failed on exactly these two points.
- `actions/checkout@v4` warns about the Node 20 deprecation; use `@v5`.

## Decision: host development vs latest runtime

Taken 2026-10-28: **required = GNOME 50, runtime = GNOME 51.** Crate features `gnome_50` /
`v1_9` + `gtk_v4_22`, Meson `gtk4 >= 4.22`, `libadwaita-1 >= 1.9`, Flatpak runtime GNOME 51. The
code builds on the maintainer's Ubuntu 26.04 host, CI tests on the GNOME 51 runtime, and features
are raised only when an API needs it. `spikes/s6-hello` was switched back to these values and the
CI runs above used them.

Rejected: required = runtime = GNOME 51, which forces SDK-only development and gains nothing until
the code uses a GTK 4.24 API (and libadwaita 1.10 is not reachable from Rust yet anyway).
