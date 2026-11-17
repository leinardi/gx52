// Copyright 2026 Roberto Leinardi.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// Adapted from swarm-device-access/meta/tests/vacuous_flags.rs at 4cd31c4, originally
// Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi

//! No gate carries a flag that makes it pass on nothing: the Makefile and its fragments, the
//! pre-commit configuration and every YAML file under .github. A reviewed exception is an exact
//! fragment, removed from the line before the flags are matched, so a second `|| true` on an
//! allowed line is still caught.

use std::sync::LazyLock;

use meta::repo;
use regex::Regex;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("invalid pattern {pattern}: {error}"))
}

/// A flag that turns a gate with nothing to check, or a failing one, into a pass.
struct VacuousFlag {
    name: &'static str,
    pattern: Regex,
}

/// The flags none of the files that define gates may carry, outside [`ALLOWED_FRAGMENTS`]. The
/// Rust gates: a test run that only compiles, lints capped or allowed wholesale, tests skipped by
/// name, or a workspace member left out of a workspace-wide command. A cargo call is cargo itself
/// or make's `$(CARGO)`.
static VACUOUS_FLAGS: LazyLock<Vec<VacuousFlag>> = LazyLock::new(|| {
    [
        ("|| true", r"\|\|\s*(true\b|:(\s|;|$)|exit\s+0\b)"),
        ("make's - recipe prefix", r"^\t[@+]*-"),
        (".IGNORE", r"^\.IGNORE\b"),
        (
            "make --ignore-errors",
            r"\$\(MAKE\).*\s(-[a-zA-Z]*i[a-zA-Z]*|--ignore-errors)\b",
        ),
        ("continue-on-error", r"continue-on-error"),
        (
            "if: false",
            r#"\bif:\s*(["']?false["']?\s*$|["']?\$\{\{\s*false\s*\}\}["']?)"#,
        ),
        (
            "cargo test --no-run",
            r"(?i)\bcargo\)?\s.*\btest\b.*\s--no-run\b",
        ),
        ("--cap-lints", r"--cap-lints\b"),
        ("warnings allowed", r"(\s-A\s*|--allow[ =])warnings\b"),
        ("cargo --skip", r"(?i)\bcargo\)?\s.*\s--skip\b"),
        ("cargo --exclude", r"(?i)\bcargo\)?\s.*\s--exclude\b"),
    ]
    .into_iter()
    .map(|(name, pattern)| VacuousFlag {
        name,
        pattern: regex(pattern),
    })
    .collect()
});

/// A clippy run in a gate (cargo itself or make's `$(CARGO)`), which must turn warnings into
/// errors.
static CLIPPY_CALL: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)\bcargo\)?\s+(\+\S+\s+)?clippy\b"));
/// A YAML `name:` line, a hook's or a step's label rather than a command it runs.
static YAML_NAME: LazyLock<Regex> = LazyLock::new(|| regex(r"^\s*(-\s+)?name:\s"));
/// The `-D warnings` a clippy gate needs: without it every finding is a warning, and the gate
/// passes whatever clippy reports.
static CLIPPY_DENIES_WARNINGS: LazyLock<Regex> =
    LazyLock::new(|| regex(r"\s(-D\s*|--deny[ =])warnings\b"));

/// One exact piece of shell, in one file, whose `|| true` is not a gate passing on nothing; the
/// reason says why. Each one tolerates a command whose failure is an answer, not an error.
struct AllowedFragment {
    file: &'static str,
    fragment: &'static str,
    reason: &'static str,
}

/// The repository's reviewed exceptions: none yet.
const ALLOWED_FRAGMENTS: &[AllowedFragment] = &[];

/// One flag found on one line.
#[derive(Debug)]
struct VacuousHit {
    /// The line, counted from 1.
    number: usize,
    flag: &'static str,
    line: String,
}

/// The flags on `content`'s lines, after removing the fragments of `allowlist` allowed in `file`;
/// `used`, as long as `allowlist`, records which entries matched.
fn vacuous_hits(
    file: &str,
    content: &str,
    allowlist: &[AllowedFragment],
    used: &mut [bool],
) -> Vec<VacuousHit> {
    let mut hits = Vec::new();
    for (index, original) in content.split('\n').enumerate() {
        let mut line = original.to_owned();
        for (allowed_index, allowed) in allowlist.iter().enumerate() {
            if allowed.file == file && line.contains(allowed.fragment) {
                line = line.replace(allowed.fragment, "");
                used[allowed_index] = true;
            }
        }
        if CLIPPY_CALL.is_match(&line)
            && !CLIPPY_DENIES_WARNINGS.is_match(&line)
            && !YAML_NAME.is_match(&line)
        {
            hits.push(VacuousHit {
                number: index + 1,
                flag: "cargo clippy without -D warnings",
                line: original.to_owned(),
            });
        }
        for flag in VACUOUS_FLAGS.iter() {
            if flag.pattern.is_match(&line) {
                hits.push(VacuousHit {
                    number: index + 1,
                    flag: flag.name,
                    line: original.to_owned(),
                });
            }
        }
    }
    hits
}

/// The hits on one sample line, in a file no exception covers.
fn sample_hits(sample: &str) -> Vec<VacuousHit> {
    vacuous_hits("Makefile", sample, &[], &mut [])
}

/// Scans the files that define gates: the Makefile and its fragments,
/// the pre-commit configuration and every YAML file under .github, the workflows included. An
/// exception that matches nothing is reported too.
#[test]
fn no_vacuous_flags() {
    let fragments = repo::files_under(".mk", &[".mk"]);
    assert!(
        !fragments.is_empty(),
        "no make fragments found under .mk/; the scan would check nothing"
    );
    let workflows = repo::files_under(".github", &[".yaml", ".yml"]);
    assert!(
        !workflows.is_empty(),
        "no YAML found under .github; the checks would see nothing"
    );
    let mut files = vec!["Makefile".to_owned(), ".pre-commit-config.yaml".to_owned()];
    files.extend(fragments);
    files.extend(workflows);

    let mut used = vec![false; ALLOWED_FRAGMENTS.len()];
    let mut problems = Vec::new();
    for file in &files {
        for hit in vacuous_hits(file, &repo::read(file), ALLOWED_FRAGMENTS, &mut used) {
            problems.push(format!(
                "{file}:{}: {}: {}",
                hit.number,
                hit.flag,
                hit.line.trim()
            ));
        }
    }
    for (allowed, used) in ALLOWED_FRAGMENTS.iter().zip(used) {
        if !used {
            problems.push(format!(
                "the exception {:?} in {} ({}) matches nothing; remove it",
                allowed.fragment, allowed.file, allowed.reason
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The negative control: each flag, written as a gate would
/// carry it, is found; a clippy gate that denies warnings and a hook's or step's label are not;
/// and an exception covers only its own fragment in its own file.
#[test]
fn vacuous_flags_are_recognized() {
    for sample in [
        "\tmeson test -C _build || true",
        "\tmeson test -C _build ||:",
        "\tmeson test -C _build || exit 0",
        "\t-meson test -C _build",
        "\t@-meson test -C _build",
        ".IGNORE: meson-test",
        "\t$(MAKE) -i meson-test",
        "\t$(MAKE) --ignore-errors meson-test",
        "        continue-on-error: true",
        "    if: false",
        "    if: ${{ false }}",
        "    if: 'false'",
        "    if: \"${{ false }}\"",
        "\tcargo test --workspace --locked --no-run",
        "        run: cargo clippy --workspace -- --cap-lints warn",
        "\tcargo clippy --workspace -- -A warnings",
        "\tcargo clippy --workspace -- --allow=warnings",
        "        entry: cargo test --workspace --locked -- --skip slow",
        "\tcargo test --workspace --exclude meta --locked",
        "\t$(CARGO) test --workspace --locked --no-run",
        "        run: cargo clippy --workspace --all-targets --locked",
        "\t$(CARGO) clippy --workspace --all-targets --all-features --locked",
        "        entry: cargo clippy --workspace --all-targets --locked",
    ] {
        assert!(
            !sample_hits(sample).is_empty(),
            "no vacuous flag found in {sample:?}"
        );
    }

    for sample in [
        "\t$(CARGO) clippy --workspace --all-targets --all-features --locked -- -D warnings",
        "        entry: cargo clippy --workspace --all-targets --locked -- --deny=warnings",
        "        name: cargo clippy",
        "      - name: Lint with clippy",
        "        entry: cargo test --locked -p meta --test docs",
    ] {
        let hits = sample_hits(sample);
        assert!(hits.is_empty(), "{sample:?} was flagged: {hits:?}");
    }

    // The allowlist mechanism, on a fixture entry: the repository has none of its own yet.
    let release = ".github/workflows/release.yaml";
    let fixture = [AllowedFragment {
        file: release,
        fragment: r#"{ grep -E "$RELEASE_TAG_REGEX" || true; }"#,
        reason: "grep exits 1 when no tag matches, which the step handles after the pipe",
    }];
    let allowed = r#"            { grep -E "$RELEASE_TAG_REGEX" || true; } | sort -V | tail -n1"#;
    let mut used = [false];
    let hits = vacuous_hits(release, allowed, &fixture, &mut used);
    assert!(
        hits.is_empty() && used[0],
        "the allowed fragment was flagged or not recorded: {hits:?}"
    );

    let extended = format!("{allowed} || true");
    for (file, sample) in [
        (".github/workflows/ci.yaml", allowed),
        (release, extended.as_str()),
    ] {
        assert!(
            !vacuous_hits(file, sample, &fixture, &mut [false]).is_empty(),
            "{file}: {sample:?} passed through the allowlist"
        );
    }
}

/// A hit names its line, so a failure is actionable.
#[test]
fn vacuous_hit_numbering() {
    let hits = sample_hits("all:\n\tmeson compile -C _build\n\tmeson test -C _build || true\n");
    assert!(
        hits.len() == 1 && hits[0].number == 3,
        "want one hit on line 3, got {hits:?}"
    );
}
