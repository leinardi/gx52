# Releasing

The short version. How it works, the channels, the one-time setup and the recovery table are in
[docs/release.md](docs/release.md).

Needs `gh` logged in as the maintainer (a collaborator on `flathub/com.leinardi.gx52`), their SSH
key on GitHub (the Flathub branches are pushed over SSH; `FLATHUB_URL=https://...` with `gh auth
setup-git` works too), plus `jq` and `curl`. Run everything from the trunk, `main`, up to
date.

## A release to beta

```bash
make release-dry-run VERSION=X.Y.Z   # optional: the dry run of the current branch
make release VERSION=X.Y.Z           # dispatch, bump PR and CI, [y/N] merge, publish, Flathub beta PR
make flathub-merge                   # once Flathub's test build passed: prints its install command, [y/N] merge
```

Before `1.0.0` the version is always given, and is a `0.99.x`.

## Stable (`1.0.0` or later)

```bash
make flathub-promote VERSION=X.Y.Z   # a release already tested on beta, to stable
make flathub-merge FLATHUB_BASE=master
```

Or straight to stable, or to both at once:

```bash
make release VERSION=X.Y.Z FLATHUB_BASE=master          # or FLATHUB_BASE="beta master"
make flathub-merge FLATHUB_BASE=master
```

A `0.x` release goes to beta only: `master` is refused for it.

## When something fails

Fix the cause, then run **the same `make release VERSION=X.Y.Z` again**. It reads from GitHub what
is already done and continues from there: it never dispatches, merges or publishes twice. A failed
or cancelled workflow run is offered one re-run of its failed jobs. If the bump pull request's CI
failed for a real bug, fix it on the trunk first: the next run refreshes the pull request onto it.

The Flathub step alone can be repeated with `make flathub-pr VERSION=X.Y.Z [FLATHUB_BASE=master]`:
it updates the open Flathub pull request instead of opening a second one.

`ASSUME_YES=1` answers every `[y/N]` with yes, `ASSUME_YES=0` with no.
