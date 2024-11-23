use std::{fmt, io};

#[derive(Debug)]
pub enum Error {
    Invalid,
    Io(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Error::Invalid => "Invalid",
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
