# Releasing

The short version. How it works, the channels, the one-time setup and the recovery table are in
[docs/release.md](docs/release.md).

Needs `gh` logged in as the maintainer (a collaborator on `flathub/com.leinardi.gx52`), their SSH
key on GitHub (the Flathub branches are pushed over SSH; `FLATHUB_URL=https://...` with `gh auth
setup-git` works too), plus `jq` and `curl`. Run everything from the trunk, `main`, up to
date.

## A release to beta

```bash
make release-dry-run                 # optional: the dry run of the current branch
make release                         # dispatch, bump PR, [y/N] merge, publish, Flathub beta PR
make flathub-merge                   # once Flathub's test build passed: prints its install command, [y/N] merge
```

With no `VERSION`, the Release workflow derives it with `svu` from the Conventional Commits since
the last release, and `make release` reads it back from the bump pull request it opens; check it
in the diff before you confirm the merge. `VERSION=X.Y.Z` on either command gives it explicitly.
Before `1.0.0` the version is always given, and is a `0.99.x`.

## Stable (`1.0.0` or later)

To beta and stable at once:

```bash
make release FLATHUB_BASE='beta master'   # or FLATHUB_BASE=master: stable only
make flathub-merge                        # the beta PR, once its test build passed
make flathub-merge FLATHUB_BASE=master    # the master PR, likewise
```

Or a release already tested on beta, promoted to stable:

```bash
make flathub-promote VERSION=X.Y.Z
make flathub-merge FLATHUB_BASE=master
```

A `0.x` release goes to beta only: `master` is refused for it.

## When something fails

Fix the cause, then run **the same `make release` command again**. It reads from GitHub what is
already done and continues from there: it never dispatches, merges or publishes twice. Without
`VERSION` it resumes the open bump pull request, or a merged one whose release is not published
yet. Once the release is published, a run without `VERSION` would start the next release, so a
failed Flathub step is resumed with the command the run printed, `make release VERSION=X.Y.Z
FLATHUB_BASE=...`. A failed or cancelled workflow run is offered one re-run of its failed jobs.
The bump pull request's CI is shown, not waited for: `release-publish.yaml` runs CI on the merge
commit before it tags. To fix a real bug first, decline the merge and fix it on the trunk: the
next run refreshes the pull request onto it.

The Flathub step alone can be repeated with `make flathub-pr VERSION=X.Y.Z
[FLATHUB_BASE='beta master']` (or `master` alone): it updates the open Flathub pull request
instead of opening a second one.

`ASSUME_YES=1` answers every `[y/N]` with yes, `ASSUME_YES=0` with no.
