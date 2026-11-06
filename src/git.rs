use std::{
    io::Write,
    ops::Range,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
};

pub type Result<T> = std::result::Result<T, String>;

#[derive(Clone, Debug)]
pub struct Repo {
    pub root: PathBuf,
}

#[derive(Clone, Debug)]
pub struct Commit {
    pub id: String,
    pub parents: Vec<String>,
    pub author: String,
    pub date: String,
    pub subject: String,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RefKind {
    Local,
    Remote,
    Tag,
}

#[derive(Clone, Debug)]
pub struct Reference {
    pub kind: RefKind,
    pub name: String,
    pub id: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Change {
    pub path: PathBuf,
    pub previous: Option<PathBuf>,
    pub code: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub unstaged: Vec<Change>,
    pub staged: Vec<Change>,
}

#[derive(Debug)]
pub struct Snapshot {
    pub branch: String,
    pub head: Option<String>,
    pub status: Status,
    pub refs: Vec<Reference>,
    pub commits: Vec<Commit>,
    pub history_limited: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Scope {
    Worktree,
    Index,
    Commit { id: String, parent: Option<String> },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileTarget {
    pub scope: Scope,
    pub change: Change,
}

#[derive(Debug)]
pub struct CommitDetail {
    pub message: String,
    pub changes: Vec<Change>,
}

#[derive(Debug)]
pub struct Patch {
    bytes: Vec<u8>,
    hunks: Vec<Range<usize>>,
    header: usize,
}

const HISTORY_LIMIT: usize = 100_000;
pub const PATCH_VIEW_LIMIT: usize = 8 * 1024 * 1024;
const PATCH_OPTIONS: &[&str] = &[
    "--no-color",
    "--no-ext-diff",
    "--no-textconv",
    "--src-prefix=a/",
    "--dst-prefix=b/",
    "--unified=3",
];

impl Repo {
    pub fn open(path: &Path) -> Result<Self> {
        let root = command(Some(path))
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .map_err(process_error)
            .and_then(checked)?;
        Ok(Self {
            root: path_from_bytes(without_final_newline(&root)),
        })
    }

    pub fn name(&self) -> String {
        self.root
            .file_name()
            .unwrap_or(self.root.as_os_str())
            .to_string_lossy()
            .into_owned()
    }

    fn git(&self) -> Command {
        command(Some(&self.root))
    }

    fn read(&self, args: &[&str]) -> Result<Vec<u8>> {
        run_read(self.git().args(args))
    }

    fn optional(&self, args: &[&str]) -> Result<Option<String>> {
        let output = self
            .git()
            .env("GIT_OPTIONAL_LOCKS", "0")
            .args(args)
            .output()
            .map_err(process_error)?;
        if output.status.code() == Some(1) {
            return Ok(None);
        }
        checked(output).map(|bytes| Some(text(without_final_newline(&bytes))))
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        let head = self.optional(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
        let branch = self
            .optional(&["symbolic-ref", "--quiet", "--short", "HEAD"])?
            .unwrap_or_else(|| {
                head.as_deref()
                    .map(|id| format!("Detached at {}", short_id(id)))
                    .unwrap_or_else(|| "HEAD".into())
            });
        let status = self.status()?;
        let refs = self.references()?;
        let limit = format!("--max-count={}", HISTORY_LIMIT + 1);
        let log = self.read(&[
            "log",
            "--all",
            "--topo-order",
            "--date=short",
            "--encoding=UTF-8",
            "--format=%H%x00%P%x00%an%x00%ad%x00%s",
            "-z",
            &limit,
        ])?;
        let mut commits = parse_log(&log);
        let history_limited = commits.len() > HISTORY_LIMIT;
        commits.truncate(HISTORY_LIMIT);
        Ok(Snapshot {
            branch,
            head,
            status,
            refs,
            commits,
            history_limited,
        })
    }

    pub fn status(&self) -> Result<Status> {
        self.read(&["status", "--porcelain=v1", "-z", "--untracked-files=all"])
            .map(|bytes| parse_status(&bytes))
    }

    fn references(&self) -> Result<Vec<Reference>> {
        let bytes = self.read(&[
            "for-each-ref",
            "--format=%(refname)%00%(objectname)%00%(*objectname)%00%(symref)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ])?;
        let refs = bytes
            .split(|byte| *byte == b'\n')
            .filter_map(|line| {
                let mut fields = line.split(|byte| *byte == 0);
                let name = std::str::from_utf8(fields.next()?).ok()?;
                let oid = fields.next()?;
                let peeled = fields.next()?;
                if !fields.next()?.is_empty() {
                    return None;
                }
                let (kind, name) = if let Some(name) = name.strip_prefix("refs/heads/") {
                    (RefKind::Local, name)
                } else if let Some(name) = name.strip_prefix("refs/remotes/") {
                    (RefKind::Remote, name)
                } else {
                    (RefKind::Tag, name.strip_prefix("refs/tags/")?)
                };
                let id = text(if peeled.is_empty() { oid } else { peeled });
                Some(Reference {
                    kind,
                    name: name.to_owned(),
                    id,
                })
            })
            .collect();
        Ok(refs)
    }

    pub fn detail(&self, commit: &Commit) -> Result<CommitDetail> {
        let message = self.read(&["show", "--no-patch", "--format=%B", &commit.id, "--"])?;
        let args = match commit.parents.first() {
            Some(parent) => vec![
                "diff",
                "--name-status",
                "-z",
                "-M",
                parent,
                &commit.id,
                "--",
            ],
            None => vec![
                "diff-tree",
                "--root",
                "--no-commit-id",
                "--name-status",
                "-r",
                "-z",
                "-M",
                &commit.id,
                "--",
            ],
        };
        let changes = parse_name_status(&self.read(&args)?);
        Ok(CommitDetail {
            message: text(&message).trim_end().to_owned(),
            changes,
        })
    }

    pub fn patch(&self, file: &FileTarget) -> Result<Patch> {
        let mut git = self.git();
        git.env("GIT_LITERAL_PATHSPECS", "1");
        let is_untracked = matches!(file.scope, Scope::Worktree) && file.change.code == b'?';
        match &file.scope {
            Scope::Commit { parent: None, .. } => {
                git.args(["diff-tree", "--root", "--no-commit-id", "-r", "-p", "-M"]);
            }
            _ => {
                git.arg("diff");
            }
        }
        if is_untracked {
            git.arg("--no-index");
        }
        git.args(PATCH_OPTIONS);
        match &file.scope {
            Scope::Index => {
                git.arg("--cached");
            }
            Scope::Commit { id, parent } => {
                if let Some(parent) = parent {
                    git.arg("-M").arg(parent);
                }
                git.arg(id);
            }
            Scope::Worktree => {}
        }
        git.arg("--");
        if is_untracked {
            git.arg("/dev/null");
        }
        append_paths(&mut git, &file.change);
        let output = git
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(process_error)?;
        let bytes = if is_untracked && output.status.code() == Some(1) {
            output.stdout
        } else {
            checked(output)?
        };
        Ok(Patch::new(bytes, file))
    }

    pub fn stage(&self, change: Option<&Change>) -> Result<()> {
        let mut git = self.git();
        git.env("GIT_LITERAL_PATHSPECS", "1")
            .args(["add", "-A", "--"]);
        if let Some(change) = change {
            append_paths(&mut git, change);
        }
        run_write(&mut git)
    }

    pub fn unstage(&self, change: Option<&Change>) -> Result<()> {
        let head = self.optional(&["rev-parse", "--verify", "--quiet", "HEAD"])?;
        let mut git = self.git();
        git.env("GIT_LITERAL_PATHSPECS", "1");
        if head.is_some() {
            git.args(["restore", "--staged", "--"]);
        } else {
            git.args(["rm", "-r", "--cached", "--force", "--"]);
        }
        if let Some(change) = change {
            append_paths(&mut git, change);
        } else {
            git.arg(".");
        }
        run_write(&mut git)
    }

    pub fn apply_hunk(&self, file: &FileTarget, shown: &Patch, index: usize) -> Result<()> {
        let patch = self.patch(file)?;
        if patch.bytes != shown.bytes {
            return Err(
                "The diff changed on disk. Review the refreshed diff before staging.".into(),
            );
        }
        let range = patch
            .hunks
            .get(index)
            .ok_or("This hunk cannot be staged separately.")?;
        let mut git = self.git();
        git.args(["apply", "--cached", "--whitespace=nowarn"]);
        if file.scope == Scope::Index {
            git.arg("--reverse");
        }
        let mut child = git
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(process_error)?;
        let mut input = child.stdin.take().expect("piped stdin");
        let write_result = input
            .write_all(&patch.bytes[..patch.header])
            .and_then(|_| input.write_all(&patch.bytes[range.clone()]));
        drop(input);
        checked(child.wait_with_output().map_err(process_error)?)?;
        write_result.map_err(process_error)
    }

    pub fn commit(&self, summary: &str, description: &str) -> Result<()> {
        let summary = summary.trim();
        if summary.is_empty() {
            return Err("Enter a commit summary.".into());
        }
        let mut git = self.git();
        git.args(["commit", "-m", summary]);
        if !description.trim().is_empty() {
            git.args(["-m", description.trim()]);
        }
        run_write(&mut git)
    }

    pub fn switch(&self, name: &str) -> Result<()> {
        run_write(self.git().args(["switch", "--no-guess", "--", name]))
    }

    pub fn create_branch(&self, name: &str) -> Result<()> {
        run_write(self.git().args(["switch", "-c", name]))
    }

    pub fn init(path: &Path) -> Result<Self> {
        run_write(command(None).args(["init", "--"]).arg(path))?;
        Self::open(path)
    }

    pub fn clone(url: &str, path: &Path) -> Result<Self> {
        run_write(command(None).args(["clone", "--", url]).arg(path))?;
        Self::open(path)
    }
}

impl Patch {
    fn new(bytes: Vec<u8>, file: &FileTarget) -> Self {
        let mut hunks: Vec<Range<usize>> = Vec::new();
        let mut offset = 0;
        let mut headers = 0;
        let mut mode_change = false;
        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            if line.starts_with(b"diff --git ") {
                headers += 1;
            } else if line.starts_with(b"old mode ") || line.starts_with(b"new mode ") {
                mode_change = true;
            } else if line.starts_with(b"@@ ") {
                if let Some(previous) = hunks.last_mut() {
                    previous.end = offset;
                }
                hunks.push(offset..bytes.len());
            }
            offset += line.len();
        }
        let header = hunks.first().map_or(0, |range| range.start);
        if !matches!(file.scope, Scope::Worktree | Scope::Index)
            || file.change.code != b'M'
            || file.change.previous.is_some()
            || headers != 1
            || mode_change
            || bytes.len() > PATCH_VIEW_LIMIT
        {
            hunks.clear();
        }
        Self {
            bytes,
            hunks,
            header,
        }
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn hunk_labels(&self) -> Vec<String> {
        self.hunks
            .iter()
            .map(|range| {
                let line = self.bytes[range.clone()]
                    .split(|byte| *byte == b'\n')
                    .next()
                    .unwrap_or_default();
                text(line)
            })
            .collect()
    }
}

pub fn short_id(id: &str) -> &str {
    id.get(..7).unwrap_or(id)
}

fn command(directory: Option<&Path>) -> Command {
    #[cfg(windows)]
    let mut command = Command::new(windows_git());
    #[cfg(not(windows))]
    let mut command = Command::new("git");
    command
        .arg("--no-pager")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .stdin(Stdio::null());
    if let Some(path) = directory {
        command.arg("-C").arg(path);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    command
}

#[cfg(windows)]
fn windows_git() -> &'static Path {
    use std::sync::OnceLock;

    static GIT: OnceLock<PathBuf> = OnceLock::new();
    GIT.get_or_init(|| {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let from_path = std::env::split_paths(&path)
            .filter(|entry| entry.is_absolute())
            .map(|entry| entry.join("git.exe"))
            .find(|entry| entry.is_file());
        from_path
            .or_else(|| {
                [
                    ("ProgramFiles", "Git/cmd/git.exe"),
                    ("ProgramFiles", "PortableGit/bin/git.exe"),
                    ("ProgramFiles(x86)", "Git/cmd/git.exe"),
                    ("LOCALAPPDATA", "Programs/Git/cmd/git.exe"),
                ]
                .into_iter()
                .filter_map(|(root, suffix)| {
                    std::env::var_os(root).map(|root| PathBuf::from(root).join(suffix))
                })
                .find(|entry| entry.is_file())
            })
            .unwrap_or_else(|| PathBuf::from("git.exe"))
    })
}

fn append_paths(command: &mut Command, change: &Change) {
    command.arg(&change.path);
    if let Some(previous) = &change.previous {
        command.arg(previous);
    }
}

fn run_read(command: &mut Command) -> Result<Vec<u8>> {
    checked(
        command
            .env("GIT_OPTIONAL_LOCKS", "0")
            .output()
            .map_err(process_error)?,
    )
}

fn run_write(command: &mut Command) -> Result<()> {
    checked(command.output().map_err(process_error)?).map(drop)
}

fn checked(output: Output) -> Result<Vec<u8>> {
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = text(&output.stderr);
    let stdout = text(&output.stdout);
    let message = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    if message.is_empty() {
        Err(format!("Git exited with {}.", output.status))
    } else {
        Err(message.to_owned())
    }
}

fn process_error(error: std::io::Error) -> String {
    if error.kind() == std::io::ErrorKind::NotFound {
        "Git was not found. Install Git and ensure it is available on PATH.".to_owned()
    } else {
        error.to_string()
    }
}

fn without_final_newline(bytes: &[u8]) -> &[u8] {
    bytes
        .strip_suffix(b"\r\n")
        .or_else(|| bytes.strip_suffix(b"\n"))
        .unwrap_or(bytes)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn path_from_bytes(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(text(bytes))
    }
}

fn parse_log(bytes: &[u8]) -> Vec<Commit> {
    let fields: Vec<&[u8]> = bytes.split(|byte| *byte == 0).collect();
    fields
        .chunks_exact(5)
        .filter(|fields| !fields[0].is_empty())
        .map(|fields| Commit {
            id: text(fields[0]),
            parents: String::from_utf8_lossy(fields[1])
                .split_ascii_whitespace()
                .map(str::to_owned)
                .collect(),
            author: text(fields[2]),
            date: text(fields[3]),
            subject: text(fields[4]),
        })
        .collect()
}

fn parse_status(bytes: &[u8]) -> Status {
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut status = Status::default();
    while let Some(field) = fields.next().filter(|field| field.len() >= 4) {
        let (index, worktree) = (field[0], field[1]);
        let path = path_from_bytes(&field[3..]);
        let rename = matches!(index, b'R' | b'C') || matches!(worktree, b'R' | b'C');
        let previous = if rename {
            fields.next().map(path_from_bytes)
        } else {
            None
        };
        if matches!(
            (index, worktree),
            (b'D', b'D') | (b'A' | b'U', b'U') | (b'U', b'A' | b'D') | (b'D', b'U') | (b'A', b'A')
        ) {
            status.unstaged.push(Change {
                path,
                previous,
                code: b'U',
            });
            continue;
        }
        if !matches!(index, b' ' | b'?' | b'!') {
            status.staged.push(Change {
                path: path.clone(),
                previous: previous.clone(),
                code: index,
            });
        }
        if !matches!(worktree, b' ' | b'!') {
            status.unstaged.push(Change {
                path,
                previous: previous.filter(|_| matches!(worktree, b'R' | b'C')),
                code: worktree,
            });
        }
    }
    status
}

fn parse_name_status(bytes: &[u8]) -> Vec<Change> {
    let mut fields = bytes.split(|byte| *byte == 0);
    let mut changes = Vec::new();
    while let Some(code) = fields.next().and_then(|field| field.first()).copied() {
        let Some(first) = fields.next() else { break };
        let (path, previous) = if matches!(code, b'R' | b'C') {
            let Some(second) = fields.next() else { break };
            (path_from_bytes(second), Some(path_from_bytes(first)))
        } else {
            (path_from_bytes(first), None)
        };
        changes.push(Change {
            path,
            previous,
            code,
        });
    }
    changes
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (tempfile::TempDir, Repo) {
        let directory = tempfile::Builder::new()
            .prefix("ritkraken-git-")
            .tempdir()
            .unwrap();
        let repo = Repo::init(directory.path()).unwrap();
        for (key, value) in [
            ("user.name", "RitKraken Test"),
            ("user.email", "ritkraken-test@example.invalid"),
            ("core.autocrlf", "false"),
            ("core.hooksPath", ".git/disabled-hooks"),
            ("commit.gpgsign", "false"),
        ] {
            run_write(repo.git().args(["config", "--local", key, value])).unwrap();
        }
        (directory, repo)
    }

    #[test]
    fn partial_stage_preserves_other_edits_and_refuses_a_stale_patch() {
        let (_directory, repo) = fixture();
        let name = "测试 file.txt";
        let path = repo.root.join(name);
        let original = (1..=40)
            .map(|line| format!("original {line}\n"))
            .collect::<String>();
        std::fs::write(&path, &original).unwrap();
        repo.stage(None).unwrap();
        repo.commit("Initial commit", "Baseline").unwrap();

        let modified = original
            .replace("original 3\n", "first edit\n")
            .replace("original 35\n", "second edit\n");
        std::fs::write(&path, &modified).unwrap();
        let snapshot = repo.snapshot().unwrap();
        assert_eq!(snapshot.commits[0].subject, "Initial commit");
        assert_eq!(snapshot.commits[0].author, "RitKraken Test");
        assert_eq!(snapshot.refs.len(), 1);
        let target = FileTarget {
            scope: Scope::Worktree,
            change: snapshot.status.unstaged[0].clone(),
        };
        assert_eq!(target.change.path, Path::new(name));
        let patch = repo.patch(&target).unwrap();
        assert_eq!(patch.hunk_labels().len(), 2);
        repo.apply_hunk(&target, &patch, 0).unwrap();
        let status = repo.status().unwrap();
        assert_eq!(status.staged[0].path, status.unstaged[0].path);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), modified);

        let staged = FileTarget {
            scope: Scope::Index,
            change: status.staged[0].clone(),
        };
        let staged_patch = repo.patch(&staged).unwrap();
        assert!(text(staged_patch.bytes()).contains("+first edit"));
        assert!(!text(staged_patch.bytes()).contains("+second edit"));
        repo.apply_hunk(&staged, &staged_patch, 0).unwrap();
        assert!(repo.status().unwrap().staged.is_empty());

        std::fs::write(&path, modified.replace("first edit", "changed externally")).unwrap();
        assert!(
            repo.apply_hunk(&target, &patch, 0)
                .unwrap_err()
                .contains("changed on disk")
        );
        assert!(repo.status().unwrap().staged.is_empty());

        let current = repo.patch(&target).unwrap();
        repo.apply_hunk(&target, &current, 1).unwrap();
        repo.commit("Partial commit", "").unwrap();
        let snapshot = repo.snapshot().unwrap();
        assert_eq!(snapshot.commits[0].subject, "Partial commit");
        assert_eq!(
            snapshot.commits[0].parents,
            vec![snapshot.commits[1].id.clone()]
        );
        assert!(snapshot.status.staged.is_empty());
        assert_eq!(snapshot.status.unstaged.len(), 1);
        let detail = repo.detail(&snapshot.commits[0]).unwrap();
        assert_eq!(detail.changes[0].path, Path::new(name));
        let committed = repo.read(&["show", &format!("HEAD:{name}")]).unwrap();
        assert!(text(&committed).contains("second edit"));
        assert!(!text(&committed).contains("changed externally"));
    }

    #[test]
    fn untracked_preview_and_unstage_work_on_an_unborn_branch() {
        let (_directory, repo) = fixture();
        std::fs::write(repo.root.join("new file.txt"), "new text\n").unwrap();
        let snapshot = repo.snapshot().unwrap();
        assert!(snapshot.head.is_none());
        assert!(snapshot.commits.is_empty());
        let file = FileTarget {
            scope: Scope::Worktree,
            change: snapshot.status.unstaged[0].clone(),
        };
        let patch = repo.patch(&file).unwrap();
        assert!(text(patch.bytes()).contains("+new text"));
        repo.stage(Some(&file.change)).unwrap();
        let staged = repo.status().unwrap().staged.remove(0);
        repo.unstage(Some(&staged)).unwrap();
        assert_eq!(repo.status().unwrap().unstaged[0].code, b'?');
        assert!(repo.root.join("new file.txt").is_file());
    }
}
