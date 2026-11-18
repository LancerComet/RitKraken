use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::{Path, PathBuf},
};

use crate::{ReflogEntry, Repo, Result, protocol::text};

impl Repo {
    pub(super) fn deleted_branch_log(&self) -> Result<PathBuf> {
        Ok(self
            .common_dir()
            .join("logs")
            .join("ritkraken-deleted-branches"))
    }

    /// Keep a native reflog without creating a graph-visible reference. Git's
    /// normal reflog retention and garbage collection apply to these entries.
    /// Write before deletion: a failed preservation must leave the branch intact.
    pub(crate) fn preserve_deleted_branch(&self, name: &str, id: &str) -> Result<()> {
        // Same committer identity Git would stamp, without a `git var` call.
        let now = chrono::Local::now();
        let record = format!(
            "{id} {id} RitKraken <recovery@ritkraken.invalid> {} {}\tdeleted branch: {name}\n",
            now.timestamp(),
            now.format("%z"),
        );
        let path = self.deleted_branch_log()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(path)?;
        file.write_all(record.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }
}

pub(super) fn read_log(path: &Path, selector: &str) -> Result<Vec<ReflogEntry>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    Ok(text(&bytes)
        .lines()
        .rev()
        .enumerate()
        .filter_map(|(index, line)| {
            let (metadata, message) = line.split_once('\t').unwrap_or((line, ""));
            let mut fields = metadata.split_whitespace();
            let _old = fields.next()?;
            let id = fields.next()?;
            let mut date = metadata.rsplitn(3, ' ');
            let zone = date.next()?;
            let seconds = date.next()?;
            let timestamp =
                chrono::DateTime::parse_from_str(&format!("{seconds} {zone}"), "%s %z").ok()?;
            Some(ReflogEntry {
                id: id.to_owned(),
                selector: format!("{selector}@{{{index}}}"),
                date: timestamp.format("%Y/%m/%d @ %H:%M %z").to_string(),
                message: message.to_owned(),
            })
        })
        .collect())
}
