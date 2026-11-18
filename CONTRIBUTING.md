# Contributing

Bug reports, X52 test reports, translations and pull requests are welcome. GX52 is maintained in
spare time, so reviews happen on a best-effort basis.

## Opening issues

Use the issue templates: a bug report, a feature request or a question. For a bug, give the GX52
version (About dialog), how it was installed (Flathub stable, Flathub beta or a source build), the
device (X52 or X52 Pro, and its `lsusb` line) and the terminal output with `RUST_LOG=debug`. Owners
of a plain X52 are especially welcome: GX52 is developed on an X52 Pro.

Security issues go through private reporting, not a public issue: see [SECURITY.md](SECURITY.md).

## Prerequisites

- Linux with GTK 4 and libadwaita at the versions the GNOME 51 runtime ships, the SQLite
  development files, Meson, `blueprint-compiler` and the two validators `meson setup` refuses to
  run without (`desktop-file-validate`, `appstreamcli`). On Ubuntu 26.04:

  ```bash
  sudo apt install libgtk-4-dev libadwaita-1-dev libsqlite3-dev blueprint-compiler meson \
      desktop-file-utils appstream
  ```

- Rust through `rustup`: `rust-toolchain.toml` pins the release and its components.
- [`pre-commit`](https://pre-commit.com/): install the hooks once with `make pre-commit-install`
  (it installs both the `pre-commit` and the `commit-msg` hook).
- For the Flatpak builds: `flatpak` with the `org.flatpak.Builder` Flatpak from Flathub.
- For `make screenshots`: `weston` (`sudo apt install weston`) and `python3`, next to the Flatpak
  prerequisites.

To reach the joystick from a development build, the host needs the udev rule the application
shows when it cannot open the device; the rule is explained in
[docs/protocol.md](docs/protocol.md#udev-rule).

## Building and running

The development build has the app ID `com.leinardi.gx52.Devel`, so it installs next to the stable
application and keeps its own settings and profiles.

```bash
make meson-setup     # once: meson setup _build -Dprofile=development
make meson-compile   # build with Meson
make meson-run       # run uninstalled, with the compiled resources and schema
make flatpak-install # build the Devel Flatpak and install it for the user
flatpak run com.leinardi.gx52.Devel
```

`make help` lists every target. The workspace and its threads are described in
[docs/architecture.md](docs/architecture.md), the device commands in
[docs/protocol.md](docs/protocol.md).

## Checks

```bash
make verify       # build, rustfmt check, clippy -D warnings, tests, rustdoc, meson test
make check        # the full pre-commit suite, the authoritative local gate
make check-stage  # the same on the staged files only
```

CI runs the same checks on every pull request (`.github/workflows/ci.yaml`); its `ci-result` job
is the required check. The table in [AGENTS.md](AGENTS.md#validation-matrix) says which extra
checks a change to a given file needs.

A few rules the tools do not enforce:

- Nothing blocks the GTK main thread: USB transfers, evdev reads and file I/O run on the device
  service's workers or in async tasks.
- The `x52` crate has no GTK dependency, and only the commands in
  [docs/protocol.md](docs/protocol.md) are ever sent, only to a device whose vendor and product ID
  are known.
- Every user-visible string is translatable, and every interactive widget has an accessible label.
- Every Rust and shell source carries the GPL-3.0-or-later header with the year of its first
  commit (the `check-license-headers` hook checks it).

## Branches and pull requests

Development happens on `main`, the trunk: open pull requests against it. Name branches
`<type>/<short-description>` (`fix/mfd-clock-offset`). Keep a pull request to one logical change,
and make sure CI passes.

Pull requests are merged with merge commits, so every commit lands on the trunk as it is and counts
toward the next version: each commit needs a correct message, not just the pull request.

## Commit messages

All commits follow [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/)
with a scope: `<type>(<scope>)[!]: <description>`, lower case and imperative. The `commit-msg`
hook enforces it.

```text
fix(mfd): clamp clock offsets
feat(leds): add a blink editor
docs(protocol): describe the brightness request
```

Types: `feat`, `fix`, `docs`, `test`, `refactor`, `perf`, `build`, `ci`, `chore`, `style`,
`revert`. Mark a breaking change with `!` before the colon or a `BREAKING CHANGE:` footer.

From `1.0.0` on, the release version is derived from these types: any `feat` makes the next
release a minor, any `fix` a patch, `!` a major, and the other types bump nothing. Pick the type by
whether the change should ship: a refactor, a performance improvement or a revert that changes the
application users receive is a `fix` (or a `feat`). The `feat` and `fix` subjects also become the
release notes in the metainfo file. See [docs/release.md](docs/release.md).

## Translations

Every translatable source is listed in `po/POTFILES.in` (a meta-test checks the list is complete).
To add a language, add its code to `po/LINGUAS` and its `.po` file to `po/`; to refresh the
template after string changes:

```bash
meson compile -C _build gx52-pot
meson compile -C _build gx52-update-po
```

## Reused code

A file adapted from another project of the same author keeps a provenance line below the license
header (`Adapted from <repo>/<path> at <commit>, originally <MIT|Apache-2.0>-licensed, ...`). Keep
it when changing the file.
