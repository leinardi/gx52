# Release

How a GX52 release is versioned, built and published to Flathub, what to do when one fails halfway,
and what has to be set up by hand. The decisions are in [rewrite-analysis.md](rewrite-analysis.md)
sections 8 and 9; what the build and CI jobs were proven to do is in [spike
S6](spikes/06-build-and-release.md). The pipeline is four workflows here, and one in the Flathub
repository:

| Workflow | Runs on | Does |
| --- | --- | --- |
| `.github/workflows/release.yaml` | dispatch | the version policy, then the bump pull request; a dry run gates the bumped tree |
| `.github/workflows/release-publish.yaml` | push of the version files to the trunk | decides that the push is a release, gates it, checks the Flathub files, tags and publishes it |
| `.github/workflows/release-gate.yaml` | called by both | the Flathub-style build, `meson test` and the linters on both architectures |
| `.github/workflows/flathub-check.yaml` | called by both | the Flathub manifest of the tag and `cargo-sources.json`, linted and diffed; pushes nothing |
| `update.yml` in `flathub/com.leinardi.gx52` | weekly, and dispatch | the Flathub pull request of a new tag, per channel branch ([The Flathub update workflow](#the-flathub-update-workflow)) |

The shared logic is in `leinardi/gh-reusable-workflows` (`version-bump-pr`, `release-on-merge`,
`simple-tag-and-release`), the same pattern as `rust-common` and `gtk-rs-skills`.

This page supersedes [RELEASING.md](../RELEASING.md), the manual procedure of the legacy `0.7.x`
Python application, which goes with the legacy tree in M9
([tooling-todo.md](tooling-todo.md#cleanup-when-the-rewrite-lands)).

## What a release produces

- A merged pull request `chore(release): bump version to X.Y.Z` with one commit, which changes the
  version in `Cargo.toml`, `Cargo.lock`, `meson.build` and the metainfo.
- A `vX.Y.Z` tag on the merge commit, created only after the gate passed.
- A GitHub release with the notes (the merged pull requests) and four assets built from the merge
  commit: two Flatpak bundles, `gx52-x86_64.flatpak` and `gx52-aarch64.flatpak`, plus
  `cargo-sources.json` (generated from the tagged `Cargo.lock`) and `com.leinardi.gx52.json` (the
  generated Flathub manifest). A `0.x` release is a pre-release.
- A pull request on [flathub/com.leinardi.gx52](https://github.com/flathub/com.leinardi.gx52) for
  each channel branch that follows the tag ([Versions and channels](#versions-and-channels)), from
  the branch `updates/<channel>`, with the new tag and commit and the release's
  `cargo-sources.json`. It is opened by the Flathub repository's own update workflow, not by this
  repository. The maintainer merges it by hand; Flathub then builds and publishes.

## Versions and channels

- **Tags** are `vX.Y.Z`. The legacy tags `0.7.0` … `0.7.6` (no `v`) stay as history.
- **Development builds are `0.99.x`, always with an explicit `version` input.** `svu` is not used
  before `1.0.0`: on a `0.x` history it turns a `feat` into `0.100.0` and a breaking change into
  `1.0.0`. `scripts/release-policy.sh`, the first job of `release.yaml`, refuses an empty version
  while no `vX.Y.Z` tag is `1.0.0` or higher, and an explicit one that is neither `0.99.x` nor
  `1.0.0`.
- **`1.0.0` is released explicitly**; from then on a run with no version derives it with `svu` from
  the Conventional Commits since the last release. Which commit type bumps what is the table in
  [AGENTS.md, "Commit messages"](../AGENTS.md#commit-messages). A run with only "none" commits
  since the last tag fails with "nothing to bump".
- **Higher than every release:** the version must be higher than every `vX.Y.Z` tag, than the
  legacy tags `0.7.0` … `0.7.6` (never below `0.7.6`, even without them) and than the version the
  tree carries. Only strict `vMAJOR.MINOR.PATCH` tags count (no suffixes, no leading zeros).
- **Channels:** Flathub has two branches, `master` (stable) and `beta`. No workflow here has a
  channel input: the channel is decided on the Flathub side, by the `x-checker-data` of each
  branch's manifest, which the update workflow follows (rewrite analysis, section 9.4):
  - `beta` follows every `vX.Y.Z` tag;
  - `master` keeps the legacy `0.7.6` manifest, with no `x-checker-data`, until the `1.0.0`
    migration pull request ([One-time setup](#one-time-setup)), so it gets no update pull request
    and `0.99.x` cannot replace `0.7.6` on stable (rewrite analysis, section 12.1). From then on
    every tag is `1.0.0` or higher, and the checker takes the highest tag, so `master` needs no
    version constraint of its own.
- **Beta first, and holding a release:** from `1.0.0` on, a release gets a pull request on both
  branches. The `master` one is opened as a draft. Promoting a release to stable means marking that
  draft ready and merging it once beta is good. Holding it back means **leaving the draft open,
  never closing it**: the update workflow does not remember a closed pull request, and its next run
  opens the same update again. A held version is not rejected for good: the next tag updates the
  same `master` pull request in place and supersedes it.

Users of the beta channel opt in once (desk research, Flathub beta channel):

```bash
flatpak remote-add --if-not-exists flathub-beta https://flathub.org/beta-repo/flathub-beta.flatpakrepo
flatpak install flathub-beta com.leinardi.gx52
```

## One version source

The version is resolved in one place, version-bump-pr's resolve step, and written by one command,
`make release-bump VERSION=X.Y.Z` (`scripts/release-bump.sh`), into every file that carries it:

| File | What changes |
| --- | --- |
| `Cargo.toml` | `[package] version` of the root package (`0.0.0` until the first release) |
| `Cargo.lock` | the `gx52` entry, rewritten by a plain `cargo metadata` resolve, which needs the registry index (the network): nothing else in the lock may change |
| `meson.build` | `project(..., version: 'X.Y.Z')`; Meson sets it as a constant of `src/config.rs` through the environment of its cargo invocations, so the About dialog shows the same value |
| `data/com.leinardi.gx52.metainfo.xml.in.in` | a new `<release version="X.Y.Z" date="YYYY-MM-DD">` at the top of `<releases>`, with the `feat` and `fix` subjects since the last `vX.Y.Z` tag; the first release, with no `v` tag yet, gets a fixed description of the rewrite |

`make -s release-version` prints the version the tree carries; both reusable workflows read it
with that command. Nothing else may hold a version: the `version` meta-test fails when
`Cargo.toml`, the `gx52` entry of `Cargo.lock`, `meson.build` and the newest metainfo `<release>`
disagree.

A `<release>` of the rewrite whose `vX.Y.Z` tag does not exist was merged but never published (its
gate failed, see [Recovering a failed run](#recovering-a-failed-run)): the next bump removes it, and
its changes are listed again under the new version. The legacy `0.7.x` entries always stay.

The bump refuses a tree with uncommitted changes, and restores the tree when anything fails after
it wrote: then the four files changed, and `Cargo.lock` only in the `gx52` version line. Locally,
on a scratch clone: `make release-bump VERSION=0.99.0`.

## CI jobs

`ci.yaml`'s Flatpak jobs come from the S6 reference workflow, which passed on GitHub Actions on
2026-10-28; `release-gate.yaml` reuses them. The lessons S6 paid for:

- **Native arm runner.** The Flatpak build is a matrix of `x86_64` on `ubuntu-26.04` and `aarch64`
  on the free `ubuntu-26.04-arm` runner; no QEMU.
- **Container:** `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` with `--privileged`, and
  `flatpak/flatpak-github-actions/flatpak-builder`. The image ships the GNOME SDK, `uv` and the
  linters, but not the `rust-stable` SDK extension and no `flathub` remote.
- **`--volume /var/lib/flatpak`** on the containers that run the action. Its
  `--install-deps-from` updates the image's runtimes in place, which on overlayfs fails with
  `Invalid cross-device link` and then a flatpak-builder segfault (exit 139), on some runner hosts
  only. The anonymous volume, filled from the image, keeps the installation off the overlay.
- **`--disable-rofiles-fuse`** on every `flatpak-builder` call made outside the action.
- **Offline build:** the Flathub-style build uses only the crates listed in `cargo-sources.json`,
  generated from `Cargo.lock` by `make cargo-sources` with the pinned generator.

## Dispatching before M9

GitHub registers a `workflow_dispatch` workflow only from the default branch, and runs a schedule
only from it. Until the rewrite lands (M9) the default branch is the legacy `master`, while releases
are cut from `rust-rewrite`. So `master` carries stubs of `ci.yaml` and `release.yaml`, with the
real files' dispatch inputs and a dispatch trigger only, and the real `scheduled-scan.yaml`.
`flathub-check.yaml` is only called, never dispatched, so it needs no stub. A dispatch then runs
the file of the ref it names:

```bash
gh workflow run release.yaml --ref rust-rewrite -f version=0.99.0 -f dry_run=false
gh workflow run release.yaml --ref m8/release -f version=0.99.0    # dry run, the default
```

A stub dispatched on `master` itself fails with "dispatch with --ref rust-rewrite". version-bump-pr
dispatches `ci.yaml` on `release/vX.Y.Z` the same way, which runs that branch's real `ci.yaml`. The
stubs have no push or pull-request trigger, so the legacy tree is never built. The stubs' inputs
are these; the `release_workflows` meta-test checks that the real files declare the same:

| Workflow | Input | Type | Required | Default |
| --- | --- | --- | --- | --- |
| `release.yaml` | `version` | string | no | `""` |
| `release.yaml` | `dry_run` | boolean | no | `true` |

A change to these inputs is made on both branches. The trunk's meta-tests cannot read `master`, so
the stubs and `scheduled-scan.yaml` are checked by hand once the bootstrap is merged: `gh workflow
list` shows CI, Release and Scheduled scan; a stub dispatched on `master` fails with its
message; `gh workflow run scheduled-scan.yaml` on `master` passes (or fails only in its
runtime job, naming a newer GNOME runtime).

At M9 the trunk becomes `main`, the default branch: the stubs go, `scheduled-scan.yaml` moves to
the trunk, and the `base_branch` inputs go
([tooling-todo.md](tooling-todo.md#cleanup-when-the-rewrite-lands)).

## How to release

1. **Dry run first**, from the trunk or a branch: `gh workflow run release.yaml --ref <branch> -f
   version=X.Y.Z`. Check the resolved version, the metainfo entry the bump job prints, the gate on
   both architectures and the Flathub check's manifest and diff.
2. **Dispatch the real run** on the trunk with `dry_run=false`. It opens the pull request
   `chore(release): bump version to X.Y.Z` from `release/vX.Y.Z` and dispatches CI on it.
3. **Merge it with a merge commit** once CI is green. Do not push to the branch or use "Update
   branch": to refresh it, dispatch the release again.
4. **The merge push runs `release-publish.yaml`:** release-on-merge checks that the push is the
   merge of exactly that one bump commit and waits for CI on the merge commit (up to 90 minutes),
   the gate builds and tests the merge commit and the Flathub check writes and lints the Flathub
   files, then simple-tag-and-release creates the tag by publishing the release with its four
   assets.
5. **Dispatch `update.yml` on the Flathub repository** (Actions, "Update", Run workflow), or wait
   for its weekly run. It opens the pull request of each channel that follows the tag.
6. **Merge the Flathub pull request** once Flathub's bot built it (it can be installed from the
   bot's test repository). Automating this merge was rejected (rewrite analysis, section 9.2). A
   `master` pull request is a draft until it is promoted
   ([Versions and channels](#versions-and-channels)).

Only one bump runs at a time (version-bump-pr's concurrency group). A newer dispatch of
`release.yaml` that queues behind a running one replaces an older pending one, as GitHub keeps one
pending run per group; dispatch it again if that was the real run.

## The gate

`release-gate.yaml` checks the tree a release would publish, before anything is published:

- it generates `cargo-sources.json` from the tree's `Cargo.lock`;
- it builds the stable manifest from a job copy with `"run-tests": true` on the app module, so
  `meson test` (desktop, metainfo and schema validation, `cargo test`) runs inside the offline
  build, as Flathub runs it; the screenshots are mirrored as Flathub's builds do;
- it runs `flatpak-builder-lint manifest` and `flatpak-builder-lint repo` (the repository lint is a
  gate from M8 on, now that the metainfo has screenshots);
- it uploads `gx52-x86_64.flatpak` and `gx52-aarch64.flatpak`, merged into one `gx52-bundles`
  artifact for the release.

A failure stops the run before the tag: nothing is published, and the version is never used.

## The Flathub check

`flathub-check.yaml` writes the files the Flathub side needs, next to the gate and before the tag:

- the manifest is the tree's `build-aux/com.leinardi.gx52.json` put through
  `scripts/flathub-manifest.sh`: the app's `dir` source becomes `{"type": "git", "url":
  "https://github.com/leinardi/gx52.git", "tag", "commit", "x-checker-data": {"type": "git",
  "tag-pattern": "^v([0-9]+\\.[0-9]+\\.[0-9]+)$"}}`, with the merge commit the tag will name, and
  everything else (the `cargo-sources.json` entry, the inline `.cargo/config.toml` that vendors the
  crates, the runtime, the SDK extension, the finish-args) is the tree's manifest's;
- `cargo-sources.json` is generated from the tree's `Cargo.lock` with the pinned generator.

`flatpak-builder-lint manifest` checks the manifest, and its diff against the Flathub branch is
printed for review. When the two differ in more than the app source's tag and commit (a runtime
bump, a new finish-arg), which the update workflow does not carry, the check warns and says so in
the run summary: that release needs a hand pull request on Flathub. Before the tag, the release also
checks that its merged assets are exactly the four files. Both files become release assets. The
check builds nothing of its own: before the tag exists the manifest can only be built from a `dir`
source, which is exactly what the gate builds. It pushes nothing and reaches no secret.

## The Flathub update workflow

`.github/workflows/update.yml` lives in `flathub/com.leinardi.gx52`, on its default branch
`master`, because scheduled and dispatch workflows only register from the default branch. It follows
the custom-workflow pattern of the Flathub documentation ("Automating updates") and of
`flatpak-external-data-checker`, and it opens the pull request with that repository's own
`GITHUB_TOKEN`. This repository holds no token for Flathub: a fine-grained personal access token
cannot write to a public repository of an organisation its owner is not a member of, and a classic
token's `public_repo` scope would grant write access to every public repository the account can
write to (rewrite analysis, section 9.1).

- **Triggers:** `schedule`, weekly at most (Flathub's guidance), and `workflow_dispatch`.
- **Matrix** over `beta` and `master`, one job per branch, in an `update-<branch>` concurrency
  group with `cancel-in-progress: false`, so overlapping runs queue instead of racing on the same
  head branch. A branch whose manifest has no `x-checker-data` is a no-op.
- **Check:** check out the branch, run `flatpak-external-data-checker --edit-only` on the manifest.
  It only edits the working tree; `--commit-only` checks out a branch of its own and plain
  `--update` opens a pull request itself, so neither is used. The checker moves the app source's
  `tag` and `commit` to the newest tag the `x-checker-data` matches.
- **Cargo sources:** if the tag changed, download the release's `cargo-sources.json` asset from
  `https://github.com/leinardi/gx52/releases/download/<tag>/` into the working tree. It is the file
  the release gate built with, so the generator stays pinned in one place (`.mk/flatpak.mk`).
- **Pull request:** `peter-evans/create-pull-request`, pinned by SHA, with `permissions: contents:
  write, pull-requests: write` and explicit inputs: `base` the matrix branch, a stable head branch
  `updates/<branch>` (never the shared default `create-pull-request/patch`), `draft` on `master`,
  and the tag in the title and commit message. Each channel has at most one open update pull
  request, and a repeat run updates it in place.
- **No automerge** (rewrite analysis, section 9.2), and Flathub's global checker is disabled with
  `"disable-external-data-checker": true` in `flathub.json`: it only runs on the default branch and
  does not refresh `cargo-sources.json`.
- **What it does not do:** it changes only the source `tag` and `commit` and `cargo-sources.json`. A
  runtime bump (twice a year), a finish-args change or any other manifest change goes to the
  Flathub repository as a hand pull request, made from the release's `com.leinardi.gx52.json`
  asset.

## Dry runs

`dry_run` is the default of both dispatch workflows, and a dry run may run from any ref.

- **`release.yaml`:** the policy job and version-bump-pr run exactly as in a real run: the version
  is resolved (`svu` included after `1.0.0`, so "nothing to bump" fails the same way), the bump
  command runs, and its patch is uploaded; nothing is pushed or opened. Then the gate runs on the
  dispatched commit with the patch applied, and the Flathub check on that same patched tree: its
  manifest and the `Cargo.lock` behind `cargo-sources.json` come from the bumped tree, and the
  manifest names the dispatched commit (no tag exists for it). The summary lists the version and
  both results.

The diff of a dry run is against `beta` for a `0.x` version, `master` from `1.0.0` on; before the
`beta` branch exists it is against `master`, and says so.

## Screenshots

The metainfo lists five screenshots under `data/screenshots/` (not installed), with translatable
captions. Their URLs follow the trunk
(`raw.githubusercontent.com/leinardi/gx52/rust-rewrite/data/screenshots/...`), not the tag: the
gate mirrors them before the tag exists. A side effect: a later screenshot change on the trunk also
changes what an older release shows. Off the trunk (a dry run of a pull request branch) the gate
points them at the dispatched commit instead, so a branch that adds or changes screenshots gates
its own. At M9 they move to `main`. The legacy `art/screenshot-*.png` stay until then.

`make screenshots` regenerates them, with no joystick and no manual set-up. It installs the Devel
Flatpak and runs its screenshot mode (`--screenshots <dir>`, Devel builds only; see
[architecture.md](architecture.md#testing-strategy)) on a private headless weston, from an
environment built from scratch: the fonts and libadwaita come from the GNOME runtime, and the
host's locale, theme, style, accent and scale never reach the shots. The app shows fixture
profiles, a fake X52 Pro in mode 1 and a fixed time, at 1000×700 (the profile list at 420×700), in
the light style without the Devel stripes. Two runs give byte-identical PNGs, so a diff in
`data/screenshots/` is a real change of the UI; look at each changed PNG before committing it.
Needs `weston` on the host (CONTRIBUTING.md).

| File | Shows |
| --- | --- |
| `data/screenshots/leds.png` | the main window with a per-mode profile, on the LEDs page |
| `data/screenshots/mfd.png` | the MFD page: the display preview and its text |
| `data/screenshots/clocks.png` | the Clocks page: three clocks in real time zones |
| `data/screenshots/profiles.png` | the profile list, in the narrow window |
| `data/screenshots/preferences.png` | Preferences |

The order is the metainfo's, and `src/screenshots/capture.rs` takes them in that order (a unit test
keeps the two in step); the `screenshots` meta-test checks that every image the metainfo names is a
PNG in git.

Until they are committed (on the trunk, or on the branch a dry run is dispatched from) the gate
cannot mirror them, and fails.

## Recovering a failed run

The recovery path is **"Re-run failed jobs"** on the failed run: every reusable workflow finishes
what exists and fails with "needs a human" on anything that disagrees with it.

| Failed at | State | What to do |
| --- | --- | --- |
| `release.yaml` policy or bump | Nothing changed | Fix the input or the history, dispatch again |
| The bump pull request's CI | PR open, nothing released | Fix on the trunk, then dispatch again: the PR is refreshed onto the new trunk |
| `release-publish.yaml` cancelled while pending | Bump merged, nothing tagged | A third push of a version file replaced it in release-on-merge's group: "Re-run all jobs" on it |
| `release-publish.yaml` decide | Bump merged, nothing tagged | A transient failure (API, CI timeout): re-run. A refusal (not the one bump commit): revert the change and dispatch a release |
| The gate | Bump merged, **no tag** | A transient failure (runner, network): re-run. If the code is wrong, fix it on the trunk and release the next patch version; the next bump drops the never-published `<release>` entry |
| The Flathub check | Bump merged, **no tag** | As the gate: re-run a transient failure, release the next patch version for a wrong manifest |
| publish (tag, release, assets) | Gate and check passed | Re-run; a draft or a published release is finished, never replaced |
| Flathub `update.yml` | GitHub release exists | Dispatch it again; the existing `updates/<branch>` pull request is updated, not duplicated |
| Flathub's build bot | PR open | Fix in the Flathub PR if the fix is packaging-only; otherwise release the next patch version, which updates the same PR |

A tag is created only after the gate and is never deleted or moved ("Immutable tags" ruleset), so
a version number is never reused for different code.

## One-time setup

- **No bypass identity.** The bump lands through a pull request like any other change, so nothing
  pushes to the protected trunk; tags are created by publishing the release with `GITHUB_TOKEN`.
- **Repository settings** (`gh-leinardi-iac`, `repo_gx52`): GitHub Actions may create pull
  requests (the bump PR); a ruleset on `rust-rewrite` until M9 (pull request, review, no deletion
  or force-push) with `ci-result` required; no required checks on the legacy `master`, whose
  `ci.yaml` stub never runs on a pull request. No environment and no secret: the `release`
  environment, applied for the Flathub token of the earlier design, is unused and is removed from
  the IaC.
- **No Flathub token.** The Flathub pull request is opened from the Flathub side
  ([The Flathub update workflow](#the-flathub-update-workflow)).
- **Flathub `beta` branch and migration pull request, before the first beta release:** `beta`
  does not exist yet, and no workflow creates it. The maintainer creates it with their own `gh`
  login after `v0.99.0` is released, following Flathub's documented flow (a pull request against
  `master` for the test build, then `beta` created from `master` with the branch merged in; desk
  research). That pull request migrates the tree: the manifest is the `v0.99.0` release asset
  `com.leinardi.gx52.json`, `cargo-sources.json` its asset, `flathub.json` becomes
  `{"only-arches": ["x86_64", "aarch64"], "disable-external-data-checker": true}`, and every other
  file goes except `README.md` (the Python modules, `pypi-dependencies.json`, the `shared-modules`
  and `flatpak-builder-tools` submodules and the script that refreshed them). If the flow does not
  let a maintainer push a new branch, the flow it prescribes is followed and recorded here.
- **`update.yml` on the Flathub `master`**, committed by hand. Before relying on it, check and
  record here:
  - that merging it to `master` does not trigger an unwanted republish of `0.7.6`, and that
    Flathub's linter accepts it;
  - whether a pull request opened with `GITHUB_TOKEN` triggers Flathub's build bot; if it does not,
    the maintainer comments `bot, build` on the pull request.
- **Actions may open pull requests on the Flathub repository.** The workflow's permissions are not
  enough: the repository needs "Allow GitHub Actions to create and approve pull requests"
  (Settings, Actions, General, Workflow permissions), and the `flathub` organisation policy must
  not block it. The `gh-leinardi-iac` setting of the same name covers `leinardi/gx52` only. The
  maintainer is not an admin of the Flathub repository: its settings page shows them only the
  Actions secrets and variables (checked on 2026-11-16), so the setting can neither be read nor
  changed from this side. It is found out by the first run of `update.yml` instead:
  - if `create-pull-request` opens the pull request, nothing more is needed;
  - if it fails with "GitHub Actions is not permitted to create or approve pull requests", the
    branch `updates/<branch>` is already pushed: the maintainer opens the pull request from it by
    hand (one click on the compare page), still with no token, and asks the Flathub admins to
    enable the setting (an issue on `flathub/flathub` or the Matrix channel).

  The outcome is recorded here.
- **`1.0.0` migration pull request to `master`:** `master` keeps the legacy manifest without
  `x-checker-data` until this pull request replaces it, so the update workflow cannot reach it
  before. When `v1.0.0` is released, the maintainer opens it by hand against `master`, with the
  same content as the `beta` migration, from the `v1.0.0` assets. From then on `update.yml` covers
  `master` too.
- **Private vulnerability reporting** on `leinardi/gx52`, as [SECURITY.md](../SECURITY.md) links to
  it: enabled by the IaC, checked once.
- **Dependabot:** production Cargo dependencies are committed as `fix(deps)` and so trigger a patch
  release after `1.0.0`; dev dependencies, actions and pre-commit hooks are `chore(deps)` (rewrite
  analysis, section 8). The GNOME runtime and the `rust-stable` extension have no Dependabot
  ecosystem: the weekly `scheduled-scan.yaml` fails when Flathub has a newer GNOME runtime than the
  manifests, and they are moved by hand.
