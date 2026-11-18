# Conventions

## Lints

- `[workspace.lints]`: clippy pedantic + `unwrap_used`, `expect_used`, `print_stdout`,
  `print_stderr`, `wildcard_enum_match_arm`, `min_ident_chars`, `allow_attributes`,
  `disallowed_methods`; `unsafe_code = "deny"`. Gates run `-D warnings`.
- Silence only with `#[expect(lint, reason = "...")]`; the reason says why the fix does not apply.
- Unit tests live in `<module>/tests.rs` next to `<module>.rs`.

## Headers

- GPL-3.0-or-later header (year of first commit) on every `.rs`, shell, Makefile, local `.mk`.
- Reused file: provenance line below header (`Adapted from <repo>/<path> at <commit>, originally
  <MIT|Apache-2.0>-licensed, Copyright (c) <year> Roberto Leinardi`), never the original block.
- Exempt: make-common files, Rust fixtures under `testdata/`.

## UI and i18n

- Every user-visible string translatable; new translatable file goes in `po/POTFILES.in`
  (meta-test checks). Every interactive widget has an accessible label.
- New `.blp`: Meson blueprint list + gresource XML + `POTFILES.in`.
- App ID `com.leinardi.gx52`; Devel build `com.leinardi.gx52.Devel` (`-Dprofile=development`).

## Makefile

- `.mk/{help,pre-commit}.mk` and `scripts/bootstrap-mk-common.sh` come from make-common: never
  hand-edit. Repo targets in own `.mk/<fragment>.mk`, listed in `MK_LOCAL_FILES`.

## Commits and branches

- Conventional Commits with scope, enforced by `commit-msg` hook. PRs to `main`, merge
  commits, so every commit's type counts. Type by whether the change ships: shipped change is
  `fix`/`feat`. 0.99.x versions are explicit; svu from 1.0.0.
- Removing a user-visible surface needs a line in `README.md` or `docs/**` retiring it.
