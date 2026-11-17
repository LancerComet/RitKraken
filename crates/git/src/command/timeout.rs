#[cfg(test)]
mod tests;

use std::{
    fs::File,
    io::{self, Read, Seek},
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use super::{CallTimer, checked, process_error};
use crate::{Error, Result};

const POLL_INTERVAL: Duration = Duration::from_millis(20);
const OUTPUT_LIMIT: u64 = 1024 * 1024;

pub(crate) fn run_write_with_timeout(mut command: Command, timeout: Duration) -> Result<()> {
    let deadline = Instant::now()
        .checked_add(timeout)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Git timeout is too large"))?;
    // Files cannot deadlock the waiter when a transport helper inherits stdio.
    // They are anonymous, automatically removed, and diagnostics stay bounded.
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .stdout(Stdio::from(stdout.try_clone()?))
        .stderr(Stdio::from(stderr.try_clone()?));
    let timer = CallTimer::start(&command);
    let mut running = match Running::spawn(&mut command) {
        Ok(running) => running,
        Err(error) => {
            if let Some(timer) = timer {
                timer.not_started(&error);
            }
            return Err(process_error(error));
        }
    };
    let status = loop {
        match running.child.try_wait() {
            Ok(Some(status)) => {
                running.finished = true;
                break Ok(status);
            }
            Ok(None) => {}
            Err(error) => break Err(process_error(error)),
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break Err(Error::Timeout(timeout));
        }
        thread::sleep(remaining.min(POLL_INTERVAL));
    };
    let status = match status {
        Ok(status) => status,
        Err(operation) => {
            if let Some(timer) = timer {
                timer.interrupted(&operation);
            }
            return match running.stop() {
                Ok(()) => Err(operation),
                Err(cleanup) => Err(Error::CleanupFailed {
                    operation: Box::new(operation),
                    cleanup: Box::new(cleanup.into()),
                }),
            };
        }
    };
    let output = Output {
        status,
        stdout: read_diagnostic(&mut stdout)?,
        stderr: read_diagnostic(&mut stderr)?,
    };
    if let Some(timer) = timer {
        timer.finish(Some(status), &output.stderr);
    }
    checked(output).map(drop)
}

fn read_diagnostic(file: &mut File) -> io::Result<Vec<u8>> {
    file.rewind()?;
    let mut bytes = Vec::new();
    file.take(OUTPUT_LIMIT).read_to_end(&mut bytes)?;
    Ok(bytes)
}

struct Running {
    child: Child,
    finished: bool,
}

impl Running {
    fn spawn(command: &mut Command) -> io::Result<Self> {
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        Ok(Self {
            child: command.spawn()?,
            finished: false,
        })
    }

    fn stop(&mut self) -> io::Result<()> {
        if self.finished {
            return Ok(());
        }
        let tree = terminate_tree(&mut self.child);
        // Still reap the direct child if terminating its descendants failed.
        let root = self
            .child
            .kill()
            .or_else(|error| self.child.try_wait()?.map_or(Err(error), |_| Ok(())));
        if root.is_ok() {
            self.child.wait()?;
            self.finished = true;
        }
        tree.and(root)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.stop();
        }
    }
}

#[cfg(windows)]
fn terminate_tree(child: &mut Child) -> io::Result<()> {
    use std::{os::windows::process::CommandExt, path::PathBuf};

    if child.try_wait()?.is_some() {
        return Ok(());
    }
    let system = std::env::var_os("SystemRoot")
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "SystemRoot is not set"))?;
    // The PID belongs to the Child held above. /T includes git-remote-* and SSH.
    let status = Command::new(PathBuf::from(system).join("System32").join("taskkill.exe"))
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .creation_flags(0x0800_0000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    if status.success() || child.try_wait()?.is_some() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Could not stop Git process tree ({status})"
        )))
    }
}

#[cfg(unix)]
fn terminate_tree(child: &mut Child) -> io::Result<()> {
    let group = i32::try_from(child.id()).map_err(io::Error::other)?;
    // process_group(0) created a group containing only this command's descendants.
    let result = unsafe { libc::kill(-group, libc::SIGKILL) };
    if result == 0 {
        return Ok(());
    }
    let error = io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        Ok(())
    } else {
        Err(error)
    }
}
