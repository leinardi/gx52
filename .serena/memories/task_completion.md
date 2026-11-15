# Task Completion

- Follow the validation matrix in `AGENTS.md` for the files touched; `make check` always applies.
- Rust: `make verify`, then `make check-stage`.
- `Cargo.toml`/`Cargo.lock`: also `make rust-deny` (network) and `make cargo-sources` if the lock
  changed.
- `.blp`, `meson.build`, `data/**`, `po/**`: `make meson-test`; `make meson-run` when visible.
- Markdown: `cargo test --locked -p meta --test docs`; wrap at 100 columns.
- Reviews: `adversarial-review` project skill, run in a fresh subagent.
- Before the final answer: `git status --short` and the diff, no unrelated changes mixed in.
