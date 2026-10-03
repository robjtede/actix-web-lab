#![doc = include_str!("../README.md")]

use std::{
    fmt,
    path::{Path, PathBuf},
    sync::Arc,
};

use openssl::{
    pkey::PKey,
    ssl::{ClientHelloResponse, SslAcceptor, SslAcceptorBuilder},
    x509::X509,
};

mod watcher;

pub use self::watcher::{Error, Event, Watcher};

type Configure = dyn Fn(&mut SslAcceptorBuilder) -> Result<(), Error> + Send + Sync;

/// TLS credential watcher builder. Only the PEM file paths are required.
#[must_use]
pub struct Builder {
    cert: PathBuf,
    key: PathBuf,
    configure: Box<Configure>,
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
        }
    }

    /// Customize the TLS configuration. A second call replaces the previous callback.
    ///
    /// Called on the worker for the frontend and each replacement context, after applying
    /// the Mozilla intermediate v5 preset. Apply the same policy each time. Do not install
    /// credentials or callbacks that replace the selected context. Set ALPN here if needed.
    pub fn configure(
        mut self,
        configure: impl Fn(&mut SslAcceptorBuilder) -> Result<(), Error> + Send + Sync + 'static,
    ) -> Self {
        self.configure = Box::new(configure);

        self
    }

    /// Validate the initial credentials and start watching on a dedicated worker.
    ///
    /// Returns the TLS configuration and a handle to keep until server shutdown. Dropping the
    /// handle stops watching. Call before starting an async runtime or from a blocking task.
    /// Reloads debounce for 100 ms and retry failures up to five times, 200 ms apart.
    pub fn build(self) -> Result<(SslAcceptorBuilder, Watcher), Error> {
        let configure = self.configure;

        load(&self.cert, &self.key, move || {
            let mut builder =
                SslAcceptor::mozilla_intermediate_v5(openssl::ssl::SslMethod::tls_server())?;
            builder.set_min_proto_version(Some(openssl::ssl::SslVersion::TLS1_2))?;

            configure(&mut builder)?;

            Ok(builder)
        })
    }
}

fn load(
    cert: impl AsRef<Path>,
    key: impl AsRef<Path>,
    configure: impl Fn() -> Result<SslAcceptorBuilder, Error> + Send + Sync + 'static,
) -> Result<(SslAcceptorBuilder, Watcher), Error> {
    let configure = Arc::new(configure);
    let frontend = Arc::clone(&configure);

    watcher::start(
        cert.as_ref(),
        key.as_ref(),
        move |cert, key| {
            let mut chain = X509::stack_from_pem(cert)?.into_iter();
            let leaf = chain.next().ok_or("no certificate in PEM file")?;
            let key = PKey::private_key_from_pem(key)?;

            if !leaf.public_key()?.public_eq(&key) {
                return Err("certificate and private key do not match".into());
            }

            let mut builder = configure()?;
            builder.set_certificate(&leaf)?;
            builder.set_private_key(&key)?;
            for cert in chain {
                builder.add_extra_chain_cert(cert)?;
            }

            builder.check_private_key()?;

            Ok(builder.build())
        },
        move |current: Arc<arc_swap::ArcSwap<SslAcceptor>>| {
            let mut builder = frontend()?;
            builder.set_client_hello_callback(move |ssl, _alert| {
                let acceptor = current.load_full();
                ssl.set_ssl_context(acceptor.context())?;

                Ok(ClientHelloResponse::SUCCESS)
            });

            Ok(builder)
        },
    )
}
