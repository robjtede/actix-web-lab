use std::{fmt, io};

/// SSE decoding error.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Invalid SSE format.
    Invalid,

    /// I/O error.
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::Invalid => "Invalid SSE format",
            Error::Io(_) => "I/O error",
        })
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}
