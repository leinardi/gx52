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
// Adapted from swarm-device-access/meta/tests/toolchain.rs at 4cd31c4, originally
// Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi

//! The repository's tooling pins: one exact Rust release across rust-toolchain.toml, the
//! workspace's `rust-version` and every CI toolchain step; the whole-tree hooks running once; and
//! no meta-test that skips on a missing tool. `vacuous_flags.rs` checks the rest of the gates.
//!
//! The Rust release is the one of the `org.freedesktop.Sdk.Extension.rust-stable` extension the
//! Flatpak builds use (docs/architecture.md): Cargo refuses a `rust-version` newer than its
//! `rustc`. Every rule has its negative control.

use std::collections::BTreeMap;
use std::sync::LazyLock;

use meta::repo;
use regex::Regex;
use serde_yaml_ng::Value;

const RUST_TOOLCHAIN_ACTION: &str = "dtolnay/rust-toolchain";
const PRE_COMMIT_CONFIG: &str = ".pre-commit-config.yaml";

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("invalid pattern {pattern}: {error}"))
}

/// rust-toolchain.toml's channel, which must name one exact release.
static RUST_CHANNEL: LazyLock<Regex> = LazyLock::new(|| regex(r#"(?m)^channel = "([^"]*)"$"#));
/// The workspace's rust-version in the root Cargo.toml.
static RUST_VERSION: LazyLock<Regex> = LazyLock::new(|| regex(r#"(?m)^rust-version = "([^"]*)"$"#));
/// A Rust release with its patch: 1.98.1, never 1.98 or stable.
static EXACT_RUST: LazyLock<Regex> = LazyLock::new(|| regex(r"^\d+\.\d+\.\d+$"));

/// One step, in a workflow or a composite action, that uses an action.
#[derive(Debug)]
struct ActionStep {
    file: String,
    /// The `uses:` value without its `@ref`.
    action: String,
    with: BTreeMap<String, String>,
}

/// Every mapping with a `uses:` key anywhere in the document: job steps, composite action steps
/// and reusable workflow calls alike.
fn parse_action_steps(file: &str, content: &str) -> Vec<ActionStep> {
    fn walk(file: &str, node: &Value, steps: &mut Vec<ActionStep>) {
        match node {
            Value::Mapping(mapping) => {
                if let Some(Value::String(uses)) = mapping.get("uses") {
                    let action = uses
                        .split_once('@')
                        .map_or(uses.as_str(), |(action, _)| action);
                    let with = mapping
                        .get("with")
                        .and_then(Value::as_mapping)
                        .map(|inputs| {
                            inputs
                                .iter()
                                .filter_map(|(key, value)| {
                                    let value = match value {
                                        Value::String(text) => text.clone(),
                                        Value::Number(number) => number.to_string(),
                                        Value::Bool(flag) => flag.to_string(),
                                        Value::Null
                                        | Value::Sequence(_)
                                        | Value::Mapping(_)
                                        | Value::Tagged(_) => return None,
                                    };
                                    Some((key.as_str()?.to_owned(), value))
                                })
                                .collect()
                        })
                        .unwrap_or_default();
                    steps.push(ActionStep {
                        file: file.to_owned(),
                        action: action.to_owned(),
                        with,
                    });
                }
                for value in mapping.values() {
                    walk(file, value, steps);
                }
            }
            Value::Sequence(sequence) => {
                for value in sequence {
                    walk(file, value, steps);
                }
            }
            Value::Null
            | Value::Bool(_)
            | Value::Number(_)
            | Value::String(_)
            | Value::Tagged(_) => {}
        }
    }

    let document: Value =
        serde_yaml_ng::from_str(content).unwrap_or_else(|error| panic!("parse {file}: {error}"));
    let mut steps = Vec::new();
    walk(file, &document, &mut steps);
    steps
}

/// Every YAML file under .github, the workflows and the composite actions alike.
fn github_yaml() -> Vec<String> {
    let files = repo::files_under(".github", &[".yaml", ".yml"]);
    assert!(
        !files.is_empty(),
        "no YAML files under .github; the checks would see nothing"
    );
    files
}

fn github_steps() -> Vec<ActionStep> {
    github_yaml()
        .iter()
        .flat_map(|file| parse_action_steps(file, &repo::read(file)))
        .collect()
}

/// The Rust rule: every toolchain step installs exactly `want`.
fn rust_step_problems(steps: &[ActionStep], want: &str) -> Vec<String> {
    steps
        .iter()
        .filter(|step| step.action == RUST_TOOLCHAIN_ACTION)
        .filter_map(|step| {
            let installed = step.with.get("toolchain").map_or("", String::as_str);
            (installed != want).then(|| {
                format!(
                    "{}: {} installs toolchain {installed:?}; it must be {want}, the release of \
                     rust-toolchain.toml",
                    step.file, step.action
                )
            })
        })
        .collect()
}

/// One exact Rust release, patch included, is shared by rust-toolchain.toml's channel, the
/// workspace's rust-version and every CI toolchain step (the bare-runner jobs of ci.yaml; the
/// Flatpak jobs use the SDK extension's Rust, which the pin follows), so the toolchain a
/// developer, a hook and CI build with is one compiler.
#[test]
fn rust_line_is_one_release() {
    let toolchain = repo::read("rust-toolchain.toml");
    let want = RUST_CHANNEL
        .captures(&toolchain)
        .map(|captures| captures[1].to_owned())
        .filter(|channel| EXACT_RUST.is_match(channel))
        .expect("rust-toolchain.toml names an exact release (channel = \"X.Y.Z\")");

    let mut problems = Vec::new();
    let manifest = repo::read("Cargo.toml");
    match RUST_VERSION.captures(&manifest) {
        Some(version) if version[1] == want => {}
        Some(version) => problems.push(format!(
            "Cargo.toml's rust-version is {:?}, but rust-toolchain.toml says {want}",
            &version[1]
        )),
        None => problems.push("the root Cargo.toml sets no rust-version".to_owned()),
    }
    let steps = github_steps();
    if !steps
        .iter()
        .any(|step| step.action == RUST_TOOLCHAIN_ACTION)
    {
        problems.push(format!(
            "no {RUST_TOOLCHAIN_ACTION} step under .github; the check would see nothing"
        ));
    }
    problems.extend(rust_step_problems(&steps, &want));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The negative control of the Rust toolchain step rule: another
/// release, a channel, a release without its patch, and no toolchain at all.
#[test]
fn rust_step_rules() {
    let steps = parse_action_steps(
        "sample.yaml",
        r#"jobs:
  ok:
    steps:
      - uses: dtolnay/rust-toolchain@0123456789abcdef0123456789abcdef01234567 # master
        with:
          toolchain: 1.98.1
  bad:
    steps:
      - uses: dtolnay/rust-toolchain@0123456789abcdef0123456789abcdef01234567
        with:
          toolchain: 1.98.0
      - uses: dtolnay/rust-toolchain@0123456789abcdef0123456789abcdef01234567
        with:
          toolchain: stable
      - uses: dtolnay/rust-toolchain@0123456789abcdef0123456789abcdef01234567
        with:
          toolchain: "1.98"
      - uses: dtolnay/rust-toolchain@0123456789abcdef0123456789abcdef01234567
"#,
    );
    assert_eq!(steps.len(), 5, "{steps:?}");
    let problems = rust_step_problems(&steps, "1.98.1");
    assert_eq!(problems.len(), 4, "{problems:#?}");
}

/// The readings the Rust line rests on: a channel or a release without its patch is not an
/// exact release.
#[test]
fn rust_line_reading() {
    let channel = RUST_CHANNEL
        .captures("[toolchain]\nchannel = \"1.98.1\"\n")
        .map(|captures| captures[1].to_owned());
    assert_eq!(channel.as_deref(), Some("1.98.1"));
    let version = RUST_VERSION
        .captures("[workspace.package]\nrust-version = \"1.98\"\n")
        .map(|captures| captures[1].to_owned());
    assert_eq!(version.as_deref(), Some("1.98"));
    for inexact in ["stable", "1.98", "nightly-2026-10-01", "1.98.1-x86_64"] {
        assert!(
            !EXACT_RUST.is_match(inexact),
            "{inexact:?} was accepted as an exact release"
        );
    }
}

/// The hooks of .pre-commit-config.yaml by id, as written; an id with aliases (the actionlint,
/// shellcheck and yamllint variants) appears more than once.
fn pre_commit_hooks(content: &str) -> Vec<(String, Value)> {
    let config: Value = serde_yaml_ng::from_str(content)
        .unwrap_or_else(|error| panic!("parse {PRE_COMMIT_CONFIG}: {error}"));
    let repos = config
        .get("repos")
        .and_then(Value::as_sequence)
        .unwrap_or_else(|| panic!("{PRE_COMMIT_CONFIG} has no repos list"));
    repos
        .iter()
        .filter_map(|entry| entry.get("hooks").and_then(Value::as_sequence))
        .flatten()
        .map(|hook| {
            let id = hook.get("id").and_then(Value::as_str).unwrap_or_default();
            (id.to_owned(), hook.clone())
        })
        .collect()
}

/// The hooks that read the whole tree rather than the files pre-commit passes, so each one runs
/// on every commit and must not run twice per commit.
const ALWAYS_RUN_HOOKS: [&str; 3] = ["check-license-headers", "check-suppressions", "docs-links"];

/// What is wrong with the whole-tree hooks of a pre-commit configuration.
fn always_run_problems(content: &str) -> Vec<String> {
    let hooks = pre_commit_hooks(content);
    let mut problems = Vec::new();
    for id in ALWAYS_RUN_HOOKS {
        let found: Vec<&Value> = hooks
            .iter()
            .filter(|(hook_id, _)| hook_id == id)
            .map(|(_, hook)| hook)
            .collect();
        let [hook] = found.as_slice() else {
            problems.push(format!(
                "hook {id} appears {} times, want once",
                found.len()
            ));
            continue;
        };
        if hook.get("always_run").and_then(Value::as_bool) != Some(true) {
            problems.push(format!("hook {id} does not set always_run: true"));
        }
        if hook.get("pass_filenames").and_then(Value::as_bool) != Some(false) {
            problems.push(format!("hook {id} does not set pass_filenames: false"));
        }
        let stages: Option<Vec<&str>> = hook
            .get("stages")
            .and_then(Value::as_sequence)
            .map(|stages| stages.iter().filter_map(Value::as_str).collect());
        if stages.as_deref() != Some(["pre-commit"].as_slice()) {
            problems.push(format!(
                "hook {id} runs at stages {stages:?}, want exactly [pre-commit]"
            ));
        }
    }
    problems
}

/// A whole-tree hook takes no file list and always runs, because a commit that only deletes a
/// file passes no file to it; and it is pinned to the pre-commit stage, because `always_run`
/// makes a hook eligible at every installed stage, commit-msg included.
#[test]
fn always_run_hooks_run_once() {
    let problems = always_run_problems(&repo::read(PRE_COMMIT_CONFIG));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The negative control: a whole-tree hook that is missing, doubled, passed file names, not
/// always run, or eligible at another stage is refused.
#[test]
fn always_run_rules() {
    let hook = |id: &str, settings: &str| {
        format!("      - id: {id}\n        entry: x\n        language: system\n{settings}")
    };
    let good = "        pass_filenames: false\n        always_run: true\n        stages: \
                [pre-commit]\n";
    let config = |docs_links: &str| {
        format!(
            "repos:\n  - repo: local\n    hooks:\n{}{}{docs_links}",
            hook("check-license-headers", good),
            hook("check-suppressions", good),
        )
    };
    assert_eq!(
        always_run_problems(&config(&hook("docs-links", good))),
        Vec::<String>::new()
    );
    for (name, docs_links) in [
        ("a missing hook", String::new()),
        (
            "a doubled hook",
            format!("{}{}", hook("docs-links", good), hook("docs-links", good)),
        ),
        (
            "file names passed",
            hook(
                "docs-links",
                &good.replace("pass_filenames: false", "pass_filenames: true"),
            ),
        ),
        (
            "not always run",
            hook(
                "docs-links",
                &good.replace("always_run: true", "always_run: false"),
            ),
        ),
        (
            "every stage",
            hook(
                "docs-links",
                &good.replace("stages: [pre-commit]", "stages: [pre-commit, commit-msg]"),
            ),
        ),
    ] {
        assert!(
            !always_run_problems(&config(&docs_links)).is_empty(),
            "{name} was accepted"
        );
    }
}

/// A tool looked up on `PATH`.
static TOOL_LOOKUP: LazyLock<Regex> = LazyLock::new(|| {
    regex(r"\bsplit_paths\(|\bvar_os\(\s*\x22PATH\x22|Command::new\(\s*\x22which\x22")
});

/// No meta-test may skip on a missing tool: none looks a tool up on `PATH`, since a skip on a
/// missing tool is a pass CI cannot turn into a failure. A test that needs an external tool fails
/// without it.
#[test]
fn no_meta_test_skips_on_a_missing_tool() {
    let sources = repo::files_under("meta", &[".rs"]);
    assert!(
        sources.iter().any(|path| path == "meta/tests/toolchain.rs"),
        "the walk did not reach meta/tests: {sources:?}"
    );
    for path in &sources {
        assert!(
            !TOOL_LOOKUP.is_match(&repo::read(path)),
            "{path} looks a tool up on PATH; a meta-test needing a tool \
             must fail without it"
        );
    }

    // The negative control. The samples are split so this file does not match its own check.
    for sample in [
        concat!("let found = env::split", "_paths(&paths);"),
        concat!("std::env::var", "_os(\x22PATH\x22)"),
        concat!("Command::new(\x22wh", "ich\x22)"),
    ] {
        assert!(
            TOOL_LOOKUP.is_match(sample),
            "{sample:?} was not recognised"
        );
    }
}
