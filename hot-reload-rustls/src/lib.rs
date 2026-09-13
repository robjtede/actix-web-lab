#![doc = include_str!("../README.md")]

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use arc_swap::ArcSwap;
use rustls::{
    ConfigBuilder, ServerConfig,
    server::{ClientHello, ResolvesServerCert, WantsServerCert},
    sign::CertifiedKey,
};

mod watcher;

pub use self::watcher::{Error, Event, Watcher};

/// A resolver that reads one validated snapshot per full handshake.
#[derive(Debug)]
struct Resolver(Arc<ArcSwap<CertifiedKey>>);

impl ResolvesServerCert for Resolver {
    fn resolve(&self, _hello: ClientHello<'_>) -> Option<Arc<CertifiedKey>> {
        Some(self.0.load_full())
    }
}

type Configure = dyn FnOnce(&mut ServerConfig) -> Result<(), Error> + Send;

/// TLS credential watcher builder. Only the PEM file paths are required.
#[must_use]
pub struct Builder {
    cert: PathBuf,
    key: PathBuf,
    configure: Box<Configure>,
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
    /// Use secure server defaults with the given PEM certificate chain and private key.
    pub fn new(cert: impl AsRef<Path>, key: impl AsRef<Path>) -> Self {
        Self {
            cert: cert.as_ref().to_owned(),
            key: key.as_ref().to_owned(),
            configure: Box::new(|_| Ok(())),
            tls: None,
        }
    }

    /// Customize the TLS configuration. A second call replaces the previous callback.
    ///
    /// Called once on the worker, after the reload resolver is installed. Use this for ALPN
    /// and session settings. Do not replace the certificate resolver. Use `tls_config` to
    /// select a crypto provider, protocol versions, or client authentication.
    pub fn configure(
        mut self,
        configure: impl FnOnce(&mut ServerConfig) -> Result<(), Error> + Send + 'static,
    ) -> Self {
        self.configure = Box::new(configure);

        self
    }

    /// Replace the default ring provider, safe protocol versions, and no-client-auth policy
    /// with a caller-selected Rustls builder. This also works without the default ring feature.
    pub fn tls_config(mut self, builder: ConfigBuilder<ServerConfig, WantsServerCert>) -> Self {
        self.tls = Some(builder);

        self
    }

    /// Validate the initial credentials and start watching on a dedicated worker.
    ///
    /// Returns the TLS configuration and a handle to keep until server shutdown. Dropping the
    /// handle stops watching. Call before starting an async runtime or from a blocking task.
    /// Reloads debounce for 100 ms and retry failures up to five times, 200 ms apart.
    pub fn build(self) -> Result<(ServerConfig, Watcher), Error> {
        let tls = match self.tls {
            Some(tls) => tls,
            None => default_tls_config()?,
        };

        load(&self.cert, &self.key, tls, self.configure)
    }
}

fn default_tls_config() -> Result<ConfigBuilder<ServerConfig, WantsServerCert>, Error> {
    #[cfg(feature = "ring")]
    {
        Ok(
            ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
                .with_safe_default_protocol_versions()?
                .with_no_client_auth(),
        )
    }

    #[cfg(not(feature = "ring"))]
    {
        Err("enable the ring feature or provide a builder with tls_config".into())
    }
}

fn load(
    cert: impl AsRef<Path>,
    key: impl AsRef<Path>,
    builder: ConfigBuilder<ServerConfig, WantsServerCert>,
    configure: Box<Configure>,
) -> Result<(ServerConfig, Watcher), Error> {
    let provider = Arc::clone(builder.crypto_provider());

    watcher::start(
        cert.as_ref(),
        key.as_ref(),
        move |cert, key| {
            let certs = rustls_pemfile::certs(&mut &*cert).collect::<Result<Vec<_>, _>>()?;
            for cert in &certs {
                rustls::server::ParsedCertificate::try_from(cert)?;
            }

            let key =
                rustls_pemfile::private_key(&mut &*key)?.ok_or("no private key in PEM file")?;

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
