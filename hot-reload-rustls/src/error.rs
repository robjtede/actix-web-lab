use std::{io, path::PathBuf};

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

impl_more::impl_display_enum! {
    Error:
    Io(_) => "credential I/O failed",
    Tls(_) => "TLS validation failed",
    Watch(_) => "filesystem watcher setup failed",
    InvalidPath(path) => ("credential path must name a file: {}", path.display()),
    MissingPrivateKey => "no private key in PEM file",
    ProviderMismatch => "TLS builder must use the provider passed to Builder::new",
    WorkerStopped => "TLS reload worker stopped during initialization",
    Configuration(_) => "TLS configuration callback failed",
}

impl_more::impl_error_enum! {
    Error:
    Io(error) => error,
    Tls(error) => error,
    Watch(error) => error,
    Configuration(error) => error.as_ref(),
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
