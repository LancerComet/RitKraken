use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

const FILE_NAME: &str = "ritkraken.log";
/// Older files kept beside the active one, so the logs directory stays below
/// about `(KEPT_FILES + 1) × limit`. The preference hint text states this count.
const KEPT_FILES: usize = 3;

/// Appends lines to `ritkraken.log` and starts a new file once it would grow
/// past the configured size, shifting `ritkraken.1.log` … `ritkraken.N.log`.
pub(super) struct RollingFile {
    directory: PathBuf,
    limit: u64,
    size: u64,
    file: Option<File>,
}

impl RollingFile {
    pub(super) fn new(directory: PathBuf, limit: u64) -> Self {
        let size = fs::metadata(directory.join(FILE_NAME)).map_or(0, |file| file.len());
        Self {
            directory,
            limit,
            size,
            file: None,
        }
    }

    pub(super) fn path(&self) -> PathBuf {
        self.directory.join(FILE_NAME)
    }

    pub(super) fn set_limit(&mut self, limit: u64) {
        self.limit = limit;
    }

    /// Logging never interrupts the application: write failures are dropped.
    pub(super) fn append(&mut self, text: &str) {
        let length = text.len() as u64;
        if self.size > 0 && self.size + length > self.limit {
            self.rotate();
        }
        if self.file.is_none() {
            self.file = fs::create_dir_all(&self.directory)
                .and_then(|()| {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(self.path())
                })
                .ok();
        }
        if let Some(file) = &mut self.file
            && file.write_all(text.as_bytes()).is_ok()
        {
            self.size += length;
        }
    }

    fn rotate(&mut self) {
        self.file = None;
        let rotated = |index: usize| self.directory.join(format!("ritkraken.{index}.log"));
        let _ = fs::remove_file(rotated(KEPT_FILES));
        for index in (1..KEPT_FILES).rev() {
            rename_if_present(&rotated(index), &rotated(index + 1));
        }
        rename_if_present(&self.path(), &rotated(1));
        self.size = 0;
    }
}

fn rename_if_present(from: &Path, to: &Path) {
    if from.exists() {
        let _ = fs::rename(from, to);
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{KEPT_FILES, RollingFile};

    #[test]
    fn rotation_keeps_each_file_under_the_limit_and_only_recent_files() {
        let directory =
            std::env::temp_dir().join(format!("ritkraken-log-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        let mut log = RollingFile::new(directory.clone(), 100);
        for index in 0..40 {
            log.append(&format!("line {index:02} with some padding text\n"));
        }
        let mut names: Vec<_> = fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names.len(), KEPT_FILES + 1, "{names:?}");
        for name in &names {
            assert!(
                fs::metadata(directory.join(name)).unwrap().len() <= 100,
                "{name}"
            );
        }
        let newest = fs::read_to_string(directory.join("ritkraken.log")).unwrap();
        assert!(newest.contains("line 39"));
        assert!(
            !fs::read_to_string(directory.join(format!("ritkraken.{KEPT_FILES}.log")))
                .unwrap()
                .contains("line 00")
        );

        let mut reopened = RollingFile::new(directory.clone(), 100);
        reopened.append("after restart\n");
        assert!(
            fs::read_to_string(directory.join("ritkraken.log"))
                .unwrap()
                .ends_with("after restart\n")
        );
        fs::remove_dir_all(directory).unwrap();
    }
}
