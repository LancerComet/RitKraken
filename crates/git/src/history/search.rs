use std::path::{Component, Path};

use crate::{Repo, Result, RuleError, command::run_read, protocol::text};

use super::{COMMIT_DATE_FORMAT, COMMIT_LOG_FORMAT, Commit, RefKind, parse_log};

#[derive(Clone, Debug, Default)]
pub struct CommitSearch {
    pub text: String,
    pub author: String,
    pub path: String,
    pub branch: String,
    pub since: String,
    pub until: String,
}

impl Repo {
    /// Searches all history, including detached HEAD, with literal AND filters.
    pub fn search_commits(&self, search: &CommitSearch) -> Result<Vec<Commit>> {
        let since = search_date(&search.since)?;
        let until = search_date(&search.until)?;
        if !search.path.is_empty()
            && (search.path.contains('\0')
                || Path::new(&search.path)
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_))))
        {
            return Err(RuleError::InvalidFilePath.into());
        }
        let branch = if search.branch.is_empty() {
            None
        } else {
            let name = search
                .branch
                .strip_prefix("refs/heads/")
                .or_else(|| search.branch.strip_prefix("refs/remotes/"))
                .unwrap_or(&search.branch);
            self.validate_ref_name(name, "heads")?;
            let (refs, _) = self.references(None)?;
            Some(
                refs.into_iter()
                    .find(|reference| {
                        let full = match reference.kind {
                            RefKind::Local => format!("refs/heads/{}", reference.name),
                            RefKind::Remote => format!("refs/remotes/{}", reference.name),
                            RefKind::Tag => return false,
                        };
                        search.branch == full || search.branch == reference.name
                    })
                    .ok_or(RuleError::RefUnavailable)?
                    .id,
            )
        };
        let mut git = self.git_with_literal_paths();
        git.args([
            "log",
            "--topo-order",
            "--no-show-signature",
            "--encoding=UTF-8",
            COMMIT_DATE_FORMAT,
            "-z",
        ])
        .arg(format!("{COMMIT_LOG_FORMAT}%x00%B"));
        if let Some(date) = since {
            git.arg(format!("--since-as-filter={date} 00:00:00"));
        }
        if let Some(date) = until {
            git.arg(format!("--until={date} 23:59:59"));
        }
        if let Some(id) = branch {
            git.arg(id);
        } else {
            git.args(["--exclude=refs/stash", "--all", "--ignore-missing", "HEAD"]);
        }
        git.arg("--");
        if !search.path.is_empty() {
            git.arg(&search.path);
        }
        let bytes = run_read(&mut git)?;
        let text_query = search.text.to_lowercase();
        let author_query = search.author.to_lowercase();
        let fields: Vec<_> = bytes.split(|byte| *byte == 0).collect();
        let mut matched = Vec::new();
        for fields in fields.chunks_exact(7) {
            if (text_query.is_empty()
                || text(fields[0]).to_lowercase().starts_with(&text_query)
                || text(fields[6]).to_lowercase().contains(&text_query))
                && (author_query.is_empty()
                    || text(fields[2]).to_lowercase().contains(&author_query)
                    || text(fields[3]).to_lowercase().contains(&author_query))
            {
                for field in &fields[..6] {
                    matched.extend_from_slice(field);
                    matched.push(0);
                }
            }
        }
        Ok(parse_log(&matched))
    }
}

fn search_date(value: &str) -> Result<Option<&str>> {
    if value.is_empty() {
        return Ok(None);
    }
    let invalid = || RuleError::InvalidSearchDate;
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return Err(invalid().into());
    }
    let year: u32 = value[..4].parse().map_err(|_| invalid())?;
    let month: u32 = value[5..7].parse().map_err(|_| invalid())?;
    let day: u32 = value[8..].parse().map_err(|_| invalid())?;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) => {
            29
        }
        2 => 28,
        _ => return Err(invalid().into()),
    };
    if year == 0 || day == 0 || day > days {
        return Err(invalid().into());
    }
    Ok(Some(value))
}
