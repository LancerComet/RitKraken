mod timeout;

pub(crate) use timeout::run_write_with_timeout;

use std::{
    io::Write,
    path::Path,
    process::{Command, Output, Stdio},
};

use crate::{Error, Result, protocol::text};

pub(crate) fn command(directory: Option<&Path>) -> Command {
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
    use std::{path::PathBuf, sync::OnceLock};

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

pub(crate) fn read_output(command: &mut Command) -> Result<Output> {
    without_optional_locks(command)
        .output()
        .map_err(process_error)
}

pub(crate) fn without_optional_locks(command: &mut Command) -> &mut Command {
    command.env("GIT_OPTIONAL_LOCKS", "0")
}

pub(crate) fn run_read(command: &mut Command) -> Result<Vec<u8>> {
    checked(read_output(command)?)
}

pub(crate) fn run_write(command: &mut Command) -> Result<()> {
    checked(command.output().map_err(process_error)?).map(drop)
}

pub(crate) fn run_with_input(command: &mut Command, parts: &[&[u8]]) -> Result<Vec<u8>> {
    checked(output_with_input(command, parts)?)
}

pub(crate) fn output_with_input(command: &mut Command, parts: &[&[u8]]) -> Result<Output> {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(process_error)?;
    let mut input = child.stdin.take().expect("piped stdin");
    // Drain stdout while sending stdin: commands such as check-ignore can
    // produce output before consuming all input, filling both OS pipes.
    let (output, written) = std::thread::scope(|scope| {
        let writer = scope.spawn(move || parts.iter().try_for_each(|part| input.write_all(part)));
        let output = child.wait_with_output();
        (output, writer.join().expect("Git stdin writer"))
    });
    let output = output.map_err(process_error)?;
    if output.status.success() {
        written.map_err(process_error)?;
    }
    Ok(output)
}

pub(crate) fn checked(output: Output) -> Result<Vec<u8>> {
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
    let status = output.status;
    if message.is_empty() {
        Err(Error::Exited(status))
    } else {
        Err(Error::Git {
            status,
            message: message.to_owned(),
        })
    }
}

pub(crate) fn process_error(error: std::io::Error) -> Error {
    if error.kind() == std::io::ErrorKind::NotFound {
        Error::MissingGit(error)
    } else {
        Error::Invocation(error)
    }
}
