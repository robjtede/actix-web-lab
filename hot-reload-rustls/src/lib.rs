#![doc = include_str!("../README.md")]

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use arc_swap::ArcSwap;
use rustls::{
    ConfigBuilder, ServerConfig,
    crypto::CryptoProvider,
    server::{ClientHello, ResolvesServerCert, WantsServerCert},
    sign::CertifiedKey,
};

mod error;
mod watcher;

pub use self::{
    error::{BuildError, CredentialError},
    watcher::{Event, Watcher},
};

/// A resolver that reads one validated snapshot per full handshake.
#[derive(Debug)]
struct Resolver(Arc<ArcSwap<CertifiedKey>>);

impl ResolvesServerCert for Resolver {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.load_full())
    }
}

type Configure = dyn FnOnce(&mut ServerConfig) -> Result<(), BuildError> + Send;

/// TLS credential watcher builder. Requires PEM file paths and an explicit crypto provider.
#[must_use]
pub struct Builder {
    cert: PathBuf,
    key: PathBuf,
    configure: Box<Configure>,
    provider: Arc<CryptoProvider>,
    tls: Option<ConfigBuilder<ServerConfig, WantsServerCert>>,
}

impl fmt::Debug for Builder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Builder")
            .field("cert", &self.cert)
            .field("key", &self.key)
            .finish_non_exhaustive()
    }
}

impl Builder {
    /// Use safe protocol defaults with the given PEM files and caller-selected crypto provider.
    pub fn new(
        cert: impl AsRef<Path>,
        key: impl AsRef<Path>,
        provider: Arc<CryptoProvider>,
    ) -> Self {
        Self {
            cert: cert.as_ref().to_owned(),
            key: key.as_ref().to_owned(),
            configure: Box::new(|_| Ok(())),
            provider,
            tls: None,
        }
    }

    /// Customize the TLS configuration. A second call replaces the previous callback.
    ///
    /// Called once on the worker, after the reload resolver is installed. Use this for ALPN
    /// and session settings. Do not replace the certificate resolver. Use `tls_config` to
    /// select protocol versions or client authentication.
    pub fn configure(
        mut self,
        configure: impl FnOnce(&mut ServerConfig) -> Result<(), BuildError> + Send + 'static,
    ) -> Self {
        self.configure = Box::new(configure);

        self
    }

    /// Customize protocol versions and client authentication with a Rustls builder.
    /// It must use the same provider passed to `new`; `build` rejects a different provider.
    pub fn tls_config(mut self, builder: ConfigBuilder<ServerConfig, WantsServerCert>) -> Self {
        self.tls = Some(builder);

        self
    }

    /// Validate the initial credentials and start watching on a dedicated worker.
    ///
    /// Returns the TLS configuration and a handle to keep until server shutdown. Dropping the
    /// handle stops watching. Call before starting an async runtime or from a blocking task.
    /// Reloads debounce for 100 ms and retry failures up to five times, 200 ms apart.
    pub fn build(self) -> Result<(ServerConfig, Watcher), BuildError> {
        let tls = match self.tls {
            Some(tls) => {
                if !Arc::ptr_eq(tls.crypto_provider(), &self.provider) {
                    return Err(BuildError::ProviderMismatch);
                }
                tls
            }
            None => ServerConfig::builder_with_provider(self.provider)
                .with_safe_default_protocol_versions()?
                .with_no_client_auth(),
        };

        load(&self.cert, &self.key, tls, self.configure)
    }
}

fn load(
    cert: impl AsRef<Path>,
    key: impl AsRef<Path>,
    builder: ConfigBuilder<ServerConfig, WantsServerCert>,
    configure: Box<Configure>,
) -> Result<(ServerConfig, Watcher), BuildError> {
    let provider = Arc::clone(builder.crypto_provider());

    watcher::start(
        cert.as_ref(),
        key.as_ref(),
        move |cert, key| {
            let certs = rustls_pemfile::certs(&mut &*cert).collect::<Result<Vec<_>, _>>()?;
            for cert in &certs {
                rustls::server::ParsedCertificate::try_from(cert)?;
            }

            let key = rustls_pemfile::private_key(&mut &*key)?
                .ok_or(CredentialError::MissingPrivateKey)?;

            let pair = CertifiedKey::from_der(certs, key, &provider)?;
            pair.keys_match()?;

            Ok(pair)
        },
        move |current| {
            let mut config = builder.with_cert_resolver(Arc::new(Resolver(current)));
            configure(&mut config)?;

            Ok(config)
        },
    )
}
