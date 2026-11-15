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
- [x] `.github/workflows/release.yaml`, implementing [release.md](release.md): `release.yaml`,
      `release-publish.yaml`, `release-gate.yaml` and `flathub-pr.yaml`, on gh-reusable-workflows'
      version-bump-pr, release-on-merge and simple-tag-and-release (M8).
- [x] `scheduled-scan.yaml`: cargo-deny on the trunk every week, and a check that fails when
      Flathub has a newer GNOME runtime than the manifests (rewrite analysis, section 8). On
      `master` until M9 ([release.md](release.md#dispatching-before-m9)).
- [ ] Repository settings in `gh-leinardi-iac` (`repo_gx52`, M8): a ruleset on `rust-rewrite`
      with `ci-result` required (none on the legacy `master`), Actions may create pull requests,
      the `release` environment deployable from `rust-rewrite`; private vulnerability reporting is
      already on for public repositories. Written on the IaC branch `feat/gx52-release`; tick when
      the maintainer applied it. By hand ([release.md](release.md#one-time-setup)): the
      `FLATHUB_TOKEN` environment secret, and the Flathub `beta` branch.

## Docs and project files

- [x] `CONTRIBUTING.md`: rewrite for the Rust project (build from source with Meson, Flatpak
      Devel build, `make check`, commit conventions), adapted from `swarm-device-access`.
- [x] `RELEASING.md`: replaced by `docs/release.md`; the file goes with the legacy tree in M9
      (below).
- [x] `README.md`: rewrite for the new application (install from Flathub, udev rule, features).
- [ ] `README.md`: the screenshot table (LEDs, MFD, profiles, Preferences), in the commit that adds
      the PNGs in `data/screenshots/` the metainfo points at.
- [x] `AGENTS.md`: add the architecture section once `docs/architecture.md` exists.
- [x] `AGENTS.md`: add the Meson and Flatpak commands once their `.mk` fragments do.
- [x] `.agents/skills/`: decide whether gx52 needs a domain skill like `swarm-device-access`'s
      `trust-boundary` (candidates: profile and legacy-database parsing, the device command path).
      Decided in M3: no. That skill exists because a privileged daemon there turns config into
      device access, and a fail-open parse exposes the host. In gx52, a sandboxed desktop app, the
      worst parse failure is a profile shown with built-in values. A wrong device command is
      already impossible by type: only the range-checked `x52` constructors build one. The
      boundaries, their rules and their tests are listed once, in the `adversarial-review`
      skill's untrusted-input pass, which every review runs.
- [x] `SECURITY.md`: check the security model against the final Flatpak `finish-args` (spike S4
      may need `--share=network` for hotplug) and the profile import/export path (portal or
      filesystem permission). Checked in M8: no network; import and export through the file
      chooser portal.
- [x] `.serena/memories/`: write the project memories for the Rust code base.

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
- [ ] Remove `RELEASING.md`, the legacy manual release procedure.
- [ ] Release workflows ([release.md](release.md#dispatching-before-m9)): remove the `ci.yaml`,
      `release.yaml` and `flathub-pr.yaml` stubs from the old `master`; move
      `scheduled-scan.yaml` to the trunk and drop its explicit `rust-rewrite` checkout; drop the
      `base_branch` inputs of the gh-reusable-workflows calls and every `rust-rewrite` ref in the
      release workflows (the trunk is then the default branch); point the metainfo screenshot
      URLs at `main`.
- [ ] Repository settings: move `ci-result` from the `rust-rewrite` ruleset to
      `default_branch_required_checks` and remove that ruleset, and set the `release`
      environment's deployment branch to `main`.
- [ ] `scheduled-scan.yaml` on the trunk: give it meta-test coverage like the release workflows'
      (`release_workflows`); on `master` the trunk's meta-tests cannot read it, so until then its
      check is the hands-on one in [release.md](release.md#dispatching-before-m9).
