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

//! The release workflows hold together (docs/release.md): the bump and the merge agree on the
//! version command and the allowed files, which are the four files of the version and the paths
//! that start the publish run; every reusable workflow of gh-reusable-workflows is pinned to one
//! commit and given the trunk; the publish jobs run in their order, each only after the one before;
//! the dry run gates and diffs the bumped tree; the Flathub token is reached by one job, which runs
//! no repository code; and the dispatch inputs are those of master's stubs, which a meta-test on
//! the trunk cannot read (docs/release.md lists them). Every rule has a negative control.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use meta::repo;
use regex::Regex;
use serde_yaml_ng::Value;

const RELEASE: &str = ".github/workflows/release.yaml";
const PUBLISH: &str = ".github/workflows/release-publish.yaml";
const GATE: &str = ".github/workflows/release-gate.yaml";
const FLATHUB: &str = ".github/workflows/flathub-pr.yaml";
const TRUNK: &str = "rust-rewrite";
const VERSION_FILES: [&str; 4] = [
    "Cargo.toml",
    "Cargo.lock",
    "meson.build",
    "data/com.leinardi.gx52.metainfo.xml.in.in",
];
const REUSABLE: &str = "leinardi/gh-reusable-workflows/.github/workflows/";

static PINNED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_./-]+@[0-9a-f]{40}$")
        .unwrap_or_else(|error| panic!("invalid pattern: {error}"))
});

/// The four workflows, parsed.
#[derive(Clone)]
struct Workflows {
    release: Value,
    publish: Value,
    gate: Value,
    flathub: Value,
}

fn parse(name: &str, content: &str) -> Value {
    serde_yaml_ng::from_str(content).unwrap_or_else(|error| panic!("parse {name}: {error}"))
}

fn workflows() -> Workflows {
    Workflows {
        release: parse(RELEASE, &repo::read(RELEASE)),
        publish: parse(PUBLISH, &repo::read(PUBLISH)),
        gate: parse(GATE, &repo::read(GATE)),
        flathub: parse(FLATHUB, &repo::read(FLATHUB)),
    }
}

fn job<'doc>(workflow: &'doc Value, name: &str) -> &'doc Value {
    &workflow["jobs"][name]
}

fn text(value: &Value) -> &str {
    value.as_str().unwrap_or_default()
}

fn words(value: &Value) -> BTreeSet<String> {
    text(value).split_whitespace().map(str::to_owned).collect()
}

fn needs(job: &Value) -> BTreeSet<String> {
    match &job["needs"] {
        Value::String(need) => BTreeSet::from([need.clone()]),
        Value::Sequence(needs) => needs
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::Mapping(_) | Value::Tagged(_) => {
            BTreeSet::new()
        }
    }
}

fn jobs(workflow: &Value) -> Vec<(String, &Value)> {
    workflow["jobs"]
        .as_mapping()
        .into_iter()
        .flatten()
        .filter_map(|(name, job)| Some((name.as_str()?.to_owned(), job)))
        .collect()
}

/// The version command, the allowed files and the publish paths agree, and name the four files.
fn version_problems(workflows: &Workflows) -> Vec<String> {
    let mut problems = Vec::new();
    let bump = &job(&workflows.release, "bump")["with"];
    let decide = &job(&workflows.publish, "decide")["with"];
    let want: BTreeSet<String> = VERSION_FILES
        .iter()
        .map(|file| (*file).to_owned())
        .collect();
    for (name, files) in [
        (
            "release.yaml bump allowed_files",
            words(&bump["allowed_files"]),
        ),
        (
            "release-publish.yaml decide allowed_files",
            words(&decide["allowed_files"]),
        ),
    ] {
        if files != want {
            problems.push(format!("{name} is {files:?}, not the four version files"));
        }
    }
    let paths: BTreeSet<String> = workflows.publish["on"]["push"]["paths"]
        .as_sequence()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if paths != want {
        problems.push(format!(
            "release-publish.yaml runs on {paths:?}, not the four version files"
        ));
    }
    let branches = &workflows.publish["on"]["push"]["branches"];
    if branches.as_sequence().map(Vec::len) != Some(1) || text(&branches[0]) != TRUNK {
        problems.push(format!(
            "release-publish.yaml runs on pushes to {branches:?}, not {TRUNK}"
        ));
    }
    for (name, command) in [
        ("bump", &bump["version_command"]),
        ("decide", &decide["version_command"]),
    ] {
        if text(command) != "make -s release-version" {
            problems.push(format!("{name}'s version_command is {command:?}"));
        }
    }
    if !text(&bump["bump_command"]).starts_with("make release-bump VERSION=\"$RELEASE_VERSION\"") {
        problems.push(format!("bump_command is {:?}", bump["bump_command"]));
    }
    problems
}

/// Every reusable workflow of gh-reusable-workflows is pinned to one commit and gets the trunk;
/// every action is pinned to a full SHA.
fn pin_problems(workflows: &Workflows) -> Vec<String> {
    let mut problems = Vec::new();
    let mut commits = BTreeSet::new();
    for (file, workflow) in [
        (RELEASE, &workflows.release),
        (PUBLISH, &workflows.publish),
        (GATE, &workflows.gate),
        (FLATHUB, &workflows.flathub),
    ] {
        for (name, job) in jobs(workflow) {
            let called = text(&job["uses"]);
            if let Some(rest) = called.strip_prefix(REUSABLE) {
                commits.insert(rest.split('@').nth(1).unwrap_or_default().to_owned());
                if text(&job["with"]["base_branch"]) != TRUNK {
                    problems.push(format!("{file} {name}: base_branch is not {TRUNK}"));
                }
            }
            if !called.is_empty() && !called.starts_with("./") && !PINNED.is_match(called) {
                problems.push(format!(
                    "{file} {name}: {called} is not pinned to a full SHA"
                ));
            }
            for step in job["steps"].as_sequence().into_iter().flatten() {
                let action = text(&step["uses"]);
                if !action.is_empty() && !PINNED.is_match(action) {
                    problems.push(format!(
                        "{file} {name}: {action} is not pinned to a full SHA"
                    ));
                }
            }
            if job["uses"].is_null() && job["timeout-minutes"].is_null() {
                problems.push(format!("{file} {name}: no timeout-minutes"));
            }
            if job["permissions"].is_null() {
                problems.push(format!("{file} {name}: no permissions"));
            }
        }
    }
    if commits.len() != 1 || commits.iter().any(|commit| commit.len() != 40) {
        problems.push(format!(
            "gh-reusable-workflows is called at {commits:?}, not one full SHA"
        ));
    }
    problems
}

/// A job's `needs` and `if`, for the order checks.
fn order(workflow: &Value, name: &str, want_needs: &[&str], want_if: &[&str]) -> Vec<String> {
    let job = job(workflow, name);
    let mut problems = Vec::new();
    if job.is_null() {
        return vec![format!("no {name} job")];
    }
    let have = needs(job);
    for need in want_needs {
        if !have.contains(*need) {
            problems.push(format!("{name} does not need {need}"));
        }
    }
    let condition = text(&job["if"]);
    for part in want_if {
        if !condition.contains(part) {
            problems.push(format!("{name}'s if ({condition:?}) lacks {part:?}"));
        }
    }
    problems
}

/// The publish run decides, gates, tags and ships in that order; the dispatch checks the policy
/// first, and its dry run gates and diffs the bumped tree.
fn order_problems(workflows: &Workflows) -> Vec<String> {
    let released = "needs.decide.outputs.release == 'true'";
    let publish = &workflows.publish;
    let release = &workflows.release;
    let mut problems = [
        order(publish, "gate", &["decide"], &[released]),
        order(publish, "publish", &["decide", "gate"], &[released]),
        order(
            publish,
            "flathub",
            &["decide", "publish"],
            &[released, "startsWith(needs.decide.outputs.tag, 'v0.')"],
        ),
        order(release, "bump", &["policy"], &[]),
        order(release, "gate", &["bump"], &["inputs.dry_run"]),
        order(release, "flathub", &["bump"], &["inputs.dry_run"]),
    ]
    .concat();
    if !needs(job(release, "policy")).is_empty() {
        problems.push("release.yaml's policy job needs another job: it must run first".to_owned());
    }
    if text(&job(publish, "gate")["uses"]) != "./.github/workflows/release-gate.yaml"
        || text(&job(release, "gate")["uses"]) != "./.github/workflows/release-gate.yaml"
    {
        problems.push("the publish run and the dry run do not share release-gate.yaml".to_owned());
    }
    for name in ["gate", "flathub"] {
        if text(&job(release, name)["with"]["patch_artifact"]) != "version-bump-pr-patch" {
            problems.push(format!(
                "release.yaml's {name} does not build the bump's patch"
            ));
        }
    }
    let flathub = &job(publish, "flathub")["with"];
    if text(&flathub["channel"]) != "beta" || flathub["dry_run"] != Value::Bool(false) {
        problems.push(
            "release-publish.yaml's Flathub pull request is not a real run to beta".to_owned(),
        );
    }
    if text(&job(publish, "publish")["with"]["prerelease"])
        != "${{ startsWith(needs.decide.outputs.tag, 'v0.') }}"
    {
        problems.push("a 0.x release is not marked as a pre-release".to_owned());
    }
    problems
}

/// Only the push job reaches the Flathub token, in the `release` environment, and it runs no
/// repository code; a 0.x tag to master is refused before anything is generated.
fn secret_problems(workflows: &Workflows) -> Vec<String> {
    let mut problems = Vec::new();
    for (name, job) in jobs(&workflows.flathub) {
        let rendered = serde_yaml_ng::to_string(job).unwrap_or_default();
        let is_push = name == "push";
        if rendered.contains("secrets.") != is_push {
            problems.push(format!(
                "flathub-pr.yaml {name}: the Flathub token is reached by the wrong job"
            ));
        }
        if (text(&job["environment"]) == "release") != is_push {
            problems.push(format!(
                "flathub-pr.yaml {name}: the release environment is on the wrong job"
            ));
        }
        if is_push {
            let runs_code = job["steps"]
                .as_sequence()
                .into_iter()
                .flatten()
                .any(|step| {
                    text(&step["uses"]).starts_with("actions/checkout@")
                        || text(&step["run"]).contains("scripts/")
                        || text(&step["run"]).contains("make ")
                });
            if runs_code {
                problems.push(
                    "flathub-pr.yaml push: it runs repository code next to the token".to_owned(),
                );
            }
            if text(&job["if"]) != "${{ !inputs.dry_run }}" {
                problems.push("flathub-pr.yaml push: it runs on a dry run".to_owned());
            }
        }
    }
    let refuses = job(&workflows.flathub, "prepare")["steps"][0]["run"]
        .as_str()
        .is_some_and(|run| run.contains("[[ \"$TAG\" == v0.* ]] && [ \"$CHANNEL\" = master ]"));
    if !refuses {
        problems.push("flathub-pr.yaml does not refuse a 0.x tag to master first".to_owned());
    }
    problems
}

/// One dispatch input as master's stub declares it: name, type, required, default (`~` for
/// none), options.
type StubInput = (
    &'static str,
    &'static str,
    bool,
    &'static str,
    &'static [&'static str],
);

/// The dispatch inputs of each workflow master carries a stub of.
const STUB_INPUTS: &[(&str, &[StubInput])] = &[
    (
        RELEASE,
        &[
            ("version", "string", false, "", &[]),
            ("dry_run", "boolean", false, "true", &[]),
        ],
    ),
    (
        FLATHUB,
        &[
            ("tag", "string", true, "~", &[]),
            ("channel", "choice", true, "~", &["beta", "master"]),
            ("dry_run", "boolean", false, "true", &[]),
        ],
    ),
];

fn scalar(value: &Value) -> String {
    match value {
        Value::Null => "~".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Sequence(_) | Value::Mapping(_) | Value::Tagged(_) => "?".to_owned(),
    }
}

fn input_problems(workflows: &Workflows) -> Vec<String> {
    let mut problems = Vec::new();
    for (file, want) in STUB_INPUTS {
        let workflow = if *file == RELEASE {
            &workflows.release
        } else {
            &workflows.flathub
        };
        let inputs = &workflow["on"]["workflow_dispatch"]["inputs"];
        let names: Vec<&str> = inputs
            .as_mapping()
            .into_iter()
            .flatten()
            .filter_map(|(name, _)| name.as_str())
            .collect();
        let want_names: Vec<&str> = want.iter().map(|(name, ..)| *name).collect();
        if names != want_names {
            problems.push(format!(
                "{file}: the dispatch inputs are {names:?}, the stub's {want_names:?}"
            ));
        }
        for (name, kind, required, default, options) in *want {
            let input = &inputs[*name];
            let have_options: Vec<String> = input["options"]
                .as_sequence()
                .into_iter()
                .flatten()
                .map(scalar)
                .collect();
            if scalar(&input["type"]) != *kind
                || input["required"].as_bool().unwrap_or(false) != *required
                || scalar(&input["default"]) != *default
                || have_options != options.to_vec()
            {
                problems.push(format!("{file}: the input {name} is not the stub's"));
            }
        }
    }
    problems
}

fn all_problems(workflows: &Workflows) -> Vec<String> {
    [
        version_problems(workflows),
        pin_problems(workflows),
        order_problems(workflows),
        secret_problems(workflows),
        input_problems(workflows),
    ]
    .concat()
}

#[test]
fn the_release_workflows_hold_together() {
    let problems = all_problems(&workflows());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Replaces the value at `path` (the workflow's short name, then its keys) with `value`.
fn set(workflows: &mut Workflows, path: &[&str], value: Value) {
    let (file, keys) = path
        .split_first()
        .unwrap_or_else(|| panic!("an empty path"));
    let mut target = match *file {
        "release" => &mut workflows.release,
        "publish" => &mut workflows.publish,
        "gate" => &mut workflows.gate,
        "flathub" => &mut workflows.flathub,
        other => panic!("no workflow {other}"),
    };
    for key in keys {
        target = &mut target[*key];
    }
    *target = value;
}

/// Each mutation breaks one rule, and the checks see it.
#[test]
fn the_checks_catch_each_break() {
    let other_commit = format!("{REUSABLE}release-on-merge.yaml@{}", "1".repeat(40));
    let cases: Vec<(&str, Vec<&str>, Value)> = vec![
        (
            "allowed_files drift",
            vec!["publish", "jobs", "decide", "with", "allowed_files"],
            "Cargo.toml".into(),
        ),
        (
            "version_command drift",
            vec!["release", "jobs", "bump", "with", "version_command"],
            "cat VERSION".into(),
        ),
        (
            "a publish path missing",
            vec!["publish", "on", "push", "paths"],
            Value::Sequence(vec!["Cargo.toml".into()]),
        ),
        (
            "no trunk for the bump",
            vec!["release", "jobs", "bump", "with", "base_branch"],
            Value::Null,
        ),
        (
            "another reusable commit",
            vec!["publish", "jobs", "decide", "uses"],
            other_commit.into(),
        ),
        (
            "an action on a tag",
            vec!["gate", "jobs", "bundles", "steps"],
            parse("step", "- uses: actions/upload-artifact/merge@v7"),
        ),
        (
            "the tag before the gate",
            vec!["publish", "jobs", "publish", "needs"],
            "decide".into(),
        ),
        (
            "Flathub for 1.x on push",
            vec!["publish", "jobs", "flathub", "if"],
            "needs.decide.outputs.release == 'true'".into(),
        ),
        (
            "the dry run gates the unbumped tree",
            vec!["release", "jobs", "gate", "with"],
            Value::Null,
        ),
        (
            "the token in prepare",
            vec!["flathub", "jobs", "prepare", "env"],
            parse("env", "T: ${{ secrets.FLATHUB_TOKEN }}"),
        ),
        (
            "the push job on a dry run",
            vec!["flathub", "jobs", "push", "if"],
            Value::Null,
        ),
        (
            "a default channel",
            vec![
                "flathub",
                "on",
                "workflow_dispatch",
                "inputs",
                "channel",
                "default",
            ],
            "beta".into(),
        ),
        (
            "a real run by default",
            vec![
                "release",
                "on",
                "workflow_dispatch",
                "inputs",
                "dry_run",
                "default",
            ],
            false.into(),
        ),
        (
            "no pre-release for 0.x",
            vec!["publish", "jobs", "publish", "with", "prerelease"],
            false.into(),
        ),
        (
            "no timeout",
            vec!["gate", "jobs", "build", "timeout-minutes"],
            Value::Null,
        ),
    ];
    let good = workflows();
    for (name, path, value) in cases {
        let mut broken = good.clone();
        set(&mut broken, &path, value);
        assert!(!all_problems(&broken).is_empty(), "{name}: not caught");
    }
}
