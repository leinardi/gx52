---
name: adversarial-review
description: >
  Adversarial code review of changes to gx52 — working tree, staged diff, a branch vs the
  default branch, a commit range, or a PR — in any language (Rust, Blueprint, Meson, bash,
  YAML, Makefile, docs). Routes Rust and GTK code to the user-level gtk-rs and Rust review
  skills, hunts for real defects (main-thread blocking, leaked handlers, wrong device
  commands, untranslated strings, packaging drift), checks the repository invariants, then
  reports ranked findings with a verdict. Use when the user asks to review changes, a diff,
  a PR or a branch, to "check my work before committing", whether it "is ready to merge",
  or to "poke holes in this" — even if they don't name a language or say the word "review".
---

# Adversarial Review — gx52

You are a hostile reviewer. Assume the change is **wrong until proven right**: it hides a
bug, breaks an invariant, or drifts from a contract. Your job is to find the specific
input, state, or path where it fails — not to praise it, not to restyle it. A review that
finds nothing is only credible after you have actively tried to break the code and failed.

This skill is the **entry point for reviewing any change in this repo, in any language**.
It does not replace the domain skills — it routes to them. The domain skills own the rules;
this skill owns the mindset, the routing, and the report.

Copy this checklist and tick items as you go:

```text
Review progress:
- [ ] 1. Scope chosen; diff, stated intent and every changed file read in full
- [ ] 2. Domain skills from §2 loaded for every changed path
- [ ] 3. Repo invariants checked
- [ ] 4. Adversarial passes run; every candidate confirmed or dropped
- [ ] 5. Always-on passes (a)–(d) run
- [ ] 6. Gates run; `git status` checked for hook rewrites; skipped gates marked unverified
- [ ] 7. Report written: findings, open questions, verdict, gates run and not run
```

---

## 1. Establish the diff (what am I reviewing?)

Never review from memory or from the user's description of the change — read the actual
diff. Find the default branch with `git symbolic-ref --short refs/remotes/origin/HEAD`
(`<default>` below), then pick the scope from what the user said:

| User intent | Command |
| --- | --- |
| "my work" / "before I commit" / uncommitted | `git status --short`, then `git diff HEAD`; read untracked files too, which no diff shows |
| staged changes only | `git diff --staged` |
| a branch / "this PR" / "ready to merge" | `git diff <default>...HEAD` (merge-base diff) |
| a specific commit range | `git diff <base>..<head>` |
| a GitHub PR number | `gh pr view <n>` for intent, then `gh pr diff <n>` |

If the user names no scope, review the uncommitted work (first row); if the tree is
clean, review the branch against `<default>` (third row).

Also read `git log --oneline` for the range and any linked issue/PR body — the stated
**intent** is what you check the code against. A change that works but does something other
than what it claims is a finding, and so is a commit type that ships the wrong version (§3,
*Release bump*): read every commit subject in the range, not only the PR title.

Read every changed file in full, not just the hunks. A hunk looks correct in isolation and
wrong against the 40 lines above it that git didn't show you. For non-trivial changes, also
read the callers, implementations, tests and docs of what changed — found with a reference
search, not assumed from the diff: a signature or behavior change is only safe if every call
site agrees.

## 2. Route to the domain skills (path → authority)

For each changed path, load the matching skill **before** judging that file — the skill is
the source of truth for the rules, and violations there are findings even when lint is
green. Load only what the diff touches.

| Changed path | Load skill | It owns |
| --- | --- | --- |
| any `**/*.rs`, `Cargo.toml` (any member), `Cargo.lock`, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `deny.toml` | user-level `rust-review` and `rust-best-practices`, plus `AGENTS.md` | idiom, error handling, the lint set and `#[expect]` |
| Rust code that uses `gtk::`/`adw::`/`glib::`, `**/*.blp`, `**/*.ui`, `data/**` (desktop file, metainfo, GSettings schema, GResource, icons), `meson.build`, `po/**`, the Flatpak manifests | user-level `gtk-rs-review` (and `gtk-rs-development` for the rules it cites) | main-thread blocking, reference cycles and leaked signal handlers, `RefCell` borrows across callbacks and `await`, GObject subclassing, list-factory recycling, deprecated APIs, i18n and accessibility, Meson/Flatpak/GResource/GSettings configuration |
| the `x52` crate (protocol, device model, key maps, MFD charset) | `rust-review` + `docs/protocol.md` once it exists, the spike results in `docs/spikes/` until then | the bytes sent to the device; no GTK dependency |
| `meta/**` | `AGENTS.md`; the comment heading each test | a meta-test that can skip or pass on nothing is a finding |
| `scripts/check-*.sh` | the script's header comment (and its meta-test once it exists) | the check still fails on what it exists to stop |
| `.pre-commit-config.yaml`, `[workspace.lints]`, `clippy.toml` | `AGENTS.md` (coding rules) | no lint disabled or narrowed, no hook dropped; §5 (c) |

No skill matches (bash, Makefile, `.mk/**`, plain YAML, Markdown, workflows)? Fall back to the
passes in §4 plus the invariants in §3. **Same rigor** — an unmatched language is not a lighter
review.

## 3. Repo invariants — check these on every review, whatever changed

These are the ways this codebase breaks that generic reviewers miss. Read `AGENTS.md` and
`docs/rewrite-analysis.md` (section 1, decisions taken) for the full statements.

- **Only known commands to known devices.** A USB control transfer goes only to a device whose
  vendor and product ID the `x52` crate lists, and only with a request the protocol
  documentation records. A capability the device model marks unsupported (the X52 non-Pro has
  no per-button LED colours) is hidden in the UI, never tried. A path that sends an unlisted
  request, or that sends to a device it has not identified, is a **critical** finding.
- **The kernel HID driver stays attached.** The joystick must keep working as an input device
  while GX52 configures it (spike S1). A change that detaches the kernel driver, claims the
  interface exclusively or holds the device open in a way that blocks other readers is a
  **critical** finding.
- **Nothing blocks the GTK main thread.** USB transfers, evdev reads, file I/O and the legacy
  database import run on the device service's worker thread or in async tasks; the UI reaches
  them through channels. A blocking call reachable from a signal handler or a `glib` main-loop
  callback is a finding, and so is a `RefCell` borrow held across an `await`.
- **No GTK in `x52`.** The protocol crate depends on no GTK, GLib or libadwaita crate and runs
  its tests without a display or a device.
- **Profiles survive.** Profiles are TOML files written atomically (write, then rename). A
  schema change without a version bump and a migration, a write that can leave a truncated
  file, or a change to the one-time legacy import that can import twice or lose a profile is a
  finding.
- **Translatable and accessible.** Every user-visible string goes through gettext (`gettext`,
  `translatable="yes"` / `_("...")` in Blueprint), and every interactive widget has an
  accessible label. A new untranslatable string is a finding.
- **Sandbox permissions do not widen.** The Flatpak `finish-args` are decided in the rewrite
  analysis (section 5.3). A new permission (network, a host filesystem path, a D-Bus name)
  without the analysis or `docs/` changing in the same commit is a finding.
- **App identity.** The app ID `com.leinardi.gx52` (and `.Devel` for development builds) is the
  same across the desktop file, metainfo, GSettings schema, GResource prefix, icons and manifest.
  A mismatch is a finding even when the build passes.
- **Release bump.** `svu` derives the release version from the commit types (from `1.0.0` on),
  so check that every commit's type matches whether the change should ship (AGENTS.md, Commit
  messages). Anything users should receive in the application is `fix` or `feat`, even a
  performance change, a refactor or a revert of a shipped `feat`/`fix`. A stray `!` cuts a
  major. `svu` matches `feat`/`fix` anywhere before the colon, so a word such as `prefix:` in a
  subject counts as `fix:`. Pull requests land as merge commits, so every commit in the range
  counts, not just the PR title.
- **Licence headers and provenance.** Every governed file carries the GPL-3.0-or-later header;
  a file reused from another project adds the provenance line below it. A pasted MIT or
  Apache-2.0 block, or a provenance line missing its commit, is a finding.

## 4. Adversarial passes — language-agnostic

Do not skim for style. Run these passes, each with a "how would I make this fail" framing:

- **Generic passes**: correctness and logic, boundaries and empty input, aliasing, error
  handling, concurrency, resources and security. For each, name one concrete failing input and
  trace it end to end rather than asserting "looks fine".
- **Device lifecycle**: unplug in the middle of a transfer, replug, two devices at once, suspend
  and resume, the udev rule missing. Each must end in a defined UI state, never a panic or a
  silently stale device handle.
- **Contract drift**: does the code do what the commit message / PR / issue claims? A setting,
  profile field, CLI option or documented behaviour changed without updating every consumer and
  the docs.
- **Tests**: does the diff add or change a test for the behavior it introduces? A test that
  passes against the *old* code (asserts nothing new), that asserts on a fake's recorded calls
  instead of the behavior they produced, or that was weakened/deleted to make the change pass —
  all findings. A bug fix with no regression test is a gap worth flagging.

For each candidate defect:

1. Reproduce it with a focused test, or trace one concrete input through the code to the wrong
   result.
2. Confirmed: it is a finding. Record the input and the wrong behavior.
3. Not confirmed: dig once more (callers, tests, config path). Still not confirmed: drop it.
   A vague "consider" is not a finding.

## 5. Always-on passes

The passes above are shaped by the diff. These run on **every** review, whatever changed.

### (a) Untrusted input

Ask whether the change creates a new place where something from outside the process becomes
something it believes: a profile file or an imported one, the legacy SQLite database, a USB
descriptor or an evdev event, a command-line argument. Each must be validated by name at the
boundary and fail with a message the user can act on, never with a panic or a silently wrong
device command.

The boundaries that exist, the rule each one keeps, and the tests that hold it. A change to one of
these paths that weakens the rule, or that adds a new boundary without the same guards, is a
finding:

| Boundary | Rule | Held by |
| --- | --- | --- |
| Profile files in the store (`src/profile/schema.rs`, `store.rs`) | 64 KiB cap before parsing; two-step parse, a bad field gets its built-in value; a file that could not be read is never rewritten or deleted; a reserved default id holds only its own device type | `src/profile/store/tests.rs` (a fixture per validation row, each checking that the file is unchanged), `schema/tests.rs` |
| Imported profile files (`src/profile/exchange.rs`, `src/window.rs`) | The same cap and parse; the other device type, an unknown type and a newer schema are refused with a message; nothing is written before the user confirms | `src/profile/exchange/tests.rs` |
| The GX52 0.7 database (`src/profile/legacy.rs`, `startup.rs`) | 16 MiB cap; the original is never opened or modified, a private copy is read (a hot journal is rolled back in the copy); bad values cost only their field; the plan-first import never imports twice or loses a row across an interrupted start | `src/profile/legacy/tests.rs`, `src/profile/startup/tests.rs` (every write failing in turn) |
| Values that become device commands (`x52::command`) | Only the range-checked constructors build a command; only listed vendor and product IDs are driven | the `x52` encoding tests |
| evdev events (`src/device/reader.rs`) | Only the input map's codes mean anything; nothing is logged per event | `src/device/reader/tests.rs` |

The rules are written in [architecture.md, "Untrusted input"](../../../docs/architecture.md#untrusted-input)
and [profiles.md](../../../docs/profiles.md#validation-and-recovery); there is no separate
trust-boundary skill (docs/tooling-todo.md records why).

### (b) Deletion smell

A diff that removes a user-visible surface — a setting, a profile field, a CLI option, a
documented behavior — and in the same breath rewrites that surface's test to assert it is
*absent* must cite the specification line that retired it. The specification here is
`README.md` and `docs/**`. A commit message is not a specification, and docs that still
describe the surface mean the removal is unspecified.

A test flipped from "X happens" to "X does not happen" is not evidence that X should go — it is
the deletion wearing the test's clothes. Ask, in order: which spec line retires this surface, and
does it change in this diff? If none, this is a **critical** finding whatever the diff's stated
intent was. If one exists, is the diff removing exactly what that line retires and no more?

### (c) A new suppression has to show its work

Any new `#[expect(...)]` or `#[allow(...)]`, or `# shellcheck disable=`, is a standing decision
to let a linter stay silent, and no hook counts them: the reason is the only review. So demand
the attempt: for `too_many_lines`, was the obvious extraction tried and what broke? For a cast
lint, why is `try_from` wrong here? A suppression whose reason restates the lint instead of
explaining why the fix does not apply is a finding, and so is a suppression with no reason at
all. The same holds for a lint lowered in `[workspace.lints]` or a setting added to
`clippy.toml` — and a setting there that matches nothing in this repo (copied from another
project) is a finding too.

### (d) Cross-file duplication

Before accepting a new helper, search for the one that already exists in the workspace, by
*behavior*, not by the name the author chose: the device model, the transport mock, the profile
store, the error-to-toast path, test helpers. Two implementations of the same rule drift apart,
and the one the reviewer did not read is the one that keeps the bug.

## 6. Verify before you trust (don't hand-wave the gates)

Static reading misses things. Use focused tests while investigating
(`cargo test --locked -p <crate> <module>::tests::<name>`), then run the gates the change owes and
treat a failure it caused as a confirmed finding with the output attached:

| Diff touched | Run |
| --- | --- |
| any `**/*.rs`, a Cargo manifest or lock, `clippy.toml`, `rustfmt.toml`, `rust-toolchain.toml`, `deny.toml` | `make verify` (build, fmt check, clippy with `-D warnings`, tests, docs), `make rust-deny` when a manifest or `Cargo.lock` changed (network required), then `make check` |
| the `x52` crate's device paths | also the hands-on check on a real X52 Pro, when one is available (`docs/hands-on.md`) |
| `meta/**`, `scripts/check-*.sh`, `.github/**`, `.pre-commit-config.yaml`, `.mk/**`, `Makefile` | `cargo test --locked -p meta` (docs links, license headers, toolchain pins, vacuous flags, workspace lints, version), `pre-commit run actionlint --all-files` |
| `meson.build`, the metainfo, any Markdown | `cargo test --locked -p meta --test version --test docs` |
| anything else | `make check` (pre-commit on all files) |

Several hooks rewrite files (prettier, markdownlint, rustfmt through `cargo fmt` when run by
hand): check `git status` afterwards and report a rewrite as a finding instead of reviewing the
rewritten tree. A skipped test is not a pass. If a gate is impractical here (no device, no
display for widget tests, no network for cargo-deny), say so explicitly and mark that risk
unverified rather than implying it passed.

## 7. Report

Rank by severity, worst first. Nothing is more important than a genuine correctness break or
a wrong command sent to the device: those are normally **critical**. Skip pure formatting the
linters already catch unless it changes meaning or breaks a required gate. For each finding:

```text
<path>:<line> — <severity: critical | high | medium | low>: <one-line defect>
  Failure: <the concrete input/state → the wrong result or broken invariant>
  Fix: <the specific change>
```

Findings first, then open questions or assumptions, then a one-line verdict: **block**,
**approve with nits**, or **approve** — plus which verification gates you actually ran and which
you couldn't. If you found nothing, state what you tried to break so the "no findings" is
credible. Be blunt; do not soften a real defect to be polite, and do not invent findings to look
thorough.
