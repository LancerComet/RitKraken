use std::path::{Path, PathBuf};

use crate::{
    FileTarget, Patch, Repo, Result, RuleError, Scope,
    command::{command, run_read, run_write},
    protocol::text,
    worktrees::registered_path_eq,
};

const SUBPROJECT_PREFIX: &[u8] = b"Subproject commit ";
const HEX_LEN: usize = 40;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmoduleChange {
    pub old: Option<String>,
    pub new: Option<String>,
    pub modified: bool,
    pub untracked: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SubmoduleInfo {
    pub change: SubmoduleChange,
    pub path: PathBuf,
    pub initialized: bool,
    pub old_subject: Option<String>,
    pub new_subject: Option<String>,
}

fn parse_subproject_line(line: &[u8]) -> Option<(u8, String, bool)> {
    let sign = *line.first()?;
    if sign != b'-' && sign != b'+' {
        return None;
    }
    let rest = line.get(1..)?.strip_prefix(SUBPROJECT_PREFIX)?;
    let (hex, dirty) = match rest.strip_suffix(b"-dirty") {
        Some(hex) => (hex, true),
        None => (rest, false),
    };
    if hex.len() != HEX_LEN || !hex.iter().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    Some((sign, text(hex), dirty))
}

fn header_allows_gitlink(header: &[u8]) -> bool {
    let mut has_index = false;
    for line in header.split(|byte| *byte == b'\n') {
        if line.starts_with(b"index ") {
            has_index = true;
            if line.ends_with(b" 160000") {
                return true;
            }
        }
        if line.starts_with(b"new file mode 160000")
            || line.starts_with(b"deleted file mode 160000")
            || line.starts_with(b"old mode 160000")
            || line.starts_with(b"new mode 160000")
        {
            return true;
        }
    }
    !has_index
}

pub(crate) fn parse_submodule_patch(
    bytes: &[u8],
) -> Option<(Option<String>, Option<String>, bool)> {
    let hunk_start = bytes
        .windows(3)
        .position(|window| window == b"@@ ")?;
    let header = &bytes[..hunk_start];
    if !header_allows_gitlink(header) {
        return None;
    }
    let body = &bytes[hunk_start..];
    let mut old = None;
    let mut new = None;
    let mut dirty = false;
    let mut seen_any = false;
    for line in body.split(|byte| *byte == b'\n') {
        if line.is_empty() || line.starts_with(b"@@ ") || line.starts_with(b"\\ ") {
            continue;
        }
        let (sign, id, is_dirty) = parse_subproject_line(line)?;
        match sign {
            b'-' => {
                if old.is_some() {
                    return None;
                }
                old = Some(id);
            }
            _ => {
                if new.is_some() {
                    return None;
                }
                new = Some(id);
                dirty = is_dirty;
            }
        }
        seen_any = true;
    }
    seen_any.then_some((old, new, dirty))
}

fn parse_status_submodule(status: &[u8]) -> Option<(String, bool, bool)> {
    for record in status.split(|byte| *byte == 0) {
        let mut fields = record.split(|byte| *byte == b' ');
        if fields.next() != Some(b"1") {
            continue;
        }
        fields.next();
        let sub = fields.next()?;
        if sub.first() != Some(&b'S') {
            continue;
        }
        fields.next();
        fields.next();
        fields.next();
        fields.next();
        let index_oid = text(fields.next().unwrap_or_default());
        if index_oid.len() != HEX_LEN {
            continue;
        }
        let modified = sub.get(2) == Some(&b'M');
        let untracked = sub.get(3) == Some(&b'U');
        return Some((index_oid, modified, untracked));
    }
    None
}

fn commit_subject(dir: &Path, id: &str) -> Option<String> {
    let mut git = command(Some(dir));
    git.args(["log", "-1", "--format=%s", id, "--"]);
    let bytes = run_read(&mut git).ok()?;
    let subject = text(&bytes).trim().to_owned();
    (!subject.is_empty()).then_some(subject)
}

impl Repo {
    pub fn submodule_change(
        &self,
        file: &FileTarget,
        patch: &Patch,
    ) -> Result<Option<SubmoduleChange>> {
        let parsed = parse_submodule_patch(patch.bytes());
        if !matches!(file.scope, Scope::Worktree) {
            return Ok(parsed.map(|(old, new, _)| SubmoduleChange {
                old,
                new,
                modified: false,
                untracked: false,
            }));
        }
        let dir = self.root().join(&file.change.path);
        if !dir.is_dir() {
            return Ok(parsed.map(|(old, new, dirty)| SubmoduleChange {
                old,
                new,
                modified: dirty,
                untracked: false,
            }));
        }
        let mut git = self.git_with_literal_paths();
        git.args([
            "status",
            "--porcelain=v2",
            "-z",
            "--ignore-submodules=none",
            "--",
        ])
        .arg(&file.change.path);
        let status = run_read(&mut git)?;
        let parsed_new = parsed.as_ref().and_then(|(_, new, _)| new.clone());
        if let Some((index_oid, modified, untracked)) = parse_status_submodule(&status) {
            return Ok(Some(SubmoduleChange {
                old: Some(index_oid.clone()),
                new: Some(parsed_new.unwrap_or(index_oid)),
                modified,
                untracked,
            }));
        }
        Ok(parsed.map(|(old, new, dirty)| SubmoduleChange {
            old,
            new,
            modified: dirty,
            untracked: false,
        }))
    }

    fn submodule_repo(&self, path: &Path) -> Option<Repo> {
        let dir = self.root().join(path).canonicalize().ok()?;
        let repo = Repo::open(&dir).ok()?;
        let root = repo.root().canonicalize().ok()?;
        registered_path_eq(&root, &dir).then_some(repo)
    }

    pub fn submodule_info(&self, path: &Path, change: SubmoduleChange) -> SubmoduleInfo {
        let dir = self.root().join(path);
        let sub = self.submodule_repo(path);
        let (old_subject, new_subject) = match &sub {
            Some(sub) => (
                change
                    .old
                    .as_deref()
                    .and_then(|id| commit_subject(sub.root(), id)),
                change
                    .new
                    .as_deref()
                    .and_then(|id| commit_subject(sub.root(), id)),
            ),
            None => (None, None),
        };
        SubmoduleInfo {
            change,
            path: dir,
            initialized: sub.is_some(),
            old_subject,
            new_subject,
        }
    }

    fn recorded_submodule_id(&self, path: &Path) -> Result<Option<String>> {
        let mut git = self.git_with_literal_paths();
        git.args(["ls-files", "--stage", "--"]).arg(path);
        let output = run_read(&mut git)?;
        for record in output.split(|byte| *byte == b'\n') {
            let mut fields = record.split(|byte| *byte == b' ');
            if fields.next() == Some(b"160000") {
                return Ok(Some(text(fields.next().unwrap_or_default())));
            }
        }
        Ok(None)
    }

    pub fn update_submodule(&self, path: &Path) -> Result<()> {
        if let Some(sub) = self.submodule_repo(path)
            && let Some(recorded) = self.recorded_submodule_id(path)?
        {
            let head = text(&sub.read(&["rev-parse", "HEAD"])?)
                .trim()
                .to_owned();
            if head != recorded {
                let containing = sub.read(&[
                    "for-each-ref",
                    "--contains",
                    "HEAD",
                    "--count=1",
                    "--format=%(refname)",
                ])?;
                if text(&containing).trim().is_empty() {
                    return Err(RuleError::UnreachableSubmoduleCommit.into());
                }
            }
        }
        let mut git = self.git_with_literal_paths();
        git.args(["submodule", "update", "--init", "--"]).arg(path);
        run_write(&mut git)
    }
}
