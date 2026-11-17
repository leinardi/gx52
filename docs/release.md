# Release

How a GX52 release is versioned, built and published to Flathub, what to do when one fails halfway,
and what has to be set up by hand. The decisions are in [rewrite-analysis.md](rewrite-analysis.md)
sections 8 and 9; what the build and CI jobs were proven to do is in [spike
S6](spikes/06-build-and-release.md). The pipeline is four workflows here, driven end to end by
`make release` on the maintainer's machine, which also opens the Flathub pull request:

| Workflow | Runs on | Does |
| --- | --- | --- |
| `.github/workflows/release.yaml` | dispatch | the version policy, then the bump pull request; a dry run gates the bumped tree |
| `.github/workflows/release-publish.yaml` | push of the version files to the trunk | decides that the push is a release, gates it, checks the Flathub files, tags and publishes it |
| `.github/workflows/release-gate.yaml` | called by both | the Flathub-style build, `meson test` and the linters on both architectures |
| `.github/workflows/flathub-check.yaml` | called by both | the Flathub manifest of the tag and `cargo-sources.json`, linted and diffed; pushes nothing |

`scripts/release.sh` (`make release`) and `scripts/flathub-release.sh` (the `flathub-*` targets)
run with the maintainer's own `gh` and git logins ([How to release](#how-to-release), [The Flathub
pull requests](#the-flathub-pull-requests)): no token is stored anywhere.

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
- A pull request on [flathub/com.leinardi.gx52](https://github.com/flathub/com.leinardi.gx52) into
  each Flathub branch the release goes to ([Versions and channels](#versions-and-channels)), from
  the branch `gx52/vX.Y.Z-<branch>`, whose tree is the release's two Flathub assets and
  `flathub.json`. `make release` opens it; the maintainer merges it with `make flathub-merge` once
  Flathub's bot built it, and Flathub then builds and publishes.

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
- **Channels:** Flathub has two branches, `master` (stable) and `beta`. The release picks them:
  `make release` opens the Flathub pull request into `FLATHUB_BASE`, `beta` by default, `master`,
  or `"beta master"` (rewrite analysis, section 9.4). A `0.x` release goes to `beta` only, so
  `0.99.x` cannot replace `0.7.6` on stable (rewrite analysis, section 12.1); `make release` and
  `make flathub-promote` refuse `master` for it.
- **Beta first, or straight to stable:** from `1.0.0` on, a release goes to `beta` and is promoted
  later with `make flathub-promote VERSION=X.Y.Z`, which opens the `master` pull request from the
  same release, so stable gets exactly the build that was tested on beta. A release can also go to
  `master` directly (`FLATHUB_BASE=master`), or to both at once. Holding a release back on beta
  means not promoting it; nothing is opened on `master` until then.

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

```bash
make release-dry-run VERSION=X.Y.Z                  # optional: the dry run of the current branch
make release VERSION=X.Y.Z                          # beta (FLATHUB_BASE=master or "beta master")
make flathub-merge                                  # after Flathub's test build of the beta PR
make flathub-promote VERSION=X.Y.Z                  # 1.0.0 or later: the same release to stable
make flathub-merge FLATHUB_BASE=master
```

`make release` (`scripts/release.sh`) drives the whole release with the maintainer's `gh` login.
Before every step it reads back from GitHub what is already done, so **a run that stopped is
resumed by running the same command again** once the cause is fixed; it never dispatches, merges
or publishes twice:

1. **Dispatch** `release.yaml` on the trunk with `dry_run=false` and wait for it: the policy job,
   then version-bump-pr, which opens the pull request `chore(release): bump version to X.Y.Z` from
   `release/vX.Y.Z` and dispatches CI on it. A failed run is offered one re-run of its failed jobs.
2. **The bump pull request:** if the trunk moved on since it was opened (a fix landed), the release
   is dispatched again, which refreshes it onto the new trunk. Then its CI run is awaited (and
   dispatched if it is missing; a failed run is offered one re-run), its diff is shown, and on
   confirmation it is approved and merged with a merge commit. Do not push to the branch or use
   "Update branch".
3. **The merge push runs `release-publish.yaml`:** release-on-merge checks that the push is the
   merge of exactly that one bump commit and waits for CI on the merge commit (up to 90 minutes),
   the gate builds and tests the merge commit and the Flathub check writes and lints the Flathub
   files, then simple-tag-and-release creates the tag by publishing the release with its four
   assets. `make release` waits for it; a failed or cancelled run is offered one re-run.
4. **The Flathub pull request** into each `FLATHUB_BASE` branch is opened, or updated if one is
   open ([The Flathub pull requests](#the-flathub-pull-requests)).
5. **`make flathub-merge`** waits for Flathub's bot to build the open pull request, prints the
   command that installs its test build, and merges it on confirmation. Automating this merge
   without the maintainer was rejected (rewrite analysis, section 9.2).

Every prompt is a `[y/N]` on the terminal; `ASSUME_YES=1` answers yes and `ASSUME_YES=0` no. The
workflows can still be dispatched by hand (`gh workflow run release.yaml --ref rust-rewrite -f
version=X.Y.Z -f dry_run=false`), and `make release` picks up from whatever state that leaves.

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
  "https://github.com/leinardi/gx52.git", "tag", "commit"}`, with the merge commit the tag will
  name, and
  everything else (the `cargo-sources.json` entry, the inline `.cargo/config.toml` that vendors the
  crates, the runtime, the SDK extension, the finish-args) is the tree's manifest's;
- `cargo-sources.json` is generated from the tree's `Cargo.lock` with the pinned generator.

`flatpak-builder-lint manifest` checks the manifest, and its diff against the Flathub branch is
printed for review. When the two differ in more than the app source's tag and commit (a runtime
bump, a new finish-arg), the run summary says so, as the Flathub pull request will carry that change
too and is worth a closer look before it is merged. Before the tag, the release also
checks that its merged assets are exactly the four files. Both files become release assets. The
check builds nothing of its own: before the tag exists the manifest can only be built from a `dir`
source, which is exactly what the gate builds. It pushes nothing and reaches no secret.

## The Flathub pull requests

`scripts/flathub-release.sh` does what the maintainer pushes to `flathub/com.leinardi.gx52`, with
their own logins: it keeps a clone in `_flathub/com.leinardi.gx52` (ignored, skipped by the
manifests' `dir` source, cloned on first use; `FLATHUB_DIR` moves it), builds a branch there,
pushes it with git, and opens its pull request with `gh` (or updates the one already open).
Nothing is downloaded by hand. `VERSION` defaults to the version the tree carries, which after a
release's merge is the released one.

| Target | Does |
| --- | --- |
| `make flathub-pr [FLATHUB_BASE=master]` | the pull request of `VERSION` into `beta` (or `master`), from `gx52/vX.Y.Z-<branch>`; `make release` runs it |
| `make flathub-promote` | the same into `master`, for `1.0.0` or later: the promotion to stable |
| `make flathub-merge [FLATHUB_BASE=master]` | waits for the checks of the open `gx52/*` pull request (Flathub's test build), prints the test build's install command, merges on confirmation |
| `make flathub-beta` | once: creates `beta` from the passed test build of `gx52/vX.Y.Z-master`, and closes that pull request |

A `flathub-pr` branch is one commit on the base whose tree is exactly the release: the release's
`com.leinardi.gx52.json` and `cargo-sources.json` assets, `build-aux/flathub/flathub.json`, and
the base's `README.md`. Everything else goes, which is what migrates the legacy tree (the Python
modules, `pypi-dependencies.json`, the submodules), and a runtime bump or a finish-args change
reaches Flathub the same way as a new tag. The assets are downloaded and checked before the clone
is touched: the manifest must name the tag and the commit the tag points at, so a refused release
leaves the clone as it was. A re-run rebuilds the branch from the base and force-pushes it with a
lease, and updates the title and body of its open pull request instead of opening a second one; a
release the base already has changes nothing and pushes nothing. The script never pushes to
`master`, and pushes `beta` only to create it: after that first push Flathub protects it.

`flathub.json` keeps `disable-external-data-checker`: Flathub's own checker would otherwise open
pull requests of its own on `master`.

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

The recovery path is **`make release VERSION=X.Y.Z` again**, unchanged, once the cause is fixed: it
resumes from the state GitHub shows, and offers a re-run of a failed run's jobs, which is the right
answer to a transient failure. Every reusable workflow finishes what exists and fails with "needs a
human" on anything that disagrees with it. By hand, the same is "Re-run failed jobs" on the run.

| Failed at | State | What to do |
| --- | --- | --- |
| `release.yaml` policy or bump | Nothing changed | Fix the input or the history, dispatch again |
| The bump pull request's CI | PR open, nothing released | Fix on the trunk, then dispatch again: the PR is refreshed onto the new trunk |
| `release-publish.yaml` cancelled while pending | Bump merged, nothing tagged | A third push of a version file replaced it in release-on-merge's group: "Re-run all jobs" on it |
| `release-publish.yaml` decide | Bump merged, nothing tagged | A transient failure (API, CI timeout): re-run. A refusal (not the one bump commit): revert the change and dispatch a release |
| The gate | Bump merged, **no tag** | A transient failure (runner, network): re-run. If the code is wrong, fix it on the trunk and release the next patch version; the next bump drops the never-published `<release>` entry |
| The Flathub check | Bump merged, **no tag** | As the gate: re-run a transient failure, release the next patch version for a wrong manifest |
| publish (tag, release, assets) | Gate and check passed | Re-run; a draft or a published release is finished, never replaced |
| The Flathub pull request | GitHub release exists | `make release` again (or `make flathub-pr`): the branch is rebuilt and its open pull request updated, not duplicated |
| Flathub's build bot | Flathub PR open | A packaging-only fix lands here and ships with the next release; otherwise release the next patch version, whose Flathub pull request replaces this one (close it) |

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
- **No Flathub token.** The Flathub pull requests are opened with the maintainer's own `gh` login,
  a collaborator on `flathub/com.leinardi.gx52` ([The Flathub pull
  requests](#the-flathub-pull-requests)). A fine-grained personal access token cannot write to a
  public repository of an organisation its owner is not a member of, and a classic one's
  `public_repo` scope would reach every public repository the account can write to.
- **Flathub `beta` branch and migration pull request, before the first beta release:** `beta`
  does not exist yet, and no workflow creates it. The maintainer creates it after `v0.99.0` is
  released, following Flathub's documented flow (a pull request against `master` for the test
  build, then `beta` created from `master` with the branch merged in): `make flathub-pr
  VERSION=0.99.0 FLATHUB_BASE=master` opens the pull request, and once the bot's test build
  passes, `make flathub-beta VERSION=0.99.0` creates `beta` and closes it. The branch migrates
  the tree ([The Flathub pull requests](#the-flathub-pull-requests)): `flathub.json` becomes
  `{"only-arches": ["x86_64", "aarch64"], "disable-external-data-checker": true}` and every file
  but `README.md` is replaced (the Python modules, `pypi-dependencies.json`, the `shared-modules`
  and `flatpak-builder-tools` submodules and the script that refreshed them go). If the flow does
  not let a maintainer push a new branch, the flow it prescribes is followed and recorded here.
- **`1.0.0` to `master`:** `master` keeps the legacy manifest until `1.0.0`. `make release
  VERSION=1.0.0 FLATHUB_BASE=master` (or `"beta master"`) opens the pull request that replaces it,
  with the same tree as the `beta` migration; it is merged with `make flathub-merge
  FLATHUB_BASE=master`.
- **Private vulnerability reporting** on `leinardi/gx52`, as [SECURITY.md](../SECURITY.md) links to
  it: enabled by the IaC, checked once.
- **Dependabot:** production Cargo dependencies are committed as `fix(deps)` and so trigger a patch
  release after `1.0.0`; dev dependencies, actions and pre-commit hooks are `chore(deps)` (rewrite
  analysis, section 8). The GNOME runtime and the `rust-stable` extension have no Dependabot
  ecosystem: the weekly `scheduled-scan.yaml` fails when Flathub has a newer GNOME runtime than the
  manifests, and they are moved by hand.
