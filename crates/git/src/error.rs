use std::{io, process::ExitStatus};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Git was not found. Install Git and ensure it is available on PATH.")]
    MissingGit(#[source] io::Error),
    #[error("Could not run Git: {0}")]
    Invocation(#[source] io::Error),
    #[error("{message}")]
    Git { status: ExitStatus, message: String },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("{0}")]
    InvalidOperation(String),
}

impl From<&str> for Error {
    fn from(message: &str) -> Self {
        Self::InvalidOperation(message.to_owned())
    }
}

impl From<String> for Error {
    fn from(message: String) -> Self {
        Self::InvalidOperation(message)
    }
}

pub type Result<T> = std::result::Result<T, Error>;
