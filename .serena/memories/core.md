# Core — GX52

GNOME app (Rust, gtk4-rs + libadwaita, Blueprint, Meson, Flatpak `com.leinardi.gx52`) that sets
the Logitech/Saitek X52 and X52 Pro LEDs, brightness, MFD text and clocks, with profiles per mode
selector position. Rust rewrite on the `rust-rewrite` trunk (0.99.x on Flathub beta); `master` holds
legacy Python 0.7.x until 1.0.0 (M9). Legacy Python in `gx52/` (not a crate), `bin/`, `*.py`: never
extend.

## Source map

```
Cargo.toml          # workspace: "." (gx52 app, src/), x52, meta; [workspace.lints]
src/
  main.rs, config.rs          # entry; build constants from Meson env (Devel defaults for cargo)
  application.rs, application/  # AdwApplication: actions, autostart + background (ashpd portal),
                              #   clocks, editing, persistence (writes on worker)
  window.rs                   # main window; import/export via gtk::FileDialog
  device/                     # device service, all off the main thread:
    watch.rs                  #   hotplug (nusb watch on host, 250 ms poll in sandbox), x52d check
    writer.rs                 #   command channel, coalescing, replay on connect, 5 s clock check
    reader.rs                 #   evdev: mode selector (200 ms debounce), input events
    mouse.rs, mouse_loop.rs   #   virtual mouse over uinput, 10 ms tick
    usb.rs, access.rs, check.rs, udev.rs, x52d.rs, tester.rs, status.rs
  profile/                    # TOML profiles: schema, store, files, writes, legacy (0.7 SQLite
                              #   import), migrate, exchange (import/export), active (mode->profile)
  model/                      # GObject models (profile_object, splice)
  preferences.rs              # GSettings wrapper
  tray.rs, tray/              # ksni StatusNotifierItem on its own async-io executor
  widgets/                    # composite widgets, one Blueprint template each
  zones.rs                    # time zones for the MFD clocks (jiff)
x52/src/            # no GTK: command (range-checked constructors), transport trait, charset
                    #   (MFD), clock, input codes, device ids, state
meta/               # repository meta-tests (headers, docs links, CI gates, version, translations)
data/               # .blp UI, gresource, gschema, desktop, metainfo, icons
build-aux/          # Flatpak manifests (stable + Devel), cargo-sources.json (generated)
docs/               # architecture, protocol, profiles, release, roadmap, rewrite-analysis, spikes
```

## Key invariants

- Nothing blocks the GTK main thread; workers send plain `Send` data over `async_channel`.
- Only commands in `docs/protocol.md`, only to known vendor/product IDs; unsupported capability is
  hidden, never tried. Device commands only via `x52` constructors.
- `docs/architecture.md` is the design reference (threads, state model, persistence, sandbox).

See `mem:tech_stack` (crates, versions, toolchain), `mem:conventions` (lints, headers, commits,
i18n, Makefile), `mem:suggested_commands`, `mem:task_completion` (checks before finishing).
