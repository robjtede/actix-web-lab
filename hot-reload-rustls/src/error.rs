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

impl_more::impl_enum_from!(io::Error => BuildError::Io);

impl_more::impl_enum_from!(rustls::Error => BuildError::Tls);

impl_more::impl_enum_from!(notify::Error => BuildError::Watch);

impl_more::impl_enum_from!(CredentialError => BuildError::Credentials);

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

impl_more::impl_enum_from!(io::Error => CredentialError::Io);

impl_more::impl_enum_from!(rustls::Error => CredentialError::Tls);
