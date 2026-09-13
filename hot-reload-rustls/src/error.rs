use std::{fmt, io, path::PathBuf};

/// An initialization, configuration, or credential validation error.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A filesystem operation, PEM read, or worker thread creation failed.
    Io(io::Error),

    /// Rustls rejected the TLS configuration or certificate/key pair.
    Tls(rustls::Error),

    /// Native filesystem watcher setup failed.
    Watch(notify::Error),

    /// A credential path does not name a file with a parent directory.
    InvalidPath(PathBuf),

    /// The PEM file contains no private key.
    MissingPrivateKey,

    /// The custom TLS builder uses a different provider from the reload builder.
    ProviderMismatch,

    /// The worker stopped before it reported the initial configuration.
    WorkerStopped,

    /// A caller's configuration callback failed.
    Configuration(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "credential I/O failed: {error}"),
            Self::Tls(error) => write!(f, "TLS validation failed: {error}"),
            Self::Watch(error) => write!(f, "filesystem watcher setup failed: {error}"),
            Self::InvalidPath(path) => {
                write!(f, "credential path must name a file: {}", path.display())
            }
            Self::MissingPrivateKey => f.write_str("no private key in PEM file"),
            Self::ProviderMismatch => {
                f.write_str("TLS builder must use the provider passed to Builder::new")
            }
            Self::WorkerStopped => f.write_str("TLS reload worker stopped during initialization"),
            Self::Configuration(error) => write!(f, "TLS configuration callback failed: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Tls(error) => Some(error),
            Self::Watch(error) => Some(error),
            Self::Configuration(error) => Some(error.as_ref()),
            Self::InvalidPath(_)
            | Self::MissingPrivateKey
            | Self::ProviderMismatch
            | Self::WorkerStopped => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rustls::Error> for Error {
    fn from(error: rustls::Error) -> Self {
        Self::Tls(error)
    }
}

impl From<notify::Error> for Error {
    fn from(error: notify::Error) -> Self {
        Self::Watch(error)
    }
}
