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

//! Drives scripts/flathub-release.sh against local stand-ins: a bare repository for
//! flathub/com.leinardi.gx52 holding the legacy tree, an upstream repository with release tags,
//! and a directory of release assets served through file://. The pull request branch carries
//! exactly the release's files, `beta` is created once from the passed test build, and a manifest
//! that does not name the tag's commit is refused before anything is pushed.

use meta::repo;
use meta::scratch::{Run, Scratch};

const SCRIPT: &str = "scripts/flathub-release.sh";
const FILES: [&str; 2] = [
    "build-aux/flathub/flathub.json",
    "build-aux/flathub/update.yml",
];
const MANIFEST: &str = "com.leinardi.gx52.json";

/// The legacy Python manifest: no x-checker-data.
const LEGACY_MANIFEST: &str =
    r#"{"id": "com.leinardi.gx52", "modules": ["python3-setuptools.json"]}"#;

/// The release tree's files, in `git ls-files` order.
const RELEASE_TREE: [&str; 5] = [
    ".github/workflows/update.yml",
    "README.md",
    "cargo-sources.json",
    "com.leinardi.gx52.json",
    "flathub.json",
];

struct Setup {
    /// The gx52 checkout the script runs from.
    gx52: Scratch,
    upstream: Scratch,
    /// The working repository behind the bare Flathub remote, holding its legacy tree.
    legacy: Scratch,
    /// Holds `flathub.git`, the clones directory and the release assets.
    outside: tempfile::TempDir,
}

impl Setup {
    fn new() -> Self {
        let gx52 = Scratch::new();
        gx52.copy_script(SCRIPT);
        for file in FILES {
            gx52.write(file, &repo::read(file));
        }

        let legacy = Scratch::new();
        legacy.git(&["switch", "--quiet", "--create", "master"]);
        legacy.write("README.md", "Flathub packaging of GX52.\n");
        legacy.write(MANIFEST, LEGACY_MANIFEST);
        legacy.write("pypi-dependencies.json", "{}\n");
        legacy.write("flathub.json", r#"{"only-arches": ["x86_64"]}"#);
        legacy.commit("Update to 0.7.6");

        let outside = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary dir: {error}"));
        let setup = Self {
            gx52,
            upstream: Scratch::new(),
            legacy,
            outside,
        };
        setup
            .legacy
            .git(&["clone", "--quiet", "--bare", ".", &setup.remote()]);
        setup
    }

    fn outside(&self, relative: &str) -> String {
        self.outside.path().join(relative).display().to_string()
    }

    fn remote(&self) -> String {
        self.outside("flathub.git")
    }

    /// Tags a new upstream commit `tag` and publishes its assets; `commit` overrides the commit the
    /// manifest names.
    fn release(&self, tag: &str, commit: Option<&str>) {
        self.upstream.write("version", tag);
        self.upstream.commit(tag);
        self.upstream.tag(tag);
        let head = self.upstream.git_stdout(&["rev-parse", "HEAD"]);
        let manifest = format!(
            r#"{{"id": "com.leinardi.gx52", "modules": [{{"name": "gx52", "sources": [
                {{"type": "git", "url": "https://github.com/leinardi/gx52.git", "tag": "{tag}",
                  "commit": "{}", "x-checker-data": {{"type": "git"}}}},
                "cargo-sources.json"]}}]}}"#,
            commit.unwrap_or(&head)
        );
        let assets = self.outside.path().join("releases/download").join(tag);
        std::fs::create_dir_all(&assets).unwrap_or_else(|error| panic!("assets dir: {error}"));
        for (name, content) in [
            (MANIFEST, manifest.as_str()),
            (
                "cargo-sources.json",
                r#"[{"type": "file", "dest": "cargo/vendor"}]"#,
            ),
        ] {
            std::fs::write(assets.join(name), content)
                .unwrap_or_else(|error| panic!("write {name}: {error}"));
        }
    }

    fn run(&self, args: &[&str]) -> Run {
        let releases = format!("file://{}", self.outside("releases"));
        self.gx52.run(
            SCRIPT,
            args,
            &[
                ("FLATHUB_DIR", &self.outside("clones")),
                ("FLATHUB_URL", &self.remote()),
                ("UPSTREAM_URL", &self.upstream.path().display().to_string()),
                ("RELEASES_URL", &releases),
                ("GIT_AUTHOR_NAME", "Maintainer"),
                ("GIT_AUTHOR_EMAIL", "maintainer@example.com"),
                ("GIT_COMMITTER_NAME", "Maintainer"),
                ("GIT_COMMITTER_EMAIL", "maintainer@example.com"),
            ],
        )
    }

    fn succeeds(&self, args: &[&str]) -> String {
        let run = self.run(args);
        assert!(run.success, "{args:?} failed:\n{}", run.text);
        run.text
    }

    fn refused(&self, args: &[&str], reason: &str) {
        let run = self.run(args);
        assert!(
            !run.success,
            "{args:?} succeeded, want refused:\n{}",
            run.text
        );
        assert!(
            run.text.contains(reason),
            "{args:?}: want '{reason}', got:\n{}",
            run.text
        );
    }

    /// Runs git in the bare remote.
    fn remote_git(&self, args: &[&str]) -> String {
        let remote = self.remote();
        let mut all = vec!["--git-dir", remote.as_str()];
        all.extend_from_slice(args);
        self.legacy.git_stdout(&all)
    }

    fn branches(&self) -> Vec<String> {
        self.remote_git(&["for-each-ref", "--format=%(refname:short)", "refs/heads"])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn files(&self, branch: &str) -> Vec<String> {
        self.remote_git(&["ls-tree", "-r", "--name-only", branch])
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn show(&self, branch: &str, file: &str) -> String {
        self.remote_git(&["show", &format!("{branch}:{file}")])
    }
}

#[test]
fn the_migration_branch_carries_exactly_the_release() {
    let setup = Setup::new();
    setup.release("v0.99.0", None);
    let output = setup.succeeds(&["pr", "0.99.0", "master"]);

    let branch = "gx52/v0.99.0-master";
    assert_eq!(setup.branches(), [branch, "master"], "pushed branches");
    assert_eq!(setup.files(branch), RELEASE_TREE, "the branch's tree");
    let assets = setup.outside("releases/download/v0.99.0");
    for (file, want) in [
        (MANIFEST, format!("{assets}/{MANIFEST}")),
        ("cargo-sources.json", format!("{assets}/cargo-sources.json")),
    ] {
        let want = std::fs::read_to_string(&want).unwrap_or_else(|error| panic!("{want}: {error}"));
        assert_eq!(setup.show(branch, file), want.trim_end(), "{file}");
    }
    for (file, source) in [
        ("flathub.json", FILES[0]),
        (".github/workflows/update.yml", FILES[1]),
    ] {
        assert_eq!(
            setup.show(branch, file),
            repo::read(source).trim_end(),
            "{file}"
        );
    }
    assert_eq!(
        setup.show(branch, "README.md"),
        "Flathub packaging of GX52.",
        "README.md is master's"
    );
    assert_eq!(
        setup.remote_git(&["rev-parse", &format!("{branch}~1")]),
        setup.remote_git(&["rev-parse", "master"]),
        "the branch is one commit on master"
    );
    assert!(
        output.contains(
            "https://github.com/flathub/com.leinardi.gx52/compare/master...gx52/v0.99.0-master?quick_pull=1&title=Migrate%20to%20the%20Rust%20rewrite%20%28v0.99.0%29"
        ),
        "no filled-in pull request link:\n{output}"
    );
    assert!(
        output.contains("not%20to%20be%20merged%20into%20master"),
        "{output}"
    );
    assert!(
        output.contains("body=Release%3A%20https%3A%2F%2Fgithub.com%2Fleinardi%2Fgx52%2Freleases%2Ftag%2Fv0.99.0%0A"),
        "the body does not link the GitHub release:\n{output}"
    );
    assert!(
        output.contains("make flathub-beta VERSION=0.99.0"),
        "{output}"
    );

    // A second run rebuilds the same branch: still one commit on master.
    setup.succeeds(&["pr", "0.99.0", "master"]);
    assert_eq!(
        setup.remote_git(&["rev-list", "--count", &format!("master..{branch}")]),
        "1",
        "a re-run stacked a commit"
    );
}

#[test]
fn beta_is_created_once_from_the_test_build() {
    let setup = Setup::new();
    setup.release("v0.99.0", None);
    setup.refused(&["beta", "0.99.0"], "run 'pr 0.99.0 master' first");
    setup.refused(&["pr", "0.99.0", "beta"], "has no beta branch yet");

    setup.succeeds(&["pr", "0.99.0", "master"]);
    setup.succeeds(&["beta", "0.99.0"]);
    assert_eq!(
        setup.remote_git(&["rev-parse", "beta"]),
        setup.remote_git(&["rev-parse", "gx52/v0.99.0-master"]),
        "beta is master with the test build merged"
    );
    assert_eq!(
        setup.files("master"),
        [
            "README.md",
            MANIFEST,
            "flathub.json",
            "pypi-dependencies.json"
        ]
    );
    setup.refused(&["beta", "0.99.0"], "already has a beta branch");

    // Nothing changes for the release beta already has; the next one is an update.
    let same = setup.succeeds(&["pr", "0.99.0", "beta"]);
    assert!(same.contains("Nothing to change"), "{same}");
    assert!(!setup.branches().contains(&"gx52/v0.99.0-beta".to_owned()));
    setup.release("v0.99.1", None);
    let update = setup.succeeds(&["pr", "0.99.1", "beta"]);
    assert!(
        update
            .contains("compare/beta...gx52/v0.99.1-beta?quick_pull=1&title=Update%20to%20v0.99.1&"),
        "{update}"
    );
    assert_eq!(setup.files("gx52/v0.99.1-beta"), RELEASE_TREE);
}

#[test]
fn the_workflow_branch_adds_only_update_yml() {
    let setup = Setup::new();
    let output = setup.succeeds(&["workflow"]);
    let mut want: Vec<String> = setup.files("master");
    want.insert(0, ".github/workflows/update.yml".to_owned());
    assert_eq!(setup.files("gx52/update-workflow"), want);
    assert!(
        output.contains("compare/master...gx52/update-workflow?quick_pull=1&title=Add%20the%20update%20workflow&"),
        "{output}"
    );
}

#[test]
fn bad_inputs_and_mismatched_assets_are_refused() {
    let setup = Setup::new();
    setup.release("v0.99.0", Some("0123456789abcdef0123456789abcdef01234567"));
    setup.refused(&["pr", "0.99", "master"], "must be X.Y.Z");
    setup.refused(&["pr", "v0.99.0", "master"], "must be X.Y.Z");
    setup.refused(&["pr", "0.99.0", "stable"], "must be beta or master");
    setup.refused(&["pr", "0.98.0", "master"], "does not exist");
    setup.refused(&["pr", "0.99.0", "master"], "does not name v0.99.0");
    setup.refused(&["publish"], "usage");
    assert_eq!(setup.branches(), ["master"], "a refused run pushed");

    setup.release("v0.99.1", None);
    setup.succeeds(&["pr", "0.99.1", "master"]);
    let clone = setup.outside("clones/com.leinardi.gx52");
    std::fs::write(format!("{clone}/stray"), "").unwrap_or_else(|error| panic!("{error}"));
    setup.refused(&["pr", "0.99.1", "master"], "has uncommitted changes");
}
