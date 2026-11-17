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

//! Drives scripts/flathub-manifest.sh on this repository's stable manifest and on altered copies of
//! it: the app's `dir` source becomes the tagged git source (or, for a dry run, a `dir` source at
//! the given path) and nothing else changes; a manifest it cannot migrate unambiguously, and a
//! malformed tag or commit, are refused.

use meta::repo;
use meta::scratch::{Run, Scratch};
use serde_yaml_ng::{Mapping, Value};

const SCRIPT: &str = "scripts/flathub-manifest.sh";
const STABLE: &str = "build-aux/com.leinardi.gx52.json";
const TAG: &str = "v0.99.0";
const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

/// JSON is YAML, so the YAML reader the other meta-tests use parses the manifests.
fn parse(json: &str) -> Value {
    serde_yaml_ng::from_str(json).unwrap_or_else(|error| panic!("parse {json}: {error}"))
}

fn stable() -> Value {
    parse(&repo::read(STABLE))
}

fn run(manifest: &Value, args: &[&str]) -> Run {
    let scratch = Scratch::new();
    scratch.copy_script(SCRIPT);
    scratch.write(
        "manifest.json",
        &to_json(manifest).unwrap_or_else(|| panic!("a manifest holds plain JSON values")),
    );
    let mut all = vec!["manifest.json"];
    all.extend_from_slice(args);
    scratch.run(SCRIPT, &all, &[])
}

/// A JSON rendering of `value` (strings, sequences, mappings and booleans are all a manifest
/// holds).
fn to_json(value: &Value) -> Option<String> {
    Some(match value {
        Value::Null => "null".to_owned(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => number.to_string(),
        Value::String(text) => json_string(text),
        Value::Sequence(items) => {
            let items: Option<Vec<String>> = items.iter().map(to_json).collect();
            format!("[{}]", items?.join(","))
        }
        Value::Mapping(map) => {
            let entries: Option<Vec<String>> = map
                .iter()
                .map(|(key, value)| Some(format!("{}:{}", to_json(key)?, to_json(value)?)))
                .collect();
            format!("{{{}}}", entries?.join(","))
        }
        Value::Tagged(_) => return None,
    })
}

fn json_string(text: &str) -> String {
    let mut out = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn migrated(manifest: &Value, args: &[&str]) -> Value {
    let result = run(manifest, args);
    assert!(result.success, "refused, want migrated:\n{}", result.text);
    parse(&result.text)
}

fn sources(manifest: &Value) -> &Vec<Value> {
    manifest["modules"][0]["sources"]
        .as_sequence()
        .unwrap_or_else(|| panic!("the gx52 module has a sources list"))
}

/// Every `dir` source anywhere in `value`.
fn dir_sources(value: &Value) -> usize {
    match value {
        Value::Mapping(map) => {
            let own = usize::from(map.get("type").and_then(Value::as_str) == Some("dir"));
            own + map.values().map(dir_sources).sum::<usize>()
        }
        Value::Sequence(items) => items.iter().map(dir_sources).sum(),
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) | Value::Tagged(_) => 0,
    }
}

fn mapping(entries: &[(&str, Value)]) -> Value {
    let mut map = Mapping::new();
    for (key, value) in entries {
        map.insert((*key).into(), value.clone());
    }
    Value::Mapping(map)
}

fn git_source() -> Value {
    mapping(&[
        ("type", "git".into()),
        ("url", "https://github.com/leinardi/gx52.git".into()),
        ("tag", TAG.into()),
        ("commit", COMMIT.into()),
    ])
}

/// The stable manifest with its `dir` source replaced by `source`: what the script must produce.
fn expected(source: Value) -> Value {
    let mut manifest = stable();
    let sources = manifest["modules"][0]["sources"]
        .as_sequence_mut()
        .unwrap_or_else(|| panic!("sources"));
    let dir = sources
        .iter()
        .position(|source| source["type"].as_str() == Some("dir"))
        .unwrap_or_else(|| panic!("the stable manifest has a dir source"));
    sources[dir] = source;
    manifest
}

#[test]
fn the_stable_manifest_is_migrated_to_the_tag() {
    let input = stable();
    assert_eq!(
        dir_sources(&input),
        1,
        "the stable manifest builds from one dir source"
    );
    let output = migrated(&input, &[TAG, COMMIT]);

    assert_eq!(dir_sources(&output), 0, "a dir source is left");
    assert_eq!(
        output,
        expected(git_source()),
        "anything but the app source changed"
    );
    // Spelled out, the parts Flathub's build depends on.
    assert_eq!(sources(&output)[0], git_source());
    assert!(
        sources(&output).contains(&Value::from("cargo-sources.json")),
        "the cargo-sources.json entry is gone"
    );
    let vendoring = sources(&input)
        .iter()
        .find(|source| source["type"].as_str() == Some("inline"))
        .unwrap_or_else(|| panic!("the stable manifest vendors the crates"));
    assert!(
        sources(&output).contains(vendoring),
        "the vendoring source is gone"
    );
    assert_eq!(output["finish-args"], input["finish-args"], "finish-args");
    assert_eq!(
        output["runtime-version"], input["runtime-version"],
        "runtime-version"
    );
    assert_eq!(
        output["sdk-extensions"], input["sdk-extensions"],
        "sdk-extensions"
    );
}

#[test]
fn a_dry_run_keeps_a_dir_source_at_the_given_path() {
    let output = migrated(&stable(), &["--source-dir", "../gx52"]);
    let mut dir = sources(&stable())
        .iter()
        .find(|source| source["type"].as_str() == Some("dir"))
        .cloned()
        .unwrap_or_else(|| panic!("the stable manifest has a dir source"));
    assert!(
        dir.get("skip").is_some(),
        "the stable dir source has no skip list to keep"
    );
    dir["path"] = "../gx52".into();
    assert_eq!(
        output,
        expected(dir),
        "only the dir source's path may change"
    );
}

#[test]
fn ambiguous_manifests_and_malformed_inputs_are_refused() {
    let with = |change: &dyn Fn(&mut Value)| {
        let mut manifest = stable();
        change(&mut manifest);
        manifest
    };
    let second_dir = with(&|manifest| {
        let dir = sources(&stable())[0].clone();
        manifest["modules"][0]["sources"]
            .as_sequence_mut()
            .unwrap_or_else(|| panic!("sources"))
            .push(dir);
    });
    let no_dir = with(&|manifest| {
        manifest["modules"][0]["sources"]
            .as_sequence_mut()
            .unwrap_or_else(|| panic!("sources"))
            .remove(0);
    });
    let renamed = with(&|manifest| manifest["modules"][0]["name"] = "app".into());
    let two_apps = with(&|manifest| {
        let module = manifest["modules"][0].clone();
        manifest["modules"]
            .as_sequence_mut()
            .unwrap_or_else(|| panic!("modules"))
            .push(module);
    });
    let stray_dir = with(&|manifest| {
        let mut module = Mapping::new();
        module.insert("name".into(), "helper".into());
        module.insert(
            "sources".into(),
            Value::Sequence(vec![sources(&stable())[0].clone()]),
        );
        manifest["modules"]
            .as_sequence_mut()
            .unwrap_or_else(|| panic!("modules"))
            .insert(0, Value::Mapping(module));
    });

    for (case, manifest, args, reason) in [
        (
            "two dir sources",
            &second_dir,
            [TAG, COMMIT],
            "2 dir sources",
        ),
        ("no dir source", &no_dir, [TAG, COMMIT], "0 dir sources"),
        ("no gx52 module", &renamed, [TAG, COMMIT], "0 gx52 modules"),
        (
            "two gx52 modules",
            &two_apps,
            [TAG, COMMIT],
            "2 gx52 modules",
        ),
        (
            "a dir source elsewhere",
            &stray_dir,
            [TAG, COMMIT],
            "left outside",
        ),
        (
            "a tag without v",
            &stable(),
            ["0.99.0", COMMIT],
            "must be vX.Y.Z",
        ),
        (
            "a pre-release tag",
            &stable(),
            ["v0.99.0-rc.1", COMMIT],
            "must be vX.Y.Z",
        ),
        ("a short commit", &stable(), [TAG, "0123456"], "full 40-hex"),
        (
            "an empty source dir",
            &stable(),
            ["--source-dir", ""],
            "needs a path",
        ),
    ] {
        let result = run(manifest, &args);
        assert!(
            !result.success,
            "{case}: migrated, want refused:\n{}",
            result.text
        );
        assert!(
            result.text.contains(reason),
            "{case}: want '{reason}', got:\n{}",
            result.text
        );
    }
}
