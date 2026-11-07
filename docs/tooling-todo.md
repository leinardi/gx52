# Tooling still to set up

The project tooling (Makefile and `.mk` fragments, pre-commit, editor and linter configs, AI
tooling, GitHub templates) was ported from `swarm-device-access` and `gh-leinardi-iac` ahead of the
rewrite, as planned in [section 6 of the rewrite
analysis](rewrite-analysis.md#6-tooling-porting-the-reference-standard-to-this-repository). This
page lists what was left out because it depends on code or decisions that do not exist yet. Tick an
item off, or delete it, in the commit that does it.

## Needs the Cargo workspace

- [x] Root `Cargo.toml` with the `gx52` (root package), `x52` and `meta` members, `rust-version`
      equal to `rust-toolchain.toml`, and `[workspace.lints]` ported from `swarm-device-access`.
      The three crates are empty stubs (`fn main() {}` and two doc-only libraries) so the cargo
      hooks have something to check.
- [x] Replace the stubs with the real crates: gtk4, libadwaita and the other dependencies with the
      version features from spike S6, the constants of `src/config.rs` set by Meson through the
      environment. `x52` stays a doc-only library until M1.
- [x] The `meta` crate, adapted from `swarm-device-access/meta`: `docs` (relative links and
      fragments), `license_headers` (driving `scripts/check-license-headers.sh` against
      scratch repositories), `toolchain` (rust-toolchain.toml, `rust-version` and the CI pins
      agree), `version` (`Cargo.toml` and `meson.build` agree, and the newest metainfo `<release>`
      too once the version is not `0.0.0`, [release.md](release.md#one-version-source)),
      `workspace_lints`, `vacuous_flags`. No Dockerfile tests. Then:
  - [x] add the `docs-links` hook (`cargo test --locked -p meta --test docs`) to
        `.pre-commit-config.yaml`;
  - [x] point the "Run" column of the validation matrix in `AGENTS.md` and §6 of the
        `adversarial-review` skill at the meta-tests, as `swarm-device-access` does;
- [x] The `ci_gates` meta-test (every job in `ci-result.needs`, every skipped hook mapped to the
      job that runs it), with `ci.yaml`.
- [x] `deny.toml`: add each licence the dependency graph needs (MIT, Apache-2.0, Unicode-3.0, ...)
      with the crate that brings it in, checking GPL-3.0-or-later compatibility for each.
- [x] `clippy.toml`: prune `doc-valid-idents` to the names the docs really use (none yet: the
      list is clippy's default).
- [x] `clippy.toml`: add `disallowed-methods` with the injected clock of the MFD clock code (M1),
      banning direct reads of the system time outside it.
- [x] `rust-toolchain.toml`: align the pin with the Rust release of the
      `org.freedesktop.Sdk.Extension.rust-stable//26.08` extension the Flatpak builds use: `1.98.0`
      on Flathub, one patch release behind the `1.98.1` the other projects pin.

## Needs the Meson / Flatpak scaffold

- [x] `.mk/meson.mk` (setup with the Devel profile, compile, test, install, run) and
      `.mk/flatpak.mk` (build and install the Devel manifest, bundle, `flatpak-builder-lint`,
      regenerate `cargo-sources.json`), built on what spike S6 proved; add them to `MK_LOCAL_FILES`
      and `verify`. `flatpak-lint-repo` reports the missing screenshots until M8 adds them, so it is
      not a gate before then.
- [x] Pre-commit hooks for the data files, once their paths and the Devel/stable templating exist:
      `appstreamcli validate --no-net` on the metainfo, `desktop-file-validate` on the desktop
      file, `glib-compile-schemas --strict --dry-run` on the schema, `flatpak-builder-lint manifest`
      on the Devel manifest. Decide per hook whether it runs in pre-commit, in `meson test`, or
      both (the gtk-rs-new-app scaffold validates the generated files in `meson test`). Decided:
      the three validators stay in `meson test`, on the generated files, with
      `desktop-file-validate` and `appstreamcli` required by `meson setup` so a missing tool cannot
      drop a test; `flatpak-builder-lint manifest` on both manifests is the `flatpak-builder-lint`
      hook.
- [x] `scripts/hooks/rustfmt-on-edit.sh`: consider a `blueprint-compiler format --fix` branch for
      `.blp` files.
- [x] `.editorconfig`: add sections for `*.blp` and `meson.build` if their formatters' indentation
      differs from the default.
- [x] `.gitattributes`: mark generated files (`po/*.pot`, a vendored `cargo-sources.json` if one is
      ever kept here) as `linguist-generated`. Only `po/*.pot`: `cargo-sources.json` is generated
      and ignored, never committed.

## Needs CI and the release flow

- [x] `.github/workflows/ci.yaml` (rewrite analysis, section 6.3): pre-commit on the bare runner
      with `SKIP=cargo-clippy,cargo-test,blueprint-format,meson-format`, clippy and the format
      checks in the GNOME SDK build shell, the Flatpak build and `meson test` on `x86_64` and
      `aarch64`, conventional commits, the `ci-result` gate. `pre-commit-warmup.yaml` is already
      ported and expects the pre-commit job to restore the cache under the same key. Also skipped
      there: `cargo-deny` (the `deny` job) and `flatpak-builder-lint` (the `flatpak` job); the
      meta-tests run in their own bare-runner job, `meta`.
- [x] `docs/release.md`: the release specification (rewrite analysis, section 9).
- [ ] `.github/workflows/release.yaml`, implementing [release.md](release.md).
- [ ] `scheduled-scan.yaml` (dependency audit only) and, possibly, a check for a newer GNOME
      runtime on Flathub (rewrite analysis, section 8).
- [ ] Repository settings in `gh-leinardi-iac`: required checks once `ci.yaml` exists, the Flathub
      token secret for the release workflow, private vulnerability reporting (linked from
      `SECURITY.md` and the issue template config).

## Docs and project files

- [ ] `CONTRIBUTING.md`: rewrite for the Rust project (build from source with Meson, Flatpak
      Devel build, `make check`, commit conventions), adapted from `swarm-device-access`.
- [ ] `RELEASING.md`: replace with `docs/release.md`.
- [ ] `README.md`: rewrite for the new application (install from Flathub, udev rule, features).
- [x] `AGENTS.md`: add the architecture section once `docs/architecture.md` exists.
- [x] `AGENTS.md`: add the Meson and Flatpak commands once their `.mk` fragments do.
- [ ] `.agents/skills/`: decide whether gx52 needs a domain skill like `swarm-device-access`'s
      `trust-boundary` (candidates: profile and legacy-database parsing, the device command path).
- [ ] `SECURITY.md`: check the security model against the final Flatpak `finish-args` (spike S4
      may need `--share=network` for hotplug) and the profile import/export path (portal or
      filesystem permission).
- [ ] `.serena/memories/`: write the project memories for the Rust code base.

## Cleanup when the rewrite lands

- [ ] Remove the legacy Python application and its tooling: `gx52/` (Python package), `bin/`,
      `build.sh`, `run.sh`, `t.py`, `requirements.txt`, `mypy.ini`, `pylintrc`, `.pycharm/`,
      `scripts/make_local_manifest.py`, `scripts/meson_post_install.py`. They stay in the tree,
      unbuilt, until then: the Rust scaffold already replaced the legacy `meson.build`,
      `data/meson.build` and `bin/meson.build`, the GTK 3 `.glade` files and the legacy data files
      ([roadmap.md](roadmap.md#m0-project-setup-the-0990-skeleton), M0). They were brought up to the
      new checks (GPL header, shellcheck) only so `make check` passes in the meantime.
- [ ] Remove the `flatpak/` submodule (rewrite analysis, section 9.3), its mapping in
      `.idea/vcs.xml`, and the `!.idea/vcs.xml` exception in `.gitignore` if nothing else needs
      it.
- [ ] Remove `.gitlab/` with the GitLab tombstone (rewrite analysis, section 7.4).
- [ ] Rename the trunk to `main` (rewrite analysis, section 1, Branching).
