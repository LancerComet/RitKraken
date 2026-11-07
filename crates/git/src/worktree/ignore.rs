use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::PathBuf,
};

use crate::{Change, Repo, Result, RuleError};

use super::files::validate_change;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum IgnoreRule {
    File,
    Extension,
}

impl IgnoreRule {
    pub fn pattern(self, change: &Change) -> Result<String> {
        validate_change(change)?;
        let pattern = match self {
            Self::File => {
                let parts = change.path.components().map(|part| {
                    part.as_os_str()
                        .to_str()
                        .ok_or(RuleError::InvalidIgnorePattern)
                });
                let path = parts.collect::<std::result::Result<Vec<_>, _>>()?.join("/");
                format!("/{}", escape_pattern(&path)?)
            }
            Self::Extension => {
                let extension = change
                    .path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .filter(|extension| !extension.is_empty())
                    .ok_or(RuleError::InvalidIgnorePattern)?;
                format!("*.{}", escape_pattern(extension)?)
            }
        };
        Ok(pattern)
    }
}

impl Repo {
    pub fn ignore_file(&self, change: &Change, rule: IgnoreRule) -> Result<()> {
        let pattern = rule.pattern(change)?;
        let path = self.root().join(".gitignore");
        let lock = self.root().join(".gitignore.lock");
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&lock)?;
        let _lock = IgnoreLock(lock.clone());
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                Some(metadata)
            }
            Ok(_) => return Err(RuleError::InvalidIgnoreFile.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if metadata
            .as_ref()
            .is_some_and(|metadata| metadata.permissions().readonly())
        {
            return Err(
                io::Error::new(io::ErrorKind::PermissionDenied, ".gitignore is read-only").into(),
            );
        }
        let original = if metadata.is_some() {
            fs::read(&path)?
        } else {
            Vec::new()
        };
        if original
            .split(|byte| *byte == b'\n')
            .any(|line| line.strip_suffix(b"\r").unwrap_or(line) == pattern.as_bytes())
        {
            return Ok(());
        }
        let newline: &[u8] = if original.windows(2).any(|pair| pair == b"\r\n") {
            b"\r\n"
        } else {
            b"\n"
        };
        output.write_all(&original)?;
        if !original.is_empty() && !original.ends_with(b"\n") {
            output.write_all(newline)?;
        }
        output.write_all(pattern.as_bytes())?;
        output.write_all(newline)?;
        output.sync_all()?;
        drop(output);
        if let Some(metadata) = metadata {
            fs::set_permissions(&lock, metadata.permissions())?;
        }
        let current = match fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => Vec::new(),
            Err(error) => return Err(error.into()),
        };
        if current != original {
            return Err(RuleError::StaleFile.into());
        }
        fs::rename(lock, path)?;
        Ok(())
    }
}

struct IgnoreLock(PathBuf);

impl Drop for IgnoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn escape_pattern(value: &str) -> Result<String> {
    if value.contains(['\r', '\n', '\0']) {
        return Err(RuleError::InvalidIgnorePattern.into());
    }
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        if matches!(character, '\\' | '*' | '?' | '[' | ']' | '#' | '!' | ' ') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    Ok(escaped)
}
