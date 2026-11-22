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

//! The release workflows hold together (docs/internal/release.md): the bump and the merge agree on the
//! version command and the allowed files, which are the four files of the version and the paths
//! that start the publish run; every reusable workflow of gh-reusable-workflows is pinned to one
//! commit and given the trunk; the publish jobs run in their order, each only after the one before;
//! the dry run gates and checks the bumped tree; the release attaches the Flathub files; no release
//! workflow reaches a secret or an environment, as the Flathub pull request is opened on the
//! Flathub side; and the dispatch inputs are those of master's stubs, which a meta-test on the
//! trunk cannot read (docs/internal/release.md lists them). Every rule has a negative control.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use meta::repo;
use regex::Regex;
use serde_yaml_ng::{Mapping, Value};

const RELEASE: &str = ".github/workflows/release.yaml";
const PUBLISH: &str = ".github/workflows/release-publish.yaml";
const GATE: &str = ".github/workflows/release-gate.yaml";
const FLATHUB: &str = ".github/workflows/flathub-check.yaml";
const TRUNK: &str = "main";
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

/// Every reusable workflow of gh-reusable-workflows is pinned to one commit and names no
/// `base_branch`: the trunk is the default branch, which they use when none is given. Every action
/// is pinned to a full SHA.
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
                if !job["with"]["base_branch"].is_null() {
                    problems.push(format!(
                        "{file} {name}: base_branch is set; the trunk, {TRUNK}, is the default branch"
                    ));
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

/// The publish run decides, then gates and checks the Flathub files, then merges the assets and
/// tags in that order; the dispatch checks the policy first, and its dry run gates and checks the
/// bumped tree.
fn order_problems(workflows: &Workflows) -> Vec<String> {
    let released = "needs.decide.outputs.release == 'true'";
    let publish = &workflows.publish;
    let release = &workflows.release;
    let mut problems = [
        order(publish, "gate", &["decide"], &[released]),
        order(publish, "flathub", &["decide"], &[released]),
        order(publish, "assets", &["gate", "flathub"], &[released]),
        order(publish, "publish", &["decide", "assets"], &[released]),
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
    for (name, workflow, tag) in [
        (
            "release-publish.yaml",
            publish,
            "${{ needs.decide.outputs.tag }}",
        ),
        ("release.yaml", release, "${{ needs.bump.outputs.tag }}"),
    ] {
        let flathub = job(workflow, "flathub");
        if text(&flathub["uses"]) != "./.github/workflows/flathub-check.yaml"
            || text(&flathub["with"]["tag"]) != tag
        {
            problems.push(format!(
                "{name}'s flathub job does not check the Flathub files of the release's tag"
            ));
        }
    }
    problems.extend(asset_problems(publish, &workflows.flathub));
    let given: Vec<&str> = job(publish, "flathub")["with"]
        .as_mapping()
        .into_iter()
        .flatten()
        .filter_map(|(key, _)| key.as_str())
        .collect();
    if given != ["tag"] {
        problems.push(format!(
            "release-publish.yaml's Flathub check takes {given:?}: the merge commit, as it is, \
             takes only the tag"
        ));
    }
    if text(&job(publish, "publish")["with"]["prerelease"])
        != "${{ startsWith(needs.decide.outputs.tag, 'v0.') }}"
    {
        problems.push("a 0.x release is not marked as a pre-release".to_owned());
    }
    problems
}

/// The four files the release attaches.
const ASSETS: [&str; 4] = [
    "cargo-sources.json",
    "com.leinardi.gx52.json",
    "gx52-aarch64.flatpak",
    "gx52-x86_64.flatpak",
];

/// The release attaches the bundles and both Flathub files: the merged artifact the publish job
/// names takes the per-architecture bundles and flathub-check's `flathub-files`, which holds both
/// files, and the merged list is checked before the tag.
fn asset_problems(publish: &Value, flathub: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    let upload = job(flathub, "prepare")["steps"]
        .as_sequence()
        .into_iter()
        .flatten()
        .find(|step| text(&step["uses"]).starts_with("actions/upload-artifact@"));
    let uploads_both = upload.is_some_and(|step| {
        let paths = text(&step["with"]["path"]);
        text(&step["with"]["name"]) == "flathub-files"
            && paths.contains("/${{ env.APP_ID }}.json")
            && paths.contains("/cargo-sources.json")
            && text(&step["with"]["if-no-files-found"]) == "error"
    });
    if !uploads_both {
        problems.push(
            "flathub-check.yaml does not upload both Flathub files as flathub-files".to_owned(),
        );
    }
    let checked = runs(job(publish, "assets"));
    if !ASSETS.iter().all(|asset| checked.contains(asset)) {
        problems.push("release-publish.yaml does not check the four release assets".to_owned());
    }
    let merge = job(publish, "assets")["steps"]
        .as_sequence()
        .into_iter()
        .flatten()
        .find(|step| text(&step["uses"]).starts_with("actions/upload-artifact/merge@"));
    let Some(merge) = merge else {
        return vec!["release-publish.yaml's assets job merges no artifact".to_owned()];
    };
    let name = text(&merge["with"]["name"]);
    if text(&job(publish, "publish")["with"]["artifact_name"]) != name || name.is_empty() {
        problems.push("the release does not attach the merged assets".to_owned());
    }
    if text(&merge["with"]["pattern"]) != "{gx52-bundle-*,flathub-files}" {
        problems.push(format!(
            "the release assets are {:?}, not the bundles and the Flathub files",
            merge["with"]["pattern"]
        ));
    }
    problems
}

/// No release workflow reaches a secret or an environment, and the Flathub check writes nothing
/// outside its run: the Flathub pull request is opened with the maintainer's own login
/// (docs/internal/release.md, "The Flathub pull requests"). The check is a called workflow only, so
/// master needs no stub of it.
fn token_problems(workflows: &Workflows) -> Vec<String> {
    let mut problems = Vec::new();
    for (file, workflow) in [
        (RELEASE, &workflows.release),
        (PUBLISH, &workflows.publish),
        (GATE, &workflows.gate),
        (FLATHUB, &workflows.flathub),
    ] {
        for (name, job) in jobs(workflow) {
            let rendered = serde_yaml_ng::to_string(job).unwrap_or_default();
            if rendered.contains("secrets.") || rendered.contains("secrets: inherit") {
                problems.push(format!("{file} {name}: it reaches a secret"));
            }
            if !job["environment"].is_null() {
                problems.push(format!("{file} {name}: it runs in an environment"));
            }
        }
    }
    for (name, job) in jobs(&workflows.flathub) {
        if job["permissions"] != parse("permissions", "contents: read") {
            problems.push(format!(
                "flathub-check.yaml {name}: its permissions are not contents: read"
            ));
        }
        let script = runs(job);
        if ["git push", "gh pr", "gh api"]
            .iter()
            .any(|write| script.contains(write))
        {
            problems.push(format!(
                "flathub-check.yaml {name}: it pushes or opens something"
            ));
        }
        let lints = job["steps"]
            .as_sequence()
            .into_iter()
            .flatten()
            .any(|step| {
                let run = text(&step["run"]);
                run.contains("flatpak-builder-lint manifest")
                    && !run.contains("|| true")
                    && step["continue-on-error"].is_null()
            });
        if name == "prepare" && !lints {
            problems.push("flathub-check.yaml does not lint the Flathub manifest".to_owned());
        }
    }
    let on = &workflows.flathub["on"];
    if on.as_mapping().map(Mapping::len) != Some(1) || on["workflow_call"].is_null() {
        problems.push("flathub-check.yaml runs on more than workflow_call".to_owned());
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
const STUB_INPUTS: &[(&str, &[StubInput])] = &[(
    RELEASE,
    &[
        ("version", "string", false, "", &[]),
        ("dry_run", "boolean", false, "true", &[]),
    ],
)];

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
        let workflow = match *file {
            RELEASE => &workflows.release,
            other => panic!("master carries no stub of {other}"),
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

/// The runs of a job's steps, joined.
fn runs(job: &Value) -> String {
    job["steps"]
        .as_sequence()
        .into_iter()
        .flatten()
        .filter_map(|step| step["run"].as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// A real run comes from the trunk only (release.yaml; the publish run is pushed there); the gate
/// builds the copy with the tests on and lints both the manifest and the repository.
fn guard_problems(workflows: &Workflows) -> Vec<String> {
    let mut problems = Vec::new();
    let policy = runs(job(&workflows.release, "policy"));
    if !policy.contains("[ \"$DRY_RUN\" != \"true\" ] && [ \"$REF\" != \"refs/heads/main\" ]") {
        problems.push("release.yaml's policy job lets a real run off the trunk".to_owned());
    }
    let build = job(&workflows.gate, "build");
    let gate_runs = runs(build);
    if !gate_runs.contains(r#". + {"run-tests": true}"#) {
        problems.push("the gate does not turn the app module's tests on".to_owned());
    }
    let builds_copy = build["steps"]
        .as_sequence()
        .into_iter()
        .flatten()
        .any(|step| {
            text(&step["uses"]).starts_with("flatpak/flatpak-github-actions/flatpak-builder@")
                && text(&step["with"]["manifest-path"]) == "build-aux/com.leinardi.gx52.gate.json"
                && step["with"]["run-tests"] == Value::Bool(true)
        });
    if !builds_copy {
        problems.push("the gate does not build the copy with the tests on".to_owned());
    }
    if !gate_runs.contains("TRUNK_URL")
        || !build["steps"]
            .as_sequence()
            .into_iter()
            .flatten()
            .any(|step| {
                text(&step["if"]) == "github.ref != 'refs/heads/main'"
                    && text(&step["env"]["TRUNK_URL"])
                        == "https://raw.githubusercontent.com/leinardi/gx52/main/"
            })
    {
        problems
            .push("the gate does not point the screenshots at the commit off the trunk".to_owned());
    }
    for lint in [
        "flatpak-builder-lint manifest build-aux/com.leinardi.gx52.json",
        "flatpak-builder-lint repo repo",
    ] {
        if !gate_runs.contains(lint) {
            problems.push(format!("the gate does not run `{lint}`"));
        }
    }
    problems
}

fn all_problems(workflows: &Workflows) -> Vec<String> {
    [
        version_problems(workflows),
        pin_problems(workflows),
        order_problems(workflows),
        token_problems(workflows),
        input_problems(workflows),
        guard_problems(workflows),
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

/// A path in the workflows, and the value that breaks a rule when put there.
type Break = (&'static str, Vec<&'static str>, Value);

/// Breaks of the consistency rules: values, names and pins.
fn value_breaks() -> Vec<Break> {
    let other_commit = format!("{REUSABLE}release-on-merge.yaml@{}", "1".repeat(40));
    vec![
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
            "a trunk other than the default branch for the bump",
            vec!["release", "jobs", "bump", "with", "base_branch"],
            "rust-rewrite".into(),
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
            "the dry run gates the unbumped tree",
            vec!["release", "jobs", "gate", "with"],
            Value::Null,
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
    ]
}

/// flathub-check's prepare steps, each passed through `change`.
fn prepare_steps(change: &dyn Fn(&mut Value)) -> Value {
    let mut steps = job(&workflows().flathub, "prepare")["steps"].clone();
    for step in steps.as_sequence_mut().into_iter().flatten() {
        change(step);
    }
    steps
}

/// Breaks of the Flathub file rules: the release without the Flathub files, or a Flathub check
/// that skips its lint.
fn flathub_breaks() -> Vec<Break> {
    vec![
        (
            "the tag before the Flathub check",
            vec!["publish", "jobs", "assets", "needs"],
            parse("needs", "[decide, gate]"),
        ),
        (
            "the bundles only",
            vec!["publish", "jobs", "publish", "with", "artifact_name"],
            "gx52-bundles".into(),
        ),
        (
            "the Flathub files left out",
            vec!["publish", "jobs", "assets", "steps"],
            parse(
                "steps",
                "- uses: actions/upload-artifact/merge@cf430e030ddbb5b0abf93d22962f4752f3646cd9\n  \
                 with: {name: release-assets, pattern: gx52-bundle-*}",
            ),
        ),
        (
            "the Flathub check of another tag",
            vec!["release", "jobs", "flathub", "with", "tag"],
            "v0.0.0".into(),
        ),
        (
            "the Flathub files renamed",
            vec!["flathub", "jobs", "prepare", "steps"],
            prepare_steps(&|step| {
                if text(&step["name"]) == "Upload the Flathub files" {
                    step["with"]["name"] = "flathub".into();
                }
            }),
        ),
        (
            "one Flathub file uploaded",
            vec!["flathub", "jobs", "prepare", "steps"],
            prepare_steps(&|step| {
                if text(&step["name"]) == "Upload the Flathub files" {
                    step["with"]["path"] = "${{ env.FILES }}/cargo-sources.json".into();
                }
            }),
        ),
        (
            "a lint that cannot fail",
            vec!["flathub", "jobs", "prepare", "steps"],
            prepare_steps(&|step| {
                if text(&step["name"]) == "Lint the Flathub manifest" {
                    step["continue-on-error"] = true.into();
                }
            }),
        ),
        (
            "a lint that ignores its status",
            vec!["flathub", "jobs", "prepare", "steps"],
            prepare_steps(&|step| {
                if text(&step["name"]) == "Lint the Flathub manifest" {
                    let run = format!("{} || true", text(&step["run"]));
                    step["run"] = run.into();
                }
            }),
        ),
        (
            "the asset list unchecked",
            vec!["publish", "jobs", "assets", "steps"],
            Value::Sequence(
                job(&workflows().publish, "assets")["steps"]
                    .as_sequence()
                    .into_iter()
                    .flatten()
                    .filter(|step| text(&step["name"]) != "Check the release assets")
                    .cloned()
                    .collect(),
            ),
        ),
        (
            "the bump's patch on the publish run",
            vec!["publish", "jobs", "flathub", "with", "patch_artifact"],
            "version-bump-pr-patch".into(),
        ),
    ]
}

/// Breaks of the no-token rules (a secret, an environment or write access anywhere, and a
/// dispatchable Flathub check), and a Flathub check with no lint step at all.
fn token_breaks() -> Vec<Break> {
    vec![
        (
            "a token in the Flathub check",
            vec!["flathub", "jobs", "prepare", "env"],
            parse("env", "T: ${{ secrets.FLATHUB_TOKEN }}"),
        ),
        (
            "an environment on the publish",
            vec!["publish", "jobs", "assets", "environment"],
            "release".into(),
        ),
        (
            "inherited secrets",
            vec!["publish", "jobs", "flathub", "secrets"],
            "inherit".into(),
        ),
        (
            "write access in the Flathub check",
            vec!["flathub", "jobs", "prepare", "permissions"],
            parse("permissions", "contents: write"),
        ),
        (
            "a dispatchable Flathub check",
            vec!["flathub", "on", "workflow_dispatch"],
            Value::Mapping(Mapping::new()),
        ),
        (
            "no lint of the Flathub manifest",
            vec!["flathub", "jobs", "prepare", "steps"],
            Value::Sequence(Vec::new()),
        ),
    ]
}

/// Breaks of the guards: a real run off the trunk, the gate without its tests or linters.
fn guard_breaks() -> Vec<Break> {
    let gate_steps = |drop: &str| {
        let steps = job(&workflows().gate, "build")["steps"].clone();
        Value::Sequence(
            steps
                .as_sequence()
                .into_iter()
                .flatten()
                .filter(|step| !text(&step["name"]).starts_with(drop))
                .cloned()
                .collect(),
        )
    };
    let without_first_step = |name: &str| {
        let workflows = workflows();
        let steps = job(&workflows.release, name)["steps"]
            .as_sequence()
            .cloned()
            .unwrap_or_default();
        Value::Sequence(steps.into_iter().skip(1).collect())
    };
    vec![
        (
            "a real release off the trunk",
            vec!["release", "jobs", "policy", "steps"],
            without_first_step("policy"),
        ),
        (
            "the gate without its tests",
            vec!["gate", "jobs", "build", "steps"],
            gate_steps("Write the gate manifest"),
        ),
        (
            "the gate without the linters",
            vec!["gate", "jobs", "build", "steps"],
            gate_steps("Lint the manifest"),
        ),
        (
            "the screenshots on the trunk in a dry run",
            vec!["gate", "jobs", "build", "steps"],
            gate_steps("Point the screenshots"),
        ),
    ]
}

/// Each mutation breaks one rule, and the checks see it.
#[test]
fn the_checks_catch_each_break() {
    let good = workflows();
    for (name, path, value) in value_breaks()
        .into_iter()
        .chain(flathub_breaks())
        .chain(token_breaks())
        .chain(guard_breaks())
    {
        let mut broken = good.clone();
        set(&mut broken, &path, value);
        assert!(!all_problems(&broken).is_empty(), "{name}: not caught");
    }
}

/// Every screenshot of the metainfo is under the trunk URL the gate rewrites off the trunk, so a
/// dry run of a branch mirrors that branch's screenshots.
#[test]
fn the_screenshots_follow_the_trunk_url() {
    let metainfo = repo::read("data/com.leinardi.gx52.metainfo.xml.in.in");
    let images: Vec<&str> = metainfo
        .lines()
        .filter_map(|line| {
            line.trim()
                .strip_prefix("<image>")?
                .strip_suffix("</image>")
        })
        .collect();
    assert!(!images.is_empty(), "the metainfo lists no screenshots");
    let trunk = "https://raw.githubusercontent.com/leinardi/gx52/main/data/screenshots/";
    for image in images {
        assert!(image.starts_with(trunk), "{image} is not under {trunk}");
    }
}
