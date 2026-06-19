use std::convert::Infallible;

use actix_rt::net::ActixStream;
use actix_service::{Service, ServiceFactory};
use actix_utils::future::{Ready, ready};
use futures_core::future::LocalBoxFuture;

use super::{HeaderPolicy, ProxyProtocolError, ProxyStream};

/// Actix service factory that wraps accepted streams in [`ProxyStream`].
#[derive(Debug, Clone, Copy, Default)]
pub struct Acceptor {
    policy: HeaderPolicy,
}

impl Acceptor {
    /// Constructs an acceptor that requires a PROXY protocol header.
    pub const fn new() -> Self {
        Self {
            policy: HeaderPolicy::Required,
        }
    }

    /// Constructs an acceptor using `policy`.
    pub const fn with_policy(policy: HeaderPolicy) -> Self {
        Self { policy }
    }

    /// Constructs an acceptor that allows streams without a PROXY protocol header.
    pub const fn optional() -> Self {
        Self {
            policy: HeaderPolicy::Optional,
        }
    }
}

impl<IO> ServiceFactory<IO> for Acceptor
where
    IO: ActixStream + 'static,
{
    type Response = ProxyStream<IO>;
    type Error = ProxyProtocolError<Infallible>;
    type Config = ();
    type Service = AcceptorService;
    type InitError = ();
    type Future = Ready<Result<Self::Service, Self::InitError>>;

    fn new_service(&self, _: ()) -> Self::Future {
        ready(Ok(AcceptorService {
            policy: self.policy,
        }))
    }
}

/// Actix service that wraps streams in [`ProxyStream`].
#[derive(Debug, Clone, Copy)]
pub struct AcceptorService {
    policy: HeaderPolicy,
}

impl<IO> Service<IO> for AcceptorService
where
    IO: ActixStream + 'static,
{
    type Response = ProxyStream<IO>;
    type Error = ProxyProtocolError<Infallible>;
    type Future = LocalBoxFuture<'static, Result<Self::Response, Self::Error>>;

    actix_service::always_ready!();

    fn call(&self, io: IO) -> Self::Future {
        let policy = self.policy;

        Box::pin(async move { ProxyStream::accept_with_policy(io, policy).await })
    }
}

#[cfg(test)]
mod tests {
    use actix_service::Service as _;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

    use super::*;
    use crate::{Header, Version, v1};

    #[actix_rt::test]
    async fn acceptor_service_wraps_streams() {
        let listener = actix_rt::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let local_addr = listener.local_addr().unwrap();
        let factory = Acceptor::new();
        let acceptor =
            <Acceptor as ServiceFactory<actix_rt::net::TcpStream>>::new_service(&factory, ())
                .await
                .unwrap();

        actix_rt::spawn(async move {
            let mut client = actix_rt::net::TcpStream::connect(local_addr).await.unwrap();
            v1::Header::unknown()
                .write_to_tokio(&mut client)
                .await
                .unwrap();
            client.write_all(b"hello").await.unwrap();
            client.shutdown().await.unwrap();
        });

        let (server, _) = listener.accept().await.unwrap();
        let mut stream = acceptor.call(server).await.unwrap();

        assert_eq!(stream.header().map(Header::version), Some(Version::V1));

        let mut body = String::new();
        stream.read_to_string(&mut body).await.unwrap();
        assert_eq!(body, "hello");
    }
}
