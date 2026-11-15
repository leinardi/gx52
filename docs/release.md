# Release

How a GX52 release is versioned, built and published to Flathub, what to do when one fails halfway,
and what has to be set up by hand. The decisions are in [rewrite-analysis.md](rewrite-analysis.md)
sections 8 and 9; what the build and CI jobs were proven to do is in [spike
S6](spikes/06-build-and-release.md). The pipeline is four workflows:

| Workflow | Runs on | Does |
| --- | --- | --- |
| `.github/workflows/release.yaml` | dispatch | the version policy, then the bump pull request; a dry run gates the bumped tree |
| `.github/workflows/release-publish.yaml` | push of the version files to the trunk | decides that the push is a release, gates it, tags and publishes it, opens the beta Flathub pull request for `0.x` |
| `.github/workflows/release-gate.yaml` | called by both | the Flathub-style build, `meson test` and the linters on both architectures |
| `.github/workflows/flathub-pr.yaml` | dispatch, and called by both | the Flathub pull request of a tag to a channel |

The shared logic is in `leinardi/gh-reusable-workflows` (`version-bump-pr`, `release-on-merge`,
`simple-tag-and-release`), the same pattern as `rust-common` and `gtk-rs-skills`.

This page supersedes [RELEASING.md](../RELEASING.md), the manual procedure of the legacy `0.7.x`
Python application, which goes with the legacy tree in M9
([tooling-todo.md](tooling-todo.md#cleanup-when-the-rewrite-lands)).

## What a release produces

- A merged pull request `chore(release): bump version to X.Y.Z` with one commit, which changes the
  version in `Cargo.toml`, `Cargo.lock`, `meson.build` and the metainfo.
- A `vX.Y.Z` tag on the merge commit, created only after the gate passed.
- A GitHub release with the notes (the merged pull requests) and two Flatpak bundles,
  `gx52-x86_64.flatpak` and `gx52-aarch64.flatpak`, built from the merge commit. A `0.x` release
  is a pre-release.
- A pull request on [flathub/com.leinardi.gx52](https://github.com/flathub/com.leinardi.gx52)
  against a channel branch (`beta` or `master`), from the branch `release/<channel>/vX.Y.Z`. The
  maintainer merges it by hand; Flathub then builds and publishes.

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
- **Channels:** Flathub has two branches, `master` (stable) and `beta`. A `0.x` tag goes to `beta`
  only, so `0.99.x` cannot replace `0.7.6` on stable before `1.0.0` (rewrite analysis, section
  12.1); `flathub-pr.yaml` refuses a `0.x` tag to `master`. For `0.x`, `release-publish.yaml` opens
  the `beta` pull request itself, since it is the only valid channel. From `1.0.0` on it opens none:
  the maintainer dispatches `flathub-pr.yaml` with an explicit `channel` (no default), `beta` first
  for a release that goes through beta, `master` later to promote the same tag. A beta and a master
  pull request of one tag can coexist, as they come from different branches.

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
2026-10-28; `release-gate.yaml` and `flathub-pr.yaml` reuse them. The lessons S6 paid for:

- **Native arm runner.** The Flatpak build is a matrix of `x86_64` on `ubuntu-24.04` and `aarch64`
  on the free `ubuntu-24.04-arm` runner; no QEMU.
- **Container:** `ghcr.io/flathub-infra/flatpak-github-actions:gnome-51` with `--privileged`, and
  `flatpak/flatpak-github-actions/flatpak-builder`. The image ships the GNOME SDK, `uv` and the
  linters, but not the `rust-stable` SDK extension and no `flathub` remote.
- **`--disable-rofiles-fuse`** on every `flatpak-builder` call made outside the action.
- **Offline build:** the Flathub-style build uses only the crates listed in `cargo-sources.json`,
  generated from `Cargo.lock` by `make cargo-sources` with the pinned generator.

## Dispatching before M9

GitHub registers a `workflow_dispatch` workflow only from the default branch, and runs a schedule
only from it. Until the rewrite lands (M9) the default branch is the legacy `master`, while releases
are cut from `rust-rewrite`. So `master` carries stubs of `ci.yaml`, `release.yaml` and
`flathub-pr.yaml`, with the real files' dispatch inputs and a dispatch trigger only, and the real
`scheduled-scan.yaml`. A dispatch then runs the file of the ref it names:

```bash
gh workflow run release.yaml --ref rust-rewrite -f version=0.99.0 -f dry_run=false
gh workflow run release.yaml --ref m8/release -f version=0.99.0    # dry run, the default
gh workflow run flathub-pr.yaml --ref rust-rewrite -f tag=v1.0.0 -f channel=beta -f dry_run=false
```

A stub dispatched on `master` itself fails with "dispatch with --ref rust-rewrite". version-bump-pr
dispatches `ci.yaml` on `release/vX.Y.Z` the same way, which runs that branch's real `ci.yaml`. The
stubs have no push or pull-request trigger, so the legacy tree is never built. The stubs' inputs
are these; the `release_workflows` meta-test checks that the real files declare the same:

| Workflow | Input | Type | Required | Default |
| --- | --- | --- | --- | --- |
| `release.yaml` | `version` | string | no | `""` |
| `release.yaml` | `dry_run` | boolean | no | `true` |
| `flathub-pr.yaml` | `tag` | string | yes | none |
| `flathub-pr.yaml` | `channel` | choice `beta`, `master` | yes | none |
| `flathub-pr.yaml` | `dry_run` | boolean | no | `true` |

A change to these inputs is made on both branches. The trunk's meta-tests cannot read `master`, so
the stubs and `scheduled-scan.yaml` are checked by hand once the bootstrap is merged: `gh workflow
list` shows CI, Release, Flathub PR and Scheduled scan; a stub dispatched on `master` fails with its
message; `gh workflow run scheduled-scan.yaml` on `master` passes (or fails only in its
runtime job, naming a newer GNOME runtime).

At M9 the trunk becomes `main`, the default branch: the stubs go, `scheduled-scan.yaml` moves to
the trunk, and the `base_branch` inputs go
([tooling-todo.md](tooling-todo.md#cleanup-when-the-rewrite-lands)).

## How to release

1. **Dry run first**, from the trunk or a branch: `gh workflow run release.yaml --ref <branch> -f
   version=X.Y.Z`. Check the resolved version, the metainfo entry the bump job prints, the gate on
   both architectures and the Flathub diff.
2. **Dispatch the real run** on the trunk with `dry_run=false`. It opens the pull request
   `chore(release): bump version to X.Y.Z` from `release/vX.Y.Z` and dispatches CI on it.
3. **Merge it with a merge commit** once CI is green. Do not push to the branch or use "Update
   branch": to refresh it, dispatch the release again.
4. **The merge push runs `release-publish.yaml`:** release-on-merge checks that the push is the
   merge of exactly that one bump commit and waits for CI on the merge commit (up to 90 minutes),
   the gate builds and tests the merge commit, then simple-tag-and-release creates the tag by
   publishing the release with both bundles, and for `0.x` the beta Flathub pull request is
   opened.
5. **Merge the Flathub pull request** once Flathub's bot built it (it can be installed from the
   bot's test repository). Automating this merge was rejected (rewrite analysis, section 9.2).
6. **From `1.0.0` on**, dispatch `flathub-pr.yaml` with the tag and the channel.

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

## The Flathub pull request

`flathub-pr.yaml` replaces the channel branch's tree instead of patching it:

- every file goes except `README.md`: the legacy Python modules, `pypi-dependencies.json`, the
  `shared-modules` and `flatpak-builder-tools` submodules and the script that refreshed them;
- the manifest is the tagged tree's `build-aux/com.leinardi.gx52.json` put through
  `scripts/flathub-manifest.sh`: the app's `dir` source becomes `{"type": "git", "url":
  "https://github.com/leinardi/gx52.git", "tag", "commit"}`, and everything else (the
  `cargo-sources.json` entry, the inline `.cargo/config.toml` that vendors the crates, the runtime,
  the SDK extension, the finish-args) is the tagged manifest's, so the legacy `--share=network` and
  `--talk-name=org.freedesktop.Flatpak` go;
- `cargo-sources.json` is generated from the tagged `Cargo.lock` with the pinned generator;
- `flathub.json` becomes `{"only-arches": ["x86_64", "aarch64"]}`.

`flatpak-builder-lint manifest` checks the result, the full diff is printed, and the generated
manifest is built on `x86_64` and `aarch64` from the git tag, offline, as Flathub's bot will build
it. Only then does the `push` job, the one job with the Flathub token, push the commit to
`release/<channel>/vX.Y.Z` (under a lease, after checking that the channel has not moved) and open
the pull request against `<channel>`, or leave an open one as it is. When the channel already holds
exactly this change, nothing is built or pushed.

## Dry runs

`dry_run` is the default of both dispatch workflows, and a dry run may run from any ref.

- **`release.yaml`:** the policy job and version-bump-pr run exactly as in a real run: the version
  is resolved (`svu` included after `1.0.0`, so "nothing to bump" fails the same way), the bump
  command runs, and its patch is uploaded; nothing is pushed or opened. Then the gate runs on the
  dispatched commit with the patch applied, and `flathub-pr.yaml` runs in dry run on that same
  patched tree: its manifest, the `Cargo.lock` behind `cargo-sources.json` and its build all come
  from the bumped tree. The summary lists the version and both results.
- **`flathub-pr.yaml` alone:** the dispatched commit as it is, which checks the packaging of the
  current tree without a bump.

A Flathub dry run prints the full diff, removed files included, and builds the generated manifest
from a `dir` source in place of the tag (no tag exists yet). Before the `beta` branch exists, it
diffs against `master` and says so.

## Screenshots

The metainfo lists four screenshots under `data/screenshots/` (not installed), with translatable
captions. Their URLs follow the trunk
(`raw.githubusercontent.com/leinardi/gx52/rust-rewrite/data/screenshots/...`), not the tag: the
gate mirrors them before the tag exists. A side effect: a later screenshot change on the trunk also
changes what an older release shows. Off the trunk (a dry run of a pull request branch) the gate
points them at the dispatched commit instead, so a branch that adds or changes screenshots gates
its own. At M9 they move to `main`. The legacy `art/screenshot-*.png` stay until then.

The maintainer captures them from the Devel app with an X52 Pro connected, in the default window
size (16:9 or 4:3), the light style, no personal data, as PNG:

| File | Shows |
| --- | --- |
| `data/screenshots/leds.png` | the main window with a profile, on the LEDs page |
| `data/screenshots/mfd.png` | the MFD page: text and clocks |
| `data/screenshots/profiles.png` | the profile list |
| `data/screenshots/preferences.png` | Preferences |

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
| publish (tag, release, bundles) | Gate passed | Re-run; a draft or a published release is finished, never replaced |
| The Flathub pull request | GitHub release exists | Re-run, or dispatch `flathub-pr.yaml` with the tag; the branch is updated, the open pull request kept |
| Flathub's build bot | PR open | Fix in the Flathub PR if the fix is packaging-only; otherwise release the next patch version and close the PR |

A tag is created only after the gate and is never deleted or moved ("Immutable tags" ruleset), so
a version number is never reused for different code.

## One-time setup

- **No bypass identity.** The bump lands through a pull request like any other change, so nothing
  pushes to the protected trunk; tags are created by publishing the release with `GITHUB_TOKEN`.
- **Repository settings** (`gh-leinardi-iac`, `repo_gx52`): GitHub Actions may create pull
  requests (the bump PR); a ruleset on `rust-rewrite` until M9 (pull request, review, no deletion
  or force-push) with `ci-result` required; no required checks on the legacy `master`, whose
  `ci.yaml` stub never runs on a pull request; the `release` environment, deployable from
  `rust-rewrite` only.
- **Flathub token:** stored as the `FLATHUB_TOKEN` secret of the `release` environment, read only
  by `flathub-pr.yaml`'s push job. A fine-grained personal access token of the maintainer limited
  to `flathub/com.leinardi.gx52` ("Contents: read and write", "Pull requests: read and write"),
  if the `flathub` organisation accepts fine-grained tokens; otherwise a classic token with
  `public_repo`. Which one was possible is recorded here when it is created. Renewal date in the
  maintainer's calendar.
- **Flathub `beta` branch, before the first beta release:** it does not exist yet, and no workflow
  creates it. The maintainer creates it from `master` following Flathub's documented flow; it holds
  the legacy manifest until the first `flathub-pr.yaml` pull request migrates it. If that flow does
  not let a maintainer push a new branch, the flow it prescribes is followed and recorded here.
- **Private vulnerability reporting** on `leinardi/gx52`, as [SECURITY.md](../SECURITY.md) links to
  it: enabled by the IaC, checked once.
- **Dependabot:** production Cargo dependencies are committed as `fix(deps)` and so trigger a patch
  release after `1.0.0`; dev dependencies, actions and pre-commit hooks are `chore(deps)` (rewrite
  analysis, section 8). The GNOME runtime and the `rust-stable` extension have no Dependabot
  ecosystem: the weekly `scheduled-scan.yaml` fails when Flathub has a newer GNOME runtime than the
  manifests, and they are moved by hand.
