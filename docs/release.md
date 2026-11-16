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
- A GitHub release with the notes and four assets built from the tag: two Flatpak bundles,
  `gx52-x86_64.flatpak` and `gx52-aarch64.flatpak`, plus `cargo-sources.json` (generated from the
  tagged `Cargo.lock`) and `com.leinardi.gx52.json` (the generated Flathub manifest). `0.99.x`
  releases are marked as pre-releases.
- A pull request on [flathub/com.leinardi.gx52](https://github.com/flathub/com.leinardi.gx52) for
  each channel branch that follows the tag ([Versions and channels](#versions-and-channels)), with
  the new tag and commit and the release's `cargo-sources.json`. The PR is opened by the Flathub
  repository's own update workflow ([The Flathub update workflow](#the-flathub-update-workflow)),
  not by this repository. The maintainer merges it by hand; Flathub then builds and publishes.

## Versions and channels

- **Tags** are `vX.Y.Z`, as in every recent GitHub project of the maintainer. The legacy tags
  `0.7.0` … `0.7.6` (no `v`) stay as history.
- **Channels:** Flathub has two branches, `master` (stable) and `beta`. The release workflow has
  **no channel input**: the channel is decided on the Flathub side, by the `x-checker-data` of each
  branch's manifest (rewrite analysis, section 9.4):
  - `beta` follows every `v` tag;
  - `master` follows only tags `>= 1.0.0` (`"versions": {">=": "1.0.0"}`), so `0.99.x` never
    replaces `0.7.6` on stable before `1.0.0` (rewrite analysis, section 12.1). Until the `1.0.0`
    migration PR ([One-time setup](#one-time-setup)), `master` holds the legacy manifest with no
    `x-checker-data` and gets no update PR at all.
- **Beta first, and holding a release:** from `1.0.0` on, a release gets a PR on both branches. The
  `master` PR is opened as a draft. Promoting a release to stable means marking that draft ready
  and merging it once beta is good. Holding it back means **leaving the draft open, never closing
  it**: the update workflow does not remember a closed PR, and its next run opens the same update
  again. A held version is not rejected for good. The next tag updates the same `master` PR in
  place and supersedes it.
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
| `meson.build` | `project(..., version: 'X.Y.Z')`; Meson sets it as a constant of `src/config.rs` through the environment of its cargo invocations, so the About dialog shows the same value |
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

1. **Dispatch.** Manual `workflow_dispatch` with the `version` and `dry_run` inputs, in
   the `release` concurrency group. A guard job fails (rather than skips) when the run is not on the
   trunk.
2. **Compute and check the version.** Take the `version` input, or run `svu` (after `1.0.0`). Check
   it against [Versions and channels](#versions-and-channels): strict format, higher than every
   release including the legacy tags. If `vX.Y.Z` already
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
     resulting repository;
   - **Flathub check:** generate the Flathub manifest from the tagged
     `build-aux/com.leinardi.gx52.json` (`scripts/flathub-manifest.sh`: the `dir` source becomes a
     git source on `github.com/leinardi/gx52` at the tag and its commit, with the `x-checker-data`
     of [The Flathub update workflow](#the-flathub-update-workflow); everything else unchanged),
     run `flatpak-builder-lint manifest` on it and build it on both architectures, offline with the
     generated `cargo-sources.json`, as Flathub's bot will. It pushes nothing.

   **Nothing is published if any of these fails**: no GitHub release, so no Flathub pull request. The
   tag stays (see [Recovering a failed run](#recovering-a-failed-run)).
5. **Create the GitHub release** for the tag, with the notes (the metainfo release description plus
   the list of commits) and four assets from step 4: the two Flatpak bundles, `cargo-sources.json`
   and the generated Flathub manifest `com.leinardi.gx52.json`. `0.99.x` is marked as a
   pre-release. The release workflow ends here.
6. **Flathub update PR.** The maintainer dispatches `update.yml` on
   `flathub/com.leinardi.gx52` (or waits for its weekly run). It opens a PR for each channel branch
   that follows the tag; see [The Flathub update workflow](#the-flathub-update-workflow).
7. **Merge manually.** Flathub's build bot builds the PR on every architecture `flathub.json`
   allows; once its test build passes (it can be installed from the bot's test repository), the
   maintainer merges (rewrite analysis, section 9.2). Automating the merge was rejected there. A
   `master` PR is a draft until it is promoted ([Versions and channels](#versions-and-channels)).

## The Flathub update workflow

`.github/workflows/update.yml` lives in `flathub/com.leinardi.gx52`, on its default branch
`master`, because scheduled and dispatch workflows only register from the default branch. It follows
the custom-workflow pattern of the Flathub documentation ("Automating updates") and of
`flatpak-external-data-checker`, and it opens the PR with that repository's own `GITHUB_TOKEN`.
This repository holds no token for Flathub: a fine-grained personal access token cannot write to a
public repository of an organisation its owner is not a member of, and a classic token's
`public_repo` scope would grant write access to every public repository the account can write to
(rewrite analysis, section 9.1).

- **Triggers:** `schedule`, weekly at most (Flathub's guidance), and `workflow_dispatch`.
- **Matrix** over `beta` and `master`, one job per branch, in a `update-<branch>` concurrency group
  with `cancel-in-progress: false`, so overlapping runs queue instead of racing on the same head
  branch. A branch whose manifest has no `x-checker-data` is a no-op.
- **Check:** check out the branch, run `flatpak-external-data-checker --edit-only` on the manifest.
  It only edits the working tree; `--commit-only` checks out a branch of its own and plain
  `--update` opens a PR itself, so neither is used. The manifest's app source carries
  `"x-checker-data": {"type": "git", "tag-pattern": "^v([0-9]+\\.[0-9]+\\.[0-9]+)$"}`, plus
  `"versions": {">=": "1.0.0"}` on `master`, and the checker updates its `tag` and `commit`.
- **Cargo sources:** if the tag changed, download the release's `cargo-sources.json` asset from
  `https://github.com/leinardi/gx52/releases/download/<tag>/` into the working tree. It is the file
  the release gate built with, so the generator stays pinned in one place (`.mk/flatpak.mk`).
- **PR:** `peter-evans/create-pull-request`, pinned by SHA, with `permissions: contents: write,
  pull-requests: write` and explicit inputs: `base` the matrix branch, a stable head branch
  `updates/<branch>` (never the shared default `create-pull-request/patch`), `draft` on `master`,
  and the tag in the title and commit message. Each channel has at most one open update PR, and a
  repeat run updates it in place.
- **No automerge** (rewrite analysis, section 9.2), and Flathub's global checker is disabled with
  `"disable-external-data-checker": true` in `flathub.json`: it only runs on the default branch and
  does not refresh `cargo-sources.json`.
- **What it does not do:** it changes only the source `tag` and `commit` and `cargo-sources.json`. A
  runtime bump (twice a year), a finish-args change or any other manifest change goes to the
  Flathub repository as a hand PR, made from the release's `com.leinardi.gx52.json` asset, which the
  Flathub check of step 4 has already built.

## Dry runs

`dry_run: true` runs steps 1, 2 and 4 against a bump commit and tag that are never pushed. The
version job makes them locally and hands them to both build jobs as a `git bundle` artifact; each
build job clones the bundle and builds that checkout exactly as in step 4 (the stable manifest's app
source is a `dir` source, so it needs nothing from GitHub). The Flathub check of step 4 builds the
generated Flathub manifest with a `dir`-equivalent source in place of the unpushed tag. The run
prints the resolved version, the metainfo `<release>` entry, the generated Flathub manifest and the
`cargo-sources.json` diff. Nothing is pushed, tagged on the remote, released or opened. Use it before
the first release and after any change to the workflow.

## Recovering a failed run

Every step after the push is idempotent. The recovery path is **"Re-run failed jobs" on the original
run** in the Actions tab: it keeps that run's commit and inputs, so it works even after other
commits have landed on the trunk. A fresh dispatch with the same `version` also recovers, but only
while the trunk tip is still the tagged commit (the version check accepts an existing tag only on
`HEAD` or on the bump commit just above it, and the guard only runs on the trunk). Either way the
run skips what already exists (the GitHub release) and does what is missing.

| Failed at | State | What to do |
| --- | --- | --- |
| 1–2 (dispatch, version) | Nothing changed | Fix the input or the history, run again |
| 3 (push) | The atomic push either landed both the commit and the tag, or neither | Neither: run again. Both: re-run the failed jobs |
| 4 (gate) | Tag pushed, nothing published | If the failure was transient (runner, network), re-run the failed jobs. If the code is wrong, fix it on the trunk and release the **next** patch version; the failed tag is never moved or reused |
| 5 (GitHub release) | Tag pushed, gate passed | Re-run the failed jobs |
| 6 (Flathub `update.yml`) | GitHub release exists | Dispatch `update.yml` again; the existing `updates/<branch>` pull request is updated, not duplicated |
| Flathub build bot fails | PR open | Fix in the Flathub PR if the fix is packaging-only; otherwise release the next patch version, which updates the same PR |

A tag is never deleted or moved after it was pushed, so a version number is never reused for
different code.

## One-time setup

- **No Flathub token.** The Flathub PR is opened from the Flathub side
  ([The Flathub update workflow](#the-flathub-update-workflow)); `leinardi/gx52` holds no secret
  for it.
- **Flathub `beta` branch and migration PR, before the first beta release:** `beta` does not exist
  yet. The maintainer creates it by hand with their own `gh` login, following Flathub's documented
  flow (a pull request against `master` for the test build, then `beta` created from `master` with
  the branch merged in; desk research). That PR migrates the manifest: the Rust manifest from the
  `v0.99.0` release asset `com.leinardi.gx52.json` (git source on GitHub instead of GitLab, with its
  `x-checker-data`), the `cargo-sources.json` asset, `flathub.json` set to `{"only-arches":
  ["x86_64", "aarch64"], "disable-external-data-checker": true}`, and the Python modules,
  `pypi-dependencies.json` and `shared-modules` removed (rewrite analysis, section 9.3).
- **`update.yml` on the Flathub `master`**, committed by hand. Before relying on it, check and
  record here:
  - that merging it to `master` does not trigger an unwanted republish of `0.7.6`, and that
    Flathub's linter accepts it;
  - whether a PR opened with `GITHUB_TOKEN` triggers Flathub's build bot; if it does not, the
    maintainer comments `bot, build` on the PR.
- **Actions may open PRs on the Flathub repository.** The YAML permissions are not enough: the
  repository needs "Allow GitHub Actions to create and approve pull requests" (Settings, Actions,
  General, Workflow permissions), and the `flathub` organisation policy must not block it. The
  `gh-leinardi-iac` setting of the same name covers `leinardi/gx52` only. If a collaborator cannot
  change it, the maintainer asks the Flathub admins (an issue or the Matrix channel). Until it is
  enabled, `update.yml` pushes `updates/<branch>` and the maintainer opens the PR from it by hand.
  The outcome is recorded here.
- **`1.0.0` migration PR to `master`:** the `versions` constraint cannot bootstrap itself, because
  `master` keeps the legacy manifest without `x-checker-data` until this PR replaces it. When
  `v1.0.0` is released, the maintainer opens it by hand against `master`, with the same content as
  the `beta` migration plus `"versions": {">=": "1.0.0"}` in the `x-checker-data`. From then on
  `update.yml` covers `master` too.
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
