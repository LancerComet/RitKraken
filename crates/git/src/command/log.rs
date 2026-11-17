use std::{
    ffi::OsStr,
    io,
    path::PathBuf,
    process::{Command, ExitStatus, Output},
    sync::OnceLock,
    time::{Duration, Instant},
};

use crate::protocol::text;

/// Git's diagnostics are kept in full for the log, up to this many characters,
/// so one misbehaving command cannot flood the application log.
const DETAIL_LIMIT: usize = 4000;

/// One finished (or failed to start) Git process.
#[derive(Clone, Debug)]
pub struct GitCall {
    pub directory: Option<PathBuf>,
    /// Arguments after the repository selection, with credentials removed.
    pub args: Vec<String>,
    pub duration: Duration,
    /// `None` when Git could not be started or was stopped without a code.
    pub exit_code: Option<i32>,
    /// Git's error output for a failed command, or the reason it did not start.
    pub detail: Option<String>,
}

impl GitCall {
    /// A non-zero exit without diagnostics is an expected answer such as
    /// "not found", not a failure.
    pub fn failed(&self) -> bool {
        self.exit_code != Some(0) && self.detail.is_some()
    }
}

type Listener = Box<dyn Fn(&GitCall) + Send + Sync>;

static LISTENER: OnceLock<Listener> = OnceLock::new();

/// Receives every Git process this crate runs. Only the first listener is kept.
pub fn set_git_call_listener(listener: impl Fn(&GitCall) + Send + Sync + 'static) -> bool {
    LISTENER.set(Box::new(listener)).is_ok()
}

pub(crate) struct CallTimer {
    started: Instant,
    directory: Option<PathBuf>,
    args: Vec<String>,
}

impl CallTimer {
    pub(crate) fn start(command: &Command) -> Option<Self> {
        LISTENER.get()?;
        let mut directory = None;
        let mut args = Vec::new();
        let mut raw = command.get_args();
        while let Some(arg) = raw.next() {
            match arg.to_str() {
                Some("--no-pager") => {}
                Some("-C") if directory.is_none() => directory = raw.next().map(PathBuf::from),
                _ => args.push(redact(arg)),
            }
        }
        Some(Self {
            started: Instant::now(),
            directory,
            args,
        })
    }

    pub(crate) fn output(timer: Option<Self>, output: &io::Result<Output>) {
        let Some(timer) = timer else { return };
        match output {
            Ok(output) => timer.finish(Some(output.status), &output.stderr),
            Err(error) => timer.not_started(error),
        }
    }

    pub(crate) fn finish(self, status: Option<ExitStatus>, stderr: &[u8]) {
        let exit_code = status.and_then(|status| status.code());
        let detail = (exit_code != Some(0))
            .then(|| limit(text(stderr).trim()))
            .filter(|detail| !detail.is_empty());
        self.emit(exit_code, detail);
    }

    pub(crate) fn not_started(self, error: &io::Error) {
        self.emit(None, Some(error.to_string()));
    }

    /// Git was stopped before it finished, for example after a timeout.
    pub(crate) fn interrupted(self, reason: &impl std::fmt::Display) {
        self.emit(None, Some(reason.to_string()));
    }

    fn emit(self, exit_code: Option<i32>, detail: Option<String>) {
        if let Some(listener) = LISTENER.get() {
            listener(&GitCall {
                directory: self.directory,
                args: self.args,
                duration: self.started.elapsed(),
                exit_code,
                detail,
            });
        }
    }
}

fn limit(detail: &str) -> String {
    match detail.char_indices().nth(DETAIL_LIMIT) {
        Some((end, _)) => format!("{}…", &detail[..end]),
        None => detail.to_owned(),
    }
}

/// Removes `user:password@` from URLs so tokens never reach the log.
fn redact(arg: &OsStr) -> String {
    let arg = arg.to_string_lossy();
    let Some(scheme) = arg.find("://") else {
        return arg.into_owned();
    };
    let authority = scheme + 3;
    let host_end = arg[authority..]
        .find('/')
        .map_or(arg.len(), |end| authority + end);
    match arg[authority..host_end].rfind('@') {
        Some(at) => format!("{}***@{}", &arg[..authority], &arg[authority + at + 1..]),
        None => arg.into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;

    use super::{limit, redact};

    #[test]
    fn credentials_in_urls_are_removed() {
        assert_eq!(
            redact(OsStr::new("https://user:token@example.com/a.git")),
            "https://***@example.com/a.git"
        );
        assert_eq!(
            redact(OsStr::new("https://example.com/a@b.git")),
            "https://example.com/a@b.git"
        );
        assert_eq!(
            redact(OsStr::new("git@example.com:a.git")),
            "git@example.com:a.git"
        );
        assert_eq!(redact(OsStr::new("--force")), "--force");
    }

    #[test]
    fn long_diagnostics_are_bounded() {
        let long = "x".repeat(5000);
        assert_eq!(limit(&long).chars().count(), 4001);
        assert_eq!(limit("short"), "short");
    }
}
