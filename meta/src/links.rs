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
// Adapted from swarm-device-access/meta/src/links.rs at 4cd31c4, originally Apache-2.0-licensed,
// Copyright (c) 2026 Roberto Leinardi

//! Checks the Markdown documentation offline: every relative link and fragment resolves. It needs
//! no network and no tool beyond cargo and git, so it runs as the docs-links pre-commit hook.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

fn regex(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap_or_else(|error| panic!("invalid pattern {pattern}: {error}"))
}

static INLINE_LINK: LazyLock<Regex> =
    LazyLock::new(|| regex(r#"!?\[[^\]]*\]\(\s*(<[^>]*>|[^)\s]+)(?:\s+"[^"]*")?\s*\)"#));
static REFERENCE_DEF: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^\s{0,3}\[[^\]]+\]:\s*(<[^>]*>|\S+)"));
static ATX_HEADING: LazyLock<Regex> = LazyLock::new(|| regex(r"^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$"));
static HTML_ANCHOR: LazyLock<Regex> = LazyLock::new(|| regex(r#"<a\s+(?:id|name)="([^"]+)""#));
static CODE_SPAN: LazyLock<Regex> = LazyLock::new(|| regex("`+[^`]*`+"));
static LINK_IN_HEADING: LazyLock<Regex> = LazyLock::new(|| regex(r"!?\[([^\]]*)\]\([^)]*\)"));
static HTML_TAG: LazyLock<Regex> = LazyLock::new(|| regex(r"<[^>]+>"));
static EDGE_UNDERLINE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(^|[^\p{L}\p{N}])_+|_+([^\p{L}\p{N}]|$)"));
static URI_SCHEME: LazyLock<Regex> = LazyLock::new(|| regex(r"^[a-zA-Z][a-zA-Z0-9+.-]*:"));
/// The characters a GitHub heading slug keeps, besides the space it turns into a hyphen: letters,
/// marks, numbers, connector punctuation and hyphens.
static SLUG_KEPT: LazyLock<Regex> = LazyLock::new(|| regex(r"^[\p{L}\p{M}\p{N}\p{Pc}\-]$"));

/// One broken link, reported against the file and line that contains it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Problem {
    /// The Markdown file, relative to the root.
    pub file: String,
    /// The line, counted from 1.
    pub line: usize,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for Problem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}: {}", self.file, self.line, self.message)
    }
}

/// The lines of a Markdown file that are prose, with fenced code blocks blanked out so line
/// numbers still match the file.
#[must_use]
pub fn lines(content: &str) -> Vec<String> {
    let mut lines: Vec<String> = content.split('\n').map(str::to_owned).collect();
    let mut fence = "";
    for line in &mut lines {
        let trimmed = line.trim().to_owned();
        if !fence.is_empty() {
            if trimmed.starts_with(fence) {
                fence = "";
            }
            line.clear();
            continue;
        }
        for marker in ["```", "~~~"] {
            if trimmed.starts_with(marker) {
                fence = marker;
                line.clear();
            }
        }
    }
    lines
}

/// The fragments a Markdown file defines: its headings under GitHub's slug rules, numbered on
/// repetition, and any explicit HTML anchors.
#[must_use]
pub fn anchors(content: &str) -> HashSet<String> {
    let mut anchors = HashSet::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for line in lines(content) {
        for captures in HTML_ANCHOR.captures_iter(&line) {
            anchors.insert(captures[1].to_owned());
        }
        let Some(heading) = ATX_HEADING.captures(&line) else {
            continue;
        };
        let slug = slug(&heading[2]);
        let count = seen.entry(slug.clone()).or_insert(0);
        if *count > 0 {
            anchors.insert(format!("{slug}-{count}"));
        } else {
            anchors.insert(slug);
        }
        *count += 1;
    }
    anchors
}

/// The anchor GitHub generates for a heading: the rendered text, lower-cased, with everything but
/// letters, marks, numbers, connector punctuation, spaces and hyphens removed, and each space
/// turned into a hyphen.
#[must_use]
pub fn slug(heading: &str) -> String {
    render_text(heading)
        .to_lowercase()
        .chars()
        .filter_map(|character| {
            if character == ' ' {
                return Some('-');
            }
            let mut buffer = [0; 4];
            SLUG_KEPT
                .is_match(character.encode_utf8(&mut buffer))
                .then_some(character)
        })
        .collect()
}

/// Strips the Markdown syntax that does not reach the rendered heading text: link targets, HTML
/// tags, emphasis markers and code-span backticks (keeping the code itself).
fn render_text(heading: &str) -> String {
    let mut rendered = String::new();
    let mut last = 0;
    for span in CODE_SPAN.find_iter(heading) {
        rendered.push_str(&strip_inline(&heading[last..span.start()]));
        rendered.push_str(span.as_str().trim_matches('`'));
        last = span.end();
    }
    rendered.push_str(&strip_inline(&heading[last..]));
    rendered
}

fn strip_inline(text: &str) -> String {
    let text = LINK_IN_HEADING.replace_all(text, "${1}");
    let text = HTML_TAG.replace_all(&text, "");
    let text = text.replace('*', "");
    EDGE_UNDERLINE.replace_all(&text, "${1}${2}").into_owned()
}

/// The link targets on one prose line, inline code spans excluded.
#[must_use]
pub fn targets(line: &str) -> Vec<String> {
    let line = CODE_SPAN.replace_all(line, "");
    let mut targets: Vec<String> = INLINE_LINK
        .captures_iter(&line)
        .map(|captures| captures[1].trim_matches(['<', '>']).to_owned())
        .collect();
    if let Some(captures) = REFERENCE_DEF.captures(&line) {
        targets.push(captures[1].trim_matches(['<', '>']).to_owned());
    }
    targets
}

/// Resolves links against a repository root.
#[derive(Debug)]
pub struct Checker {
    root: PathBuf,
    anchors: HashMap<String, HashSet<String>>,
}

impl Checker {
    /// A checker for the repository at `root`.
    #[must_use]
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
            anchors: HashMap::new(),
        }
    }

    /// Checks every relative link in the Markdown file at `relative`, a slash-separated path
    /// relative to the root. Paths are resolved logically, against the file's own directory, and
    /// then followed through any symlink on disk.
    ///
    /// # Errors
    ///
    /// The file cannot be read.
    pub fn check_file(&mut self, relative: &str) -> io::Result<Vec<Problem>> {
        let content = fs::read_to_string(self.root.join(relative))?;
        let mut problems = Vec::new();
        for (index, line) in lines(&content).iter().enumerate() {
            for target in targets(line) {
                if let Some(message) = self.check_target(relative, &target) {
                    problems.push(Problem {
                        file: relative.to_owned(),
                        line: index + 1,
                        message,
                    });
                }
            }
        }
        Ok(problems)
    }

    fn check_target(&mut self, relative: &str, target: &str) -> Option<String> {
        if URI_SCHEME.is_match(target) || target.starts_with("//") {
            return None;
        }

        let (file, fragment) = target.split_once('#').unwrap_or((target, ""));
        let mut resolved = relative.to_owned();
        if !file.is_empty() {
            resolved = match file.strip_prefix('/') {
                Some(absolute) => clean(absolute),
                None => match relative.rsplit_once('/') {
                    Some((directory, _)) => clean(&format!("{directory}/{file}")),
                    None => clean(file),
                },
            };
            if resolved == ".." || resolved.starts_with("../") {
                return Some(format!("link {target:?} leaves the repository"));
            }
            if fs::metadata(self.root.join(&resolved)).is_err() {
                return Some(format!("link {target:?}: {resolved} does not exist"));
            }
        }

        let markdown = Path::new(&resolved)
            .extension()
            .is_some_and(|extension| extension == "md");
        if fragment.is_empty() || !markdown {
            return None;
        }
        match self.anchors_of(&resolved) {
            Ok(anchors) if anchors.contains(fragment) => None,
            Ok(_) => Some(format!(
                "link {target:?}: {resolved} has no heading or anchor #{fragment}"
            )),
            Err(error) => Some(format!("link {target:?}: read {resolved}: {error}")),
        }
    }

    fn anchors_of(&mut self, relative: &str) -> io::Result<&HashSet<String>> {
        if !self.anchors.contains_key(relative) {
            let content = fs::read_to_string(self.root.join(relative))?;
            self.anchors.insert(relative.to_owned(), anchors(&content));
        }
        Ok(&self.anchors[relative])
    }
}

/// Go's `path.Clean` for a relative slash-separated path: `.` segments dropped, `..` applied
/// lexically, leading `..` kept.
fn clean(path: &str) -> String {
    let mut segments: Vec<&str> = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(segment) => segments.push(segment.to_str().unwrap_or_default()),
            Component::ParentDir => {
                if segments.last().is_some_and(|last| *last != "..") {
                    segments.pop();
                } else {
                    segments.push("..");
                }
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
        }
    }
    if segments.is_empty() {
        ".".to_owned()
    } else {
        segments.join("/")
    }
}
