# AGENTS.md

## What this is

GX52 is a GNOME desktop application that configures the Logitech (Saitek) X52 and X52 Pro
joysticks on Linux: LEDs, brightness, the MFD (multi-function display) text and clocks, and
profiles. It is distributed through Flathub as `com.leinardi.gx52`.

The application is being rewritten from Python + GTK 3 to Rust + gtk4-rs + libadwaita, on the
`rust-rewrite` branch. The Python code still in the tree is the legacy `0.7.x` application and is
removed when the rewrite lands; do not extend it. The decisions behind the rewrite are recorded in
[`docs/rewrite-analysis.md`](docs/rewrite-analysis.md), the preparation in
[`docs/preparation.md`](docs/preparation.md) and the spike results in
[`docs/spikes/`](docs/spikes/). Tooling that is decided but not set up yet is tracked in
[`docs/tooling-todo.md`](docs/tooling-todo.md).

Target layout (rewrite analysis, section 1 and 5): a Cargo workspace with `x52` (protocol, device
model, key maps, MFD charset; no GTK), `gx52` (the application) and `meta` (repository hygiene
tests), built by Meson following the GNOME Rust app layout, with Blueprint templates, a GResource
bundle, a GSettings schema and gettext from the first commit.
The root package `gx52` in `src/` is the libadwaita application skeleton, built by Meson
(`meson.build`, `data/`, `po/`, Flatpak manifests in `build-aux/`); `x52/` and `meta/` are still
doc-only stubs. The legacy Python package lives, unbuilt, in `gx52/`, which is not a crate.

## Architecture

[`docs/architecture.md`](docs/architecture.md) describes how the app is built: the workspace
(`gx52`, `x52`, `meta`), the threads (GTK main thread, device writer, evdev reader, virtual-mouse
loop, device watch, ksni tray) and the channels between them, the state model (device type, active
mode mapped to a profile per type, clocks, overrides), persistence, errors and logging, the sandbox
permissions, the test strategy and the 1.x extension points. What the device does, and the only
commands the app may send, are in [`docs/protocol.md`](docs/protocol.md).

## Common commands

Build / test / lint — Make wrappers around `cargo`, `meson`, `flatpak` and `pre-commit`:

```bash
make verify        # verify-rust and meson-test: every check a change has to pass
make verify-rust   # rust-build, rust-fmt-check, rust-clippy (-D warnings), rust-test, rust-doc
make rust-build    # cargo build --workspace --all-targets --all-features --locked
make rust-test     # cargo test --workspace --locked (meta-tests included)
make rust-clippy   # cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
make rust-fmt      # cargo fmt --all
make rust-fmt-check  # cargo fmt --all --check
make rust-doc      # cargo doc --workspace --all-features --no-deps --locked, warnings as errors
make rust-deny     # cargo deny --workspace check (network required)
make audit-deps    # cargo deny --workspace check advisories (network required)
make check         # pre-commit on all files: the authoritative local gate
make check-stage   # pre-commit on staging area only
make pre-commit-install  # install the pre-commit and commit-msg hooks
make help          # every target with its one-line description

make meson-setup   # meson setup _build -Dprofile=development (once; Meson reconfigures itself)
make meson-compile # build the Devel app (com.leinardi.gx52.Devel) with Meson
make meson-test    # meson test: cargo test -p gx52 -p x52, desktop, metainfo and schema validation
make meson-run     # meson devenv -C _build ./src/gx52: run uninstalled
make meson-install # meson install -C _build

make flatpak-build # Devel Flatpak into _flatpak/repo via org.flatpak.Builder (runs meson test)
make flatpak-install  # the same, installed for the user: flatpak run com.leinardi.gx52.Devel
make flatpak-bundle   # _flatpak/com.leinardi.gx52.Devel.flatpak
make flatpak-lint-manifest  # flatpak-builder-lint on both manifests (also a pre-commit hook)
make flatpak-lint-repo      # flatpak-builder-lint on _flatpak/repo; reports missing screenshots until M8
make cargo-sources # regenerate build-aux/cargo-sources.json from Cargo.lock (network required)
```

The cargo and Meson targets need the GTK 4 and libadwaita development files on the host, at the
versions the crate features require (see
[`docs/spikes/06-build-and-release.md`](docs/spikes/06-build-and-release.md)), plus Meson,
`blueprint-compiler` and the two validators `meson setup` refuses to run without
(`desktop-file-validate`, `appstreamcli`). On Ubuntu 26.04:

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev blueprint-compiler meson desktop-file-utils appstream
```

The Flatpak targets need `flatpak` with the `org.flatpak.Builder` Flatpak from Flathub;
`cargo-sources` needs `uv` and `curl`.

The Makefile pulls shared snippets from `leinardi/make-common@v1` into `.mk/` on first run. To
refresh: `make mk-common-update`. Repo-local targets go in their own `.mk/<fragment>.mk`, listed in
the Makefile's `MK_LOCAL_FILES`; never add recipes to the Makefile itself.

### Generated or vendored: never hand-edit

- `.mk/{help,pre-commit}.mk` and `scripts/bootstrap-mk-common.sh`: fetched from make-common (under
  its MIT license) and refreshed with `make mk-common-update`; a change is ported upstream.

## Conventions worth knowing

- Every Rust and shell source, the Makefile and the repo-local `.mk` fragments carry the
  GPL-3.0-or-later header (the IntelliJ `GPL-3.0-or-later` copyright profile), with the year of the
  file's first commit, after any shebang. `scripts/check-license-headers.sh` (the
  `check-license-headers` hook) checks the whole tree; make-common's files and Rust fixtures under
  `testdata/` are exempt. A file reused from another project of the same author adds a provenance
  comment below the header (`Adapted from <repo>/<path> at <commit>, originally
  <MIT|Apache-2.0>-licensed, Copyright (c) <year> Roberto Leinardi`), complete, and never the
  original licence block.
- The `x52` crate has no GTK dependency: protocol encoding is pure functions over a transport
  trait, so it is unit-tested without a device or a display.
- Nothing blocks the GTK main thread. USB transfers, evdev reads and file I/O run on the device
  service's worker or in async tasks; the UI talks to them through channels.
- Every user-visible string is translatable from the first commit, and every interactive widget
  has an accessible label.
- The app ID is `com.leinardi.gx52`; development builds use `com.leinardi.gx52.Devel` so both
  can be installed side by side.
- Prose in Markdown wraps at 100 columns (markdownlint in `make check`).

## Project skills

Skills live in `.agents/skills/` (symlinked as `.claude/skills`). Load them before the work, not
after review:

- `adversarial-review` — for any review request ("review my diff", "is this ready to merge").

For Rust and GTK code, also load the user-level `gtk-rs-development` and `rust-best-practices`
skills (and `gtk-rs-review` / `rust-review` when reviewing).

## Quality rules not enforced by tooling

- **Reuse before writing.** Check the workspace for an existing helper before adding one, and
  check the reference projects (swarm-device-access, swarm-scheduler-exporter) for a component
  that can be adapted with its provenance comment instead of rewritten.
- **Never guess at the device.** A command is only sent to a device whose vendor and product ID
  are known, and only commands the protocol documentation lists; an unknown or unsupported
  capability is hidden in the UI, never tried.
- **Deletion smell.** Removing a user-visible surface (a setting, a profile field, a CLI option, a
  documented behaviour) and flipping its test to assert absence needs a line in `README.md` or
  `docs/**` that retires it; a commit message is not enough.
- **Suppressions show their work.** A new `#[expect(...)]` or `# shellcheck disable=` explains
  why the fix does not apply here, not which rule fired; `#[expect]` carries it in
  `reason = "..."`.

## Validation matrix

Run before considering an edit done. `make check` applies to every row.

| You edited | Run |
| --- | --- |
| `**/*.rs`, `clippy.toml`, `rustfmt.toml` | `make verify`, then `make check-stage` |
| a `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `deny.toml` | `make verify`, `make rust-deny` (network) |
| `**/*.blp`, `meson.build`, `meson.options`, `data/**`, `po/**` | `make meson-test`, `make check` (blueprint and meson format hooks); `make meson-run` when the change is visible |
| `build-aux/*.json` | `make flatpak-lint-manifest`, `make flatpak-install` |
| `.mk/meson.mk`, `.mk/flatpak.mk` | `make help`, and the targets the edit touched |
| `scripts/check-license-headers.sh` | `cargo test --locked -p meta --test license_headers` |
| `meta/**` | `make verify` |
| `meson.build`, `data/com.leinardi.gx52.metainfo.xml.in.in` (the version) | `cargo test --locked -p meta --test version` |
| `.pre-commit-config.yaml`, `Makefile`, `.mk/**` | `make check`, `cargo test --locked -p meta` (vacuous flags, whole-tree hooks), `make help` |
| `.github/workflows/**` | `pre-commit run actionlint --all-files`, `cargo test --locked -p meta` (Rust pins, vacuous flags, required gates and skipped hooks) |
| `scripts/hooks/rustfmt-on-edit.sh` | shellcheck (in `make check`); run it once by editing a `.rs` file in Claude Code |
| `README.md`, `docs/**/*.md`, other Markdown | `cargo test --locked -p meta --test docs` (the `docs-links` hook); prose wraps at 100 columns (markdownlint in `make check`) |
| `.agents/skills/**` | the Markdown row |

## Commit messages

All commits MUST be Conventional Commits 1.0.0 **with a scope**:
`<type>(<scope>)[!]: <description>`, optional blank-line body and footers. Enforced by the
`conventional-pre-commit` `commit-msg` hook (`--force-scope`). Types: `feat`, `fix`, `docs`,
`test`, `refactor`, `perf`, `build`, `ci`, `chore`, `style`, `revert`. Breaking changes use `!`
before `:` or a `BREAKING CHANGE:` footer. Examples: `fix(mfd): clamp clock offsets`,
`ci(dependabot): group cargo updates`.

From `1.0.0` on, release versions are derived by `svu` from the commits since the last tag, so a
wrong type ships a wrong version. The `0.99.x` development builds always give the version
explicitly (rewrite analysis, section 9.4).

| Release | Commit | Example |
| --- | --- | --- |
| major | any type with `!` before the colon, or a `BREAKING CHANGE:` footer | `feat(profiles)!: drop the v1 schema` |
| minor | `feat` | `feat(leds): add a blink editor` |
| patch | `fix` | `fix(device): reopen after resume` |
| none | everything else: `perf`, `refactor`, `build`, `ci`, `chore`, `docs`, `style`, `test`, `revert` | `docs(protocol): ...` |

The highest bump among the commits wins; with only "none" commits since the last tag, a release
with no version fails with "nothing to bump".

**Pick the type by whether the change should ship, not by what kind of change it is.** Anything
that changes the shipped application and that users should receive is `fix` (or `feat`), even when
it is a performance improvement, a refactor or a revert. Use `perf`, `refactor`, `style` and
`revert` only when the commit is deliberately not meant to trigger a release on its own. A `revert`
of a shipped `feat` or `fix` is itself a `fix`. `svu` matches `feat`/`fix` anywhere in the subject
(e.g. `prefix:` counts as `fix:`), so avoid a word ending in `feat` or `fix` directly before a colon
in other subjects. PRs land as merge commits, so every commit counts, not just the PR title.
