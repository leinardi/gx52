# Release

How a GX52 release is versioned, built and published to Flathub, what to do when one fails halfway,
and what has to be set up by hand. The decisions are in [rewrite-analysis.md](rewrite-analysis.md)
sections 8 and 9; what the build and CI jobs were proven to do is in [spike
S6](spikes/06-build-and-release.md). The pipeline, `.github/workflows/release.yaml`, is not written
yet ([tooling-todo.md](tooling-todo.md#needs-ci-and-the-release-flow)); this page is its
specification.

This page supersedes [RELEASING.md](../RELEASING.md), the manual procedure of the legacy `0.7.x`
Python application. `RELEASING.md` stays until `release.yaml` lands and is removed then
([tooling-todo.md](tooling-todo.md#docs-and-project-files)).

## What a release produces

- A `vX.Y.Z` tag on the release commit, which bumps the version in `Cargo.toml`, `Cargo.lock`,
  `meson.build` and the metainfo.
- A GitHub release with the notes and two Flatpak bundles, `gx52-x86_64.flatpak` and
  `gx52-aarch64.flatpak`, built from the tag. `0.99.x` releases are marked as pre-releases.
- A pull request on [flathub/com.leinardi.gx52](https://github.com/flathub/com.leinardi.gx52)
  against the chosen channel branch (`beta` or `master`), with the new tag and commit and a
  regenerated `cargo-sources.json`. The maintainer merges it by hand; Flathub then builds and
  publishes.

## Versions and channels

- **Tags** are `vX.Y.Z`, as in every recent GitHub project of the maintainer. The legacy tags
  `0.7.0` … `0.7.6` (no `v`) stay as history.
- **Channels:** Flathub has two branches, `master` (stable) and `beta`. The release workflow takes
  the **target channel as an explicit input** (`channel`: `auto`, `beta` or `master`). The version
  only sets the default: rewrite analysis section 9.4 has the channel "defaulting to `beta` for
  `0.x` versions and to `master` from `1.0.0` on", and `auto`, the input's default value, is that
  rule (a GitHub `choice` input cannot compute its default from another input). `beta` and `master`
  are the explicit choices of the plan. So a 1.x feature can still go through `beta` first by
  choosing `beta` (rewrite analysis, section 9.4). On top of the plan, a `0.x` version sent to
  `master` is refused: `0.99.x` must not replace `0.7.6` on stable before `1.0.0` (rewrite analysis,
  section 12.1).
- **Development builds are `0.99.x`, always with an explicit `version` input.** `svu` is not used
  before `1.0.0`: on a `0.x` version it turns a `feat` into `0.100.0` and a breaking change into
  `1.0.0`. A run with no version on a `0.x` history fails and asks for one.
- **`1.0.0` is released explicitly**; from then on a run with no version derives it with `svu` from
  the Conventional Commits since the last release. Which commit type bumps what is the table in
  [AGENTS.md, "Commit messages"](../AGENTS.md#commit-messages); it is not repeated here. A run with
  only "none" commits since the last tag fails with "nothing to bump".
- **Higher than every release:** derived or typed, the version must be higher than every version
  already released. Two sets count as releases: the strict tags below and, explicitly, the legacy
  tags `0.7.0` … `0.7.6` without `v`, which `svu` does not see. Apart from those legacy tags, only
  strict `vMAJOR.MINOR.PATCH` tags count as releases (no pre-release or build-metadata suffixes, no
  leading zeros), as in awtrix-controller's release workflow.

Users of the beta channel opt in once (desk research, Flathub beta channel):

```bash
flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo
flatpak install flathub-beta com.leinardi.gx52
```

## One version source

The version is decided in one place, the release workflow's version step, and written by one bump
step into every file that carries it:

| File | What changes |
| --- | --- |
| `Cargo.toml` | `[package] version` of the root package (today `0.0.0`, set by the release workflow) |
| `Cargo.lock` | the `gx52` entry, updated with `cargo update -p gx52 --offline` |
| `meson.build` | `project(..., version: 'X.Y.Z')`; Meson generates `src/config.rs` with it, so the About dialog shows the same value |
| `data/com.leinardi.gx52.metainfo.xml.in.in` | a new `<release version="X.Y.Z" date="YYYY-MM-DD">` at the top of `<releases>`, with a description built from the `feat` and `fix` subjects since the last release |

Nothing else may hold a version. A `meta` test (a `version` test next to the `toolchain` checks of
[tooling-todo.md](tooling-todo.md#needs-the-cargo-workspace)) fails when `Cargo.toml`, `meson.build`
and the newest metainfo `<release>` disagree, so a hand edit cannot drift.

A `<release>` entry whose version was tagged but never published (a run that failed at the gate, see
[Recovering a failed run](#recovering-a-failed-run)) is removed by the next bump step. It removes
only an entry whose `vX.Y.Z` tag exists and has no GitHub release (a version burned by a failed
gate); every other entry stays, including the `0.7.0` … `0.7.6` history of the legacy application,
which was published from GitLab without `v` tags or GitHub releases.

## CI jobs

`ci.yaml` is specified in [tooling-todo.md](tooling-todo.md#needs-ci-and-the-release-flow) and
rewrite analysis section 6.3. Its Flatpak jobs come from the S6 reference workflow
(`spikes/s6-hello/build-aux/ci-spike.reference.yaml` on the `spikes` branch), which passed on
GitHub Actions on 2026-10-28. The release workflow reuses the same jobs. The lessons S6 paid for:

- **Native arm runner.** The Flatpak build is a matrix of `x86_64` on `ubuntu-24.04` and `aarch64`
  on the free `ubuntu-24.04-arm` runner. The arm build is native and as fast as x86_64 (about 5–6
  minutes each); no QEMU.
- **Container:** `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` with `--privileged`, and
  `flatpak/flatpak-github-actions/flatpak-builder@v6`, cache keyed on the architecture and
  `Cargo.lock`. `actions/checkout@v5` (v4 warns about Node 20).
- **The flathub remote and the Rust extension.** The container image ships the GNOME SDK but **not**
  the `rust-stable` SDK extension, and has **no `flathub` remote**. A job that runs
  `flatpak-builder` itself (the clippy job) must first run `flatpak remote-add --if-not-exists
  flathub https://dl.flathub.org/repo/flathub.flatpakrepo` and pass `--install-deps-from=flathub`
  (system installation, no `--user`). The first two S6 runs failed on exactly these points.
- **`--disable-rofiles-fuse`** on every `flatpak-builder` call made outside the action (the action
  already passes it); without it the build fails with "Failure spawning rofiles-fuse".
- **Clippy in the SDK build shell:** `flatpak-builder --stop-at=gx52`, then
  `flatpak build-finish --share=network` on the build directory and `cargo clippy` piped into
  `--build-shell=gx52`. The runner's own GTK is older than the runtime's (risk R7).
- **Offline build:** the Flathub-style build uses only the crates listed in `cargo-sources.json`; S6
  built it offline without extra configuration (Meson keeps `CARGO_HOME` in the build directory).

## How to release

Run the **Release** workflow from the Actions tab (or `gh workflow run release.yaml`) on the trunk.
Inputs:

- `version`: `X.Y.Z` or `vX.Y.Z`. Required before `1.0.0`; optional after it (derived with `svu`).
- `channel`: `auto` (default), `beta` or `master`; see [Versions and
  channels](#versions-and-channels).
- `dry_run`: boolean, default `false`; see [Dry runs](#dry-runs).

**One release runs at a time:** the workflow uses a `release` concurrency group with
`cancel-in-progress: false`, so a second dispatch queues instead of racing the first.

`workflow_dispatch` only lists workflows that are on the repository's **default branch** (S6). Until
the rewrite lands (M9) the default branch is the legacy `master`, while releases are cut from
`rust-rewrite`; how the first `0.99.x` is dispatched (the workflow file also on `master`, or
`rust-rewrite` as the default branch for the beta period) is decided when `release.yaml` is written
in milestone M8, and recorded here.

## The release workflow

Following rewrite analysis section 9.1, in this order. Everything that publishes comes after the
gate.

1. **Dispatch.** Manual `workflow_dispatch` with the `version`, `dry_run` and `channel` inputs, in
   the `release` concurrency group. A guard job fails (rather than skips) when the run is not on the
   trunk.
2. **Compute and check the version.** Take the `version` input, or run `svu` (after `1.0.0`). Check
   it against [Versions and channels](#versions-and-channels): strict format, higher than every
   release including the legacy tags, and a valid channel for the version. If `vX.Y.Z` already
   exists on the remote, continue only as a [recovery](#recovering-a-failed-run): when the tag
   points at `HEAD`, or at a `chore(release): vX.Y.Z` commit whose parent is `HEAD` (the run's own
   bump commit, when the version job is re-run after a successful push). In that case step 3 does
   not bump again: it re-reads the existing tag and the later jobs check it out.
3. **Bump, commit, tag, push.** The bump step writes the version into `Cargo.toml`, `Cargo.lock`,
   `meson.build` and the metainfo `<releases>` ([One version source](#one-version-source)), commits
   it as `chore(release): vX.Y.Z`, creates the tag `vX.Y.Z` on that commit and pushes both in one
   `git push --atomic origin <trunk> vX.Y.Z`, so either both land or neither does. The push uses the
   release identity of [One-time setup](#one-time-setup), because the trunk is protected.
4. **Gate: build, test and lint from the tag, on both architectures.** Each build job checks out the
   tagged commit and builds it with the stable manifest, whose app source is the checkout (a `dir`
   source, as in S6), and a `cargo-sources.json` generated from the tagged `Cargo.lock` (the same
   offline build Flathub does). For the gate, the job's copy of the manifest sets `"run-tests":
   true` on the app module, as the S6 Devel manifest does, so `meson test` runs inside the build:
   - build the Flatpak on `x86_64` and on `aarch64` (native runners, as in CI);
   - `meson test` runs inside the build (desktop file, metainfo and GSettings schema validation, and
     `cargo test`);
   - run `flatpak-builder-lint manifest` on the manifest and `flatpak-builder-lint repo` on the
     resulting repository.

   **Nothing is published if any of these fails**: no GitHub release, no Flathub pull request. The
   tag stays (see [Recovering a failed run](#recovering-a-failed-run)).
5. **Create the GitHub release** for the tag, with the notes (the metainfo release description plus
   the list of commits) and the two Flatpak bundles from step 4. `0.99.x` is marked as a
   pre-release.
6. **Open the Flathub pull request.** Clone `flathub/com.leinardi.gx52`, check out the chosen
   channel branch (`beta` or `master`), then:
   - update the app's git source in the manifest to the new `tag` and its `commit`;
   - regenerate `cargo-sources.json` from the tagged `Cargo.lock` with `flatpak-cargo-generator.py`,
     run with `uv run --script` (it declares its dependencies inline, S6), from a pinned commit of
     `flatpak-builder-tools`;
   - run `flatpak-builder-lint manifest` on the result;
   - commit on a branch `release/vX.Y.Z`, push it, and open the pull request against the channel
     branch with `gh pr create`, authenticated with a fine-grained token stored as a repository
     secret (the default `GITHUB_TOKEN` cannot write to another repository).
7. **Merge manually.** The workflow stops after opening the pull request. Flathub's build bot builds
   it on every architecture `flathub.json` allows; once its test build passes (it can be installed
   from the bot's test repository), the maintainer merges (rewrite analysis, section 9.2).
   Automating the merge was rejected there.

## Dry runs

`dry_run: true` runs steps 1, 2 and 4 against a bump commit and tag that are never pushed. The
version job makes them locally and hands them to both build jobs as a `git bundle` artifact; each
build job clones the bundle and builds that checkout exactly as in step 4 (the stable manifest's app
source is a `dir` source, so it needs nothing from GitHub). It then prepares the Flathub change of
step 6 without pushing it and prints the resolved version and channel, the metainfo `<release>`
entry, the Flathub manifest diff and the `cargo-sources.json` diff. Nothing is pushed, tagged on the
remote, released or opened. Use it before the first release of each channel and after any change to
the workflow.

## Recovering a failed run

Every step after the push is idempotent. The recovery path is **"Re-run failed jobs" on the original
run** in the Actions tab: it keeps that run's commit and inputs, so it works even after other
commits have landed on the trunk. A fresh dispatch with the same `version` also recovers, but only
while the trunk tip is still the tagged commit (the version check accepts an existing tag only on
`HEAD` or on the bump commit just above it, and the guard only runs on the trunk). Either way the
run skips what already exists (the GitHub release, the Flathub branch or pull request) and does what
is missing.

| Failed at | State | What to do |
| --- | --- | --- |
| 1–2 (dispatch, version) | Nothing changed | Fix the input or the history, run again |
| 3 (push) | The atomic push either landed both the commit and the tag, or neither | Neither: run again. Both: re-run the failed jobs |
| 4 (gate) | Tag pushed, nothing published | If the failure was transient (runner, network), re-run the failed jobs. If the code is wrong, fix it on the trunk and release the **next** patch version; the failed tag is never moved or reused |
| 5 (GitHub release) | Tag pushed, gate passed | Re-run the failed jobs |
| 6 (Flathub PR) | GitHub release exists | Re-run the failed jobs; an existing `release/vX.Y.Z` branch or pull request is updated, not duplicated |
| Flathub build bot fails | PR open | Fix in the Flathub PR if the fix is packaging-only; otherwise release the next patch version and close the PR |

A tag is never deleted or moved after it was pushed, so a version number is never reused for
different code.

## One-time setup

- **Flathub token:** a fine-grained personal access token of the maintainer, limited to
  `flathub/com.leinardi.gx52`, with "Contents: read and write" and "Pull requests: read and write",
  stored as a repository secret of `leinardi/gx52` (the secret name is set in `release.yaml`). The
  token's resource owner is the `flathub` organisation, so it only works if that organisation
  accepts fine-grained tokens from the maintainer; that is checked when the token is created, and
  the outcome is recorded here. Renewal date in the maintainer's calendar.
- **Flathub `beta` branch, before the first beta release:** it does not exist yet, and the workflow
  does not create it. It is created by hand first, following Flathub's documented flow (a pull
  request against `master` for the test build, then `beta` created from `master` with the branch
  merged in; desk research). Only then can step 6 check it out. On `beta`, and later on `master`,
  `flathub.json` allows `x86_64` and `aarch64`, and the manifest's source URL points at GitHub
  instead of GitLab (rewrite analysis, section 9.3).
- **Release identity:** step 3 pushes a commit to the protected trunk and creates a `vX.Y.Z` tag.
  The workflow pushes as a dedicated identity (a GitHub App installed on `leinardi/gx52`, or a
  deploy key) that is a bypass actor of the trunk's ruleset, and the tag rules allow it to create
  `v*` tags but not to update or delete them. Pushes made with the default `GITHUB_TOKEN` would not
  trigger other workflows; that does not matter here because the gate builds and tests the tag
  itself.
- **Required checks and branch protection** on `leinardi/gx52` (the `ci-result` gate of `ci.yaml`,
  tag rules) through `gh-leinardi-iac`, as listed in
  [tooling-todo.md](tooling-todo.md#needs-ci-and-the-release-flow).
- **Dependabot:** already ported in `583fc54` (`.github/dependabot.yml`). Production Cargo
  dependencies are committed as `fix(deps)` and so trigger a patch release after `1.0.0`; dev
  dependencies, actions and pre-commit hooks are `chore(deps)` (rewrite analysis, section 8). The
  GNOME runtime and the `rust-stable` extension have no Dependabot ecosystem and are bumped by hand
  with the GNOME releases.
- **Private vulnerability reporting** enabled on `leinardi/gx52`, as [SECURITY.md](../SECURITY.md)
  links to it ([tooling-todo.md](tooling-todo.md#needs-ci-and-the-release-flow)).
