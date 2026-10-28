# Desk research (2026-10-28)

Answers to the desk-research items of [preparation.md](../preparation.md), stage B. Gathered from
upstream sources and documentation, not from experiments; versions and commits are the ones current
on the date above.

## udev coverage for the raw USB device

Question: does any distribution-shipped rule grant user access to `/dev/bus/usb/...` for `06a3:0762`,
`06a3:0255`, `06a3:075c`? (The evdev node is already covered by systemd's joystick `uaccess` rule;
control transfers need the raw USB node.)

- **systemd** (main, 2026-10-29): no. The IDs only appear as names in `20-usb-vendor-model.hwdb`.
  `70-uaccess.rules` grants joysticks `uaccess` on the `input` subsystem only; `SUBSYSTEM=="usb"`
  `uaccess` is limited to hwdb categories (audio, media players, signal analyzers, AV production,
  AV lights, hardware wallets), none of which fits.
- **game-devices-udev** (Codeberg, 2026-10-10): no `06a3` entry. Precedents exist for flight-sim gear
  (Thrustmaster, VKB, on `hidraw`) and for raw USB nodes (`SUBSYSTEM=="usb"`, `MODE="0660"`,
  `TAG+="uaccess"`, e.g. Microsoft, Nintendo, Sony, 8BitDo). Contribution: one PR per device with a
  verified rule and a README line; active project. Distributed mostly via AUR, NixOS and manual copy;
  it periodically imports steam-devices.
- **steam-devices** (2026-06-25): no `06a3` entry. Scope is devices Steam itself needs; an X52 LED/MFD
  rule is unlikely to be accepted.
- **libx52** ships `60-saitek-x52-x52pro.rules` using a group instead of `uaccess`:
  `SUBSYSTEM=="usb", ATTRS{idVendor}=="06a3", ATTRS{idProduct}=="0762", MODE="0664", GROUP="plugdev"`
  (plus `hidraw` and the other two IDs). Users of libx52 packages may already have access.

Decision: keep the in-app guidance with a host rule
(`SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="06a3", ATTR{idProduct}=="0762|0255|075c", TAG+="uaccess"`
in `/etc/udev/rules.d/70-gx52.rules`, validated in S1); propose a `saitek-gdu.rules` to
game-devices-udev once verified on hardware. Skip steam-devices and systemd.

## Flathub beta channel

- Available to apps already on Flathub; no admin step documented. Flow: open a PR against `master`,
  wait for the test build, create `beta` from `master`, merge the PR branch into it, push; the push
  triggers an official build. After that, `beta` is protected and changes go through PRs.
- Only `master` (stable) and `beta` branches are allowed. Each branch has its own `flathub.json`
  (the current one restricts to `x86_64`), so the architecture change must be made on `beta` too.
- Beta builds are not listed on the website, inactive builds may be cleaned up, nightly or daily
  snapshots are not allowed, and users must switch to stable manually afterwards.
- Users: `flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo`,
  `flatpak install flathub-beta com.leinardi.gx52`, `flatpak run --branch=beta com.leinardi.gx52`
  (stable and beta can be installed side by side).
- Version ordering between beta and stable is not documented; they are separate refs.
- Example of an app using both branches: `flathub/org.gnome.Fractal`.

Decision: the release workflow targets the `beta` branch for `0.99.x`. The first beta release creates
the branch following the documented flow (a one-time manual step), later ones are PRs like on
`master`.

## Time zones

- The GNOME 51 runtime ships `/usr/share/zoneinfo` (tzdata 2026e); Flatpak maps the host zone onto it
  via `/etc/localtime`.
- `chrono-tz` 0.10.4 embeds tzdata 2025b at compile time (already stale), can list zones, has no
  transition API.
- `jiff` 0.2 reads the system tzdb (`/usr/share/zoneinfo`) on Linux, lists zones
  (`jiff::tz::db().available()`), gives offset and DST info, and iterates transitions
  (`TimeZone::following` / `preceding`).
- `glib::TimeZone` uses the system tzdb but cannot list zones and has no transition API.

Decision: `jiff` with default features. Zone data updates with the runtime instead of being frozen
in the binary, and transitions give the exact moment to re-sync the MFD clocks.

## Tray: ksni

- `ksni` 0.3.7: features `tokio` (default) or `async-io`, mutually exclusive. With `async-io` it runs
  its own executor thread, so it can be spawned from `glib::spawn_future_local` with no tokio.
- Used this way by gtk4-rs apps on Flathub (Packet, SongRec). Menu callbacks run on ksni's thread:
  send an enum through an `async_channel` and handle it in a `glib::spawn_future_local` loop; push
  state to the tray with `Handle::update`.
- No `StatusNotifierWatcher` (stock GNOME without the AppIndicator extension): `spawn()` fails with
  `Error::Watcher(ServiceUnknown)`, or with `assume_sni_available(true)` it succeeds and reports
  through `Tray::watcher_offline` / `watcher_online`, which also covers a shell that starts later.
- Flatpak: `--talk-name=org.kde.StatusNotifierWatcher` (not flagged by `flatpak-builder-lint`) and
  `disable_dbus_name(true)` (the sandbox can't own `org.kde.StatusNotifierItem-<pid>-<n>`). Use an
  icon name with the app-ID prefix.
- `tray-icon` is not an option: its default backend is GTK 3 libappindicator, and its `ksni` backend
  is a wrapper with no benefit here.

Decision: `ksni = { version = "0.3", default-features = false, features = ["async-io"] }`, spawned
with `disable_dbus_name(true)` and `assume_sni_available(true)`; the watcher callbacks decide
whether closing the window hides it (tray available) or quits (no tray). S5 verifies this on GNOME
and inside Flatpak.

## Sources

- <https://codeberg.org/fabiscafe/game-devices-udev>
- <https://github.com/ValveSoftware/steam-devices>
- <https://github.com/nirenjan/libx52/blob/master/udev/60-saitek-x52-x52pro.rules.in>
- <https://github.com/systemd/systemd/blob/main/rules.d/70-uaccess.rules.in>
- <https://docs.flathub.org/docs/for-app-authors/maintenance>
- <https://docs.flathub.org/docs/for-app-authors/requirements>
- <https://github.com/flathub/org.gnome.Fractal/tree/beta>
- <https://github.com/iovxw/ksni>, <https://github.com/nozwock/packet>, <https://github.com/marin-m/SongRec>
- <https://github.com/flathub-infra/flatpak-builder-lint/blob/master/flatpak_builder_lint/checks/finish_args.py>
- <https://github.com/tauri-apps/tray-icon>
- crates.io sources: `chrono-tz` 0.10.4, `jiff` 0.2.38, `glib` 0.22.10, `ksni` 0.3.7
