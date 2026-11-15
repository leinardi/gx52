# Suggested Commands

- `make help` lists all targets.
- `make verify`: rust build, fmt check, clippy, tests (meta included), rustdoc, meson test.
- `make check` (all files) / `make check-stage` (staged): pre-commit, the authoritative gate.
- `make pre-commit-install`: pre-commit and commit-msg hooks.
- `make rust-fmt`, `make rust-clippy`, `make rust-test`; single meta test:
  `cargo test --locked -p meta --test <name>` (e.g. `docs`, `license_headers`, `version`).
- `make meson-setup` (once), `make meson-compile`, `make meson-test`, `make meson-run`.
- `make flatpak-install` then `flatpak run com.leinardi.gx52.Devel`; `make flatpak-lint-manifest`.
- Network: `make rust-deny`, `make audit-deps`, `make cargo-sources`.
