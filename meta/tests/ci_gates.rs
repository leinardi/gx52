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
// Adapted from swarm-device-access/meta/tests/ci_gates.rs at 4cd31c4, originally
// Apache-2.0-licensed, Copyright (c) 2026 Roberto Leinardi

//! Every CI gate is required: each job of ci.yaml that is a gate rather than an annotation is
//! needed by `ci-result`, the one required check, and `pre-commit-all` skips only the hooks that
//! needed jobs run instead. The cargo-test hook is split between two jobs, and together they test
//! every workspace member.

use std::collections::{BTreeMap, BTreeSet};

use meta::repo;
use serde_yaml_ng::Value;

const CI_WORKFLOW: &str = ".github/workflows/ci.yaml";
const RESULT_JOB: &str = "ci-result";
const PRE_COMMIT_JOB: &str = "pre-commit-all";
const SDK_JOB: &str = "sdk-checks";
const META_JOB: &str = "meta";

/// For each hook `pre-commit-all` may skip, the ci.yaml jobs that run it instead. A skip is only
/// safe when every one of those jobs exists and `ci-result` needs it.
///
/// cargo-test runs `cargo test --workspace` locally, but CI cannot run it in one place: gx52 and
/// x52 need GTK 4 and libadwaita, which only the GNOME SDK build shell of sdk-checks has, and the
/// meta-tests read the git checkout, which that build shell lacks, so they run in the meta job on
/// the bare runner. `cargo_test_jobs_cover_the_workspace` checks that the packages the two jobs
/// test are, together, every workspace member.
const SKIPPED_HOOK_RUNNERS: &[(&str, &[&str])] = &[
    ("cargo-clippy", &[SDK_JOB]),
    ("cargo-test", &[SDK_JOB, META_JOB]),
    ("cargo-deny", &["deny"]),
    ("blueprint-format", &[SDK_JOB]),
    ("meson-format", &[SDK_JOB]),
    ("flatpak-builder-lint", &["flatpak"]),
];

/// The jobs that only post review comments on a pull request; `ci-result` does not need them,
/// because `pre-commit-all` runs the same hooks on every file.
const ANNOTATE_ONLY_JOBS: [&str; 5] = [
    "actionlint",
    "pre-commit-hooks",
    "markdownlint",
    "shellcheck",
    "yamllint",
];

/// One job of the workflow: the jobs it needs, the `SKIP` value of each of its steps, and the
/// shell its steps run.
#[derive(Debug, Default)]
struct Job {
    needs: Vec<String>,
    skips: Vec<String>,
    runs: Vec<String>,
}

/// The part of ci.yaml these checks read, by job name.
fn parse_ci(content: &str) -> BTreeMap<String, Job> {
    let document: Value = serde_yaml_ng::from_str(content)
        .unwrap_or_else(|error| panic!("parse the CI workflow: {error}"));
    let Some(jobs) = document.get("jobs").and_then(Value::as_mapping) else {
        return BTreeMap::new();
    };
    jobs.iter()
        .filter_map(|(name, job)| {
            let needs = match job.get("needs") {
                Some(Value::String(need)) => vec![need.clone()],
                Some(Value::Sequence(needs)) => needs
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect(),
                Some(
                    Value::Null
                    | Value::Bool(_)
                    | Value::Number(_)
                    | Value::Mapping(_)
                    | Value::Tagged(_),
                )
                | None => Vec::new(),
            };
            let steps: &[Value] = job
                .get("steps")
                .and_then(Value::as_sequence)
                .map_or(&[], Vec::as_slice);
            let skips = steps
                .iter()
                .filter_map(|step| step.get("env")?.get("SKIP")?.as_str())
                .map(str::to_owned)
                .collect();
            let runs = steps
                .iter()
                .filter_map(|step| step.get("run")?.as_str())
                .map(str::to_owned)
                .collect();
            Some((name.as_str()?.to_owned(), Job { needs, skips, runs }))
        })
        .collect()
}

/// Every gate job `ci-result` does not need.
fn ci_result_problems(jobs: &BTreeMap<String, Job>) -> Vec<String> {
    let Some(result) = jobs.get(RESULT_JOB) else {
        return vec![format!("the CI workflow has no {RESULT_JOB} job")];
    };
    jobs.keys()
        .filter(|name| *name != RESULT_JOB && !ANNOTATE_ONLY_JOBS.contains(&name.as_str()))
        .filter(|name| !result.needs.contains(name))
        .map(|name| format!("{RESULT_JOB} does not need the {name} job"))
        .collect()
}

/// Every hook `pre-commit-all` skips that is not a hook of `hooks` (the pre-commit
/// configuration), or is not run by jobs `ci-result` needs.
fn skip_problems(jobs: &BTreeMap<String, Job>, hooks: &str) -> Vec<String> {
    let Some(job) = jobs.get(PRE_COMMIT_JOB) else {
        return vec![format!("the CI workflow has no {PRE_COMMIT_JOB} job")];
    };
    let needed: &[String] = jobs
        .get(RESULT_JOB)
        .map_or(&[], |result| result.needs.as_slice());
    let mut problems = Vec::new();
    for hook in job.skips.iter().flat_map(|skip| skip.split(',')) {
        let hook = hook.trim();
        if hook.is_empty() {
            continue;
        }
        if !hooks.contains(&format!("- id: {hook}\n")) {
            problems.push(format!("{PRE_COMMIT_JOB} skips {hook}, which is no hook"));
            continue;
        }
        let Some((_, runners)) = SKIPPED_HOOK_RUNNERS
            .iter()
            .find(|(skipped, _)| *skipped == hook)
        else {
            problems.push(format!(
                "{PRE_COMMIT_JOB} skips {hook}, which no dedicated job is recorded to run"
            ));
            continue;
        };
        for runner in *runners {
            if !jobs.contains_key(*runner) {
                problems.push(format!(
                    "{PRE_COMMIT_JOB} skips {hook}, to be run by the {runner} job, which does \
                     not exist"
                ));
            } else if !needed.iter().any(|need| need == runner) {
                problems.push(format!(
                    "{PRE_COMMIT_JOB} skips {hook}, run by the {runner} job, which {RESULT_JOB} \
                     does not need"
                ));
            }
        }
    }
    problems
}

/// The packages the `cargo test` commands of `job` name with `-p`.
fn tested_packages(job: &Job) -> BTreeSet<String> {
    let mut packages = BTreeSet::new();
    for line in job.runs.iter().flat_map(|run| run.lines()) {
        let words: Vec<&str> = line.split_whitespace().collect();
        if !words.windows(2).any(|pair| pair == ["cargo", "test"]) {
            continue;
        }
        for pair in words.windows(2) {
            if pair[0] == "-p" || pair[0] == "--package" {
                packages.insert(pair[1].to_owned());
            }
        }
    }
    packages
}

/// What is wrong with the split of cargo-test between `sdk-checks` and `meta`: together they
/// must test exactly `members`.
fn cargo_test_split_problems(
    jobs: &BTreeMap<String, Job>,
    members: &BTreeSet<String>,
) -> Vec<String> {
    let mut tested = BTreeSet::new();
    let mut problems = Vec::new();
    for name in [SDK_JOB, META_JOB] {
        match jobs.get(name) {
            Some(job) => {
                let packages = tested_packages(job);
                if packages.is_empty() {
                    problems.push(format!("the {name} job runs no cargo test -p <package>"));
                }
                tested.extend(packages);
            }
            None => problems.push(format!("the CI workflow has no {name} job")),
        }
    }
    for missing in members.difference(&tested) {
        problems.push(format!(
            "no CI job runs the tests of the workspace member {missing}"
        ));
    }
    for unknown in tested.difference(members) {
        problems.push(format!("CI tests {unknown}, which is no workspace member"));
    }
    problems
}

/// The package names of the workspace members.
fn workspace_packages() -> BTreeSet<String> {
    let parse = |relative: &str| -> toml::Table {
        repo::read(relative)
            .parse()
            .unwrap_or_else(|error| panic!("parse {relative}: {error}"))
    };
    let root = parse("Cargo.toml");
    let members = root
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(toml::Value::as_array)
        .unwrap_or_else(|| panic!("the root Cargo.toml lists no workspace members"));
    members
        .iter()
        .map(|member| {
            let member = member
                .as_str()
                .unwrap_or_else(|| panic!("member {member} is not a path"));
            let manifest = parse(&format!("{member}/Cargo.toml"));
            manifest
                .get("package")
                .and_then(|package| package.get("name"))
                .and_then(toml::Value::as_str)
                .unwrap_or_else(|| panic!("{member}/Cargo.toml names no package"))
                .to_owned()
        })
        .collect()
}

/// Every job of ci.yaml that is a gate rather than an annotation is needed by `ci-result`, so a
/// new gate cannot fail without failing the pull request.
#[test]
fn ci_gates_are_required() {
    let problems = ci_result_problems(&parse_ci(&repo::read(CI_WORKFLOW)));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// `pre-commit-all` runs every hook on every file except the ones in its `SKIP` list, and each of
/// those must be run by jobs `ci-result` needs.
#[test]
fn pre_commit_all_skips_only_hooks_run_elsewhere() {
    let jobs = parse_ci(&repo::read(CI_WORKFLOW));
    assert!(
        jobs.get(PRE_COMMIT_JOB)
            .is_some_and(|job| !job.skips.is_empty()),
        "{PRE_COMMIT_JOB} skips nothing; the check would pass on nothing"
    );
    let problems = skip_problems(&jobs, &repo::read(".pre-commit-config.yaml"));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The two jobs that share the cargo-test hook test every workspace member between them, so a new
/// member cannot fall out of CI.
#[test]
fn cargo_test_jobs_cover_the_workspace() {
    let members = workspace_packages();
    assert!(
        members.len() >= 3,
        "the workspace members were not read: {members:?}"
    );
    let problems = cargo_test_split_problems(&parse_ci(&repo::read(CI_WORKFLOW)), &members);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The negative control of the three checks above.
#[test]
fn ci_structure_rules() {
    let jobs = parse_ci(
        "jobs:
  pre-commit-all:
    steps:
      - env:
          SKIP: cargo-test,cargo-clippy,unknown-hook,meson-format
  sdk-checks:
    steps:
      - run: |
          cargo clippy --workspace -- -D warnings
          cargo test --locked -p gx52
  meta:
    steps:
      - run: cargo test --locked -p meta
  test:
    steps: []
  markdownlint:
    steps: []
  ci-result:
    needs: [pre-commit-all, sdk-checks, test]
",
    );
    let hooks = "      - id: cargo-test\n      - id: cargo-clippy\n      - id: meson-format\n";

    // meta is a gate ci-result does not need; markdownlint only annotates.
    let problems = ci_result_problems(&jobs);
    assert!(
        problems.len() == 1 && problems[0].contains("meta"),
        "want one problem naming meta, got {problems:?}"
    );

    // cargo-test is run by sdk-checks and by meta, which ci-result does not need; cargo-clippy and
    // meson-format by sdk-checks, which it does; unknown-hook is not a hook at all.
    let problems = skip_problems(&jobs, hooks);
    assert_eq!(problems.len(), 2, "{problems:#?}");
    assert!(
        problems[0].contains("cargo-test") && problems[0].contains("the meta job"),
        "{problems:#?}"
    );

    // A flatpak-builder-lint skip with no flatpak job at all.
    let problems = skip_problems(
        &parse_ci(
            "jobs:\n  pre-commit-all:\n    steps:\n      - env:\n          SKIP: \
             flatpak-builder-lint\n  ci-result:\n    needs: [pre-commit-all]\n",
        ),
        "      - id: flatpak-builder-lint\n",
    );
    assert!(
        problems.len() == 1 && problems[0].contains("does not exist"),
        "{problems:#?}"
    );

    // x52 is tested by neither job; a package that is no member is refused too.
    let members: BTreeSet<String> = ["gx52", "meta", "x52"].map(str::to_owned).into();
    let problems = cargo_test_split_problems(&jobs, &members);
    assert_eq!(problems.len(), 1, "{problems:#?}");
    assert!(problems[0].contains("x52"), "{problems:#?}");
    let extra = parse_ci(
        "jobs:\n  sdk-checks:\n    steps:\n      - run: cargo test -p gx52 -p x52 -p ghost\n  \
         meta:\n    steps:\n      - run: cargo test -p meta\n",
    );
    let problems = cargo_test_split_problems(&extra, &members);
    assert!(
        problems.len() == 1 && problems[0].contains("ghost"),
        "{problems:#?}"
    );
    let empty_meta = parse_ci(
        "jobs:\n  sdk-checks:\n    steps:\n      - run: cargo test -p gx52 -p x52 -p meta\n  \
         meta:\n    steps:\n      - run: cargo build\n",
    );
    let problems = cargo_test_split_problems(&empty_meta, &members);
    assert!(
        problems.len() == 1 && problems[0].contains("the meta job runs no cargo test"),
        "{problems:#?}"
    );

    let problems = skip_problems(&parse_ci("jobs:\n  ci-result:\n    needs: []\n"), hooks);
    assert_eq!(
        problems.len(),
        1,
        "a workflow without pre-commit-all: {problems:?}"
    );
    let problems = ci_result_problems(&parse_ci("jobs:\n  test:\n    steps: []\n"));
    assert_eq!(
        problems.len(),
        1,
        "a workflow without ci-result: {problems:?}"
    );
}

/// Rustdoc warnings must fail both the local verify target and the sdk-checks job, which runs
/// `make rust-doc` in the SDK build shell.
fn rustdoc_gate_present(fragment: &str, workflow: &str) -> bool {
    let verify = fragment.lines().any(|line| {
        line.starts_with("verify-rust:")
            && line.split_whitespace().any(|target| target == "rust-doc")
    });
    let recipe = fragment.contains(
        "rust-doc: ## Check Rust documentation, warnings as errors\n\tRUSTDOCFLAGS=\"$(RUSTDOCFLAGS) -D warnings\" $(CARGO) doc --workspace --all-features --no-deps --locked\n",
    );
    let ci = parse_ci(workflow).get(SDK_JOB).is_some_and(|job| {
        job.runs
            .iter()
            .flat_map(|run| run.lines())
            .any(|line| line.trim() == "make rust-doc")
    });
    verify && recipe && ci
}

#[test]
fn rust_documentation_is_a_required_gate() {
    assert!(rustdoc_gate_present(
        &repo::read(".mk/rust.mk"),
        &repo::read(CI_WORKFLOW),
    ));
}

/// Negative controls: a gate absent from verify or CI, or one allowing warnings, must fail.
#[test]
fn weakened_rustdoc_gates_are_refused() {
    let fragment = repo::read(".mk/rust.mk");
    let workflow = repo::read(CI_WORKFLOW);
    for broken in [
        fragment.replace("rust-test rust-doc", "rust-test"),
        fragment.replace(" -D warnings", ""),
    ] {
        assert!(!rustdoc_gate_present(&broken, &workflow));
    }
    let without = workflow.replace("make rust-doc\n", "echo missing documentation gate\n");
    assert_ne!(without, workflow, "the workflow runs no make rust-doc line");
    assert!(!rustdoc_gate_present(&fragment, &without));
}
