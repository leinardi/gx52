# Rewrite preparation

The work to do before the long coding session on the Rust rewrite: housekeeping, spikes that retire
the technical risks, then the documentation and the roadmap that the coding session follows. The
decisions behind it are in [rewrite-analysis.md](rewrite-analysis.md).

Order: stage A and stage B can overlap; stage C is written from the spike results, so it comes last.

## Stage A: housekeeping

- [x] Import the `leinardi/gx52` GitHub repository into `gh-leinardi-iac` (OpenTofu stack
      `stacks/github-repos`) and manage it like the other repositories: description, homepage
      (drop the "mirror of the GitLab repository" wording), topics, settings, branch rules.
      Configuration committed on `gh-leinardi-iac` branch `feat/gx52-import`; the `tofu import`,
      plan review and apply are done by the owner.
- [x] Close Dependabot PR #2 (`requests` bump for the Python app) and delete its branch.
- [x] Recreate the relevant open GitLab issues on GitHub, with a link to the original:
      #3 "No POV1 LED Control" (now GitHub #3) and #6 "Fix permissions on default flatpak
      installation for Pop!OS users" (now GitHub #4). #5 "python strange error" is obsolete with the
      rewrite.
- [ ] Decide on GitHub #1 "[feature request] Deb Snap ?" (opened on the mirror in 2023).
- [x] Update section 7 of `rewrite-analysis.md` with the current state: the GitLab push mirror is
      off, GitHub is in sync and is the working repository.

Deferred, not part of this preparation: the GitLab tombstone (section 7.4 of the analysis) is pushed
to GitLab, and the GitLab project archived, right before the first public `v1.0.0` release. Until
then GitLab stays as it is.

## Stage B: spikes

Throwaway code on a separate `spikes` branch that is never merged. Each spike records its outcome in
`docs/spikes/NN-name.md`: question, what was tried, what worked, decision.

| # | Question | Needs |
|---|---|---|
| S1 | Can `nusb` send the vendor control transfers without detaching the kernel HID driver, so the joystick keeps working? If not, does `rusb`? | X52 Pro |
| S2 | Verify every command on the device: LEDs, both brightness values, MFD text, clocks 1 to 3, 12h/24h, date formats, shift, blink. Find the MFD character set and whether clock offsets are relative to clock 1. Becomes `docs/protocol.md`. | X52 Pro |
| S3 | evdev next to the USB control: X52 Pro key codes, mode selector behaviour, unplug / replug with `nusb::watch_devices`. | X52 Pro |
| S4 (partial ✅, hotplug pending) | S1 to S3 inside a minimal Flatpak with `--device=all`: USB access, input device, hotplug events, behaviour when the udev rule is missing. | X52 Pro |
| S5 (D-Bus ✅, visual and portal pending) | Tray: integrating `ksni` with the GLib main loop; GNOME with the AppIndicator extension, KDE, inside Flatpak, behaviour without a tray host. Background portal for autostart and hidden-window running. | desktop session |
| S6 ✅ | CI and release dry run: hello-world gtk-rs + Meson + Blueprint + Flatpak app; GitHub Actions on x86_64 and aarch64 (clippy in the SDK build shell, caching, durations); `cargo-sources.json` generation and `flatpak-builder-lint`. | nothing |
| S7 (✅ placeholders, artwork and hands-on check pending) | Visual LED editor: rendering stick and throttle with clickable LEDs in GTK 4 (SVG plus hotspot overlay, or custom snapshot drawing); source and licence of the artwork. | look and feel input |
| S8 ✅ | Legacy import: read a real `gx52.db` and check schema and enum values against the mapping in the analysis. | old database, if available |
| S9 (✅ desk research and uinput, hands-on pending) | Virtual mouse: ministick as mouse pointer like the Windows driver; prior art, evdev codes, uinput access on the host and in the sandbox. | X52 Pro |

Desk research, can run in parallel with the spikes (results in
[spikes/00-desk-research.md](spikes/00-desk-research.md)):

- [x] Do `game-devices-udev` / `steam-devices` already cover the X52 product IDs?
- [x] Flathub beta branch process for an existing app.
- [x] Time zones: `chrono-tz` vs `glib::TimeZone` (uses the runtime's tzdata, nothing compiled in).
- [x] `ksni` async runtime requirements (input for S5).

Start with S1 and S2: S1 is the risk that changes the most if it fails.

The remaining steps that need a person at the device (photos, button presses, hotplug, looking
at the tray and the LED editor) are collected in [hands-on.md](hands-on.md).

## Stage C: documentation and roadmap

Written after the spikes, so they describe what has been proven.

- [ ] `docs/architecture.md`: crates, threads and data flow, device service, state model, error
      handling, where the 1.x features plug in.
- [ ] `docs/protocol.md`: USB commands and evdev key maps as verified in S2 and S3.
- [ ] `docs/profiles.md`: TOML schema and versioning, legacy import mapping.
- [ ] `docs/ui.md`: screens, navigation, libadwaita widgets, wireframes (Blueprint sketches or ASCII).
- [ ] `docs/release.md`: Flatpak, beta and stable channels, versioning procedure.
- [ ] `docs/roadmap.md`: milestones `0.99.0`, `0.99.x`, `1.0.0`, `1.x`, each with an ordered task
      list and a "done when" condition per task. The first milestone is the project setup (Makefile,
      pre-commit, CI, Meson scaffold), built on what S6 proved. This is the input for the coding
      session.
- [ ] `docs/rewrite-analysis.md`: kept as the decision record, updated where spikes change a
      decision.
