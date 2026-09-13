use std::{io, path::PathBuf};

/// A failure to configure TLS or start the credential watcher.
#[derive(Debug)]
#[non_exhaustive]
pub enum BuildError {
    /// Path resolution or worker thread creation failed.
    Io(io::Error),

    /// Rustls rejected the TLS configuration.
    Tls(rustls::Error),

    /// Native filesystem watcher setup failed.
    Watch(notify::Error),

    /// A credential path does not name a file with a parent directory.
    InvalidPath(PathBuf),

    /// The initial certificate/key pair could not be loaded or validated.
    Credentials(CredentialError),

    /// The custom TLS builder uses a different provider from the reload builder.
    ProviderMismatch,

    /// The worker stopped before it reported the initial configuration.
    WorkerStopped,

    /// A caller's configuration callback failed.
    Configuration(Box<dyn std::error::Error + Send + Sync>),
}

impl_more::impl_display_enum! {
    BuildError:
    Io(_) => "Watcher initialization I/O failed",
    Tls(_) => "TLS configuration failed",
    Watch(_) => "Filesystem watcher setup failed",
    InvalidPath(path) => ("Credential path must name a file: {}", path.display()),
    Credentials(_) => "Initial credential loading failed",
    ProviderMismatch => "TLS builder must use the provider passed to Builder::new",
    WorkerStopped => "TLS reload worker stopped during initialization",
    Configuration(_) => "TLS configuration callback failed",
}

impl_more::impl_error_enum! {
    BuildError:
    Io(error) => error,
    Tls(error) => error,
    Watch(error) => error,
    Credentials(error) => error,
    Configuration(error) => error.as_ref(),
}

impl From<io::Error> for BuildError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rustls::Error> for BuildError {
    fn from(error: rustls::Error) -> Self {
        Self::Tls(error)
    }
}

impl From<notify::Error> for BuildError {
    fn from(error: notify::Error) -> Self {
        Self::Watch(error)
    }
}

impl From<CredentialError> for BuildError {
    fn from(error: CredentialError) -> Self {
        Self::Credentials(error)
    }
}

/// A failure to read, parse, or validate a complete certificate/key pair.
#[derive(Debug)]
#[non_exhaustive]
pub enum CredentialError {
    /// A credential file read or PEM parse failed.
    Io(io::Error),

    /// Rustls rejected the certificate chain, private key, or key match.
    Tls(rustls::Error),

    /// The PEM file contains no private key.
    MissingPrivateKey,
}

impl_more::impl_display_enum! {
    CredentialError:
    Io(_) => "Credential I/O failed",
    Tls(_) => "Credential validation failed",
    MissingPrivateKey => "No private key in PEM file",
}

impl_more::impl_error_enum! {
    CredentialError:
    Io(error) => error,
    Tls(error) => error,
}

impl From<io::Error> for CredentialError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<rustls::Error> for CredentialError {
    fn from(error: rustls::Error) -> Self {
        Self::Tls(error)
    }
}
