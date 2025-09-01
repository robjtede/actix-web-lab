//! `rustls` v0.23 based TLS connection acceptor service.
//!
//! See [`Acceptor`] for main service factory docs.

use std::{
    any::Any,
    convert::Infallible,
    io::{self, IoSlice},
    pin::Pin,
    task::{Context, Poll},
};

use actix_rt::net::{ActixStream, Ready};
use actix_service::{Service, ServiceFactory};
use actix_utils::future::{Ready as FutReady, ready};
use futures_core::future::LocalBoxFuture;
use tokio::{
    io::{AsyncBufReadExt as _, AsyncRead, AsyncWrite, BufReader, ReadBuf},
    net::TcpStream,
};

use crate::{v1, v2};

/// TLS handshake error, TLS timeout, or inner service error.
///
/// All TLS acceptors from this crate will return the `SvcErr` type parameter as [`Infallible`],
/// which can be cast to your own service type, inferred or otherwise, using [`into_service_error`].
///
/// [`into_service_error`]: Self::into_service_error
#[derive(Debug)]
pub enum TlsError<TlsErr, SvcErr> {
    /// Wraps TLS service errors.
    Tls(TlsErr),

    /// Wraps service errors.
    Service(SvcErr),
}

/// Wraps a `rustls` based async TLS stream in order to implement [`ActixStream`].
pub struct TlsStream<IO>(pub BufReader<IO>);

impl<IO: ActixStream> AsyncRead for TlsStream<IO> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl<IO: ActixStream> AsyncWrite for TlsStream<IO> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.0.is_write_vectored()
    }
}

impl<IO: ActixStream> ActixStream for TlsStream<IO> {
    fn poll_read_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Ready>> {
        <_>::poll_read_ready(&self.0, cx)
    }

    fn poll_write_ready(&self, cx: &mut Context<'_>) -> Poll<io::Result<Ready>> {
        <_>::poll_write_ready(&self.0, cx)
    }
}

/// Accept TLS connections via the `rustls` crate.
#[derive(Debug, Clone, Default)]
pub struct Acceptor {}

impl Acceptor {
    /// Constructs `rustls` based acceptor service factory.
    pub fn new() -> Self {
        Acceptor {}
    }
}

impl<TlsErr> TlsError<TlsErr, Infallible> {
    /// Casts the infallible service error type returned from acceptors into caller's type.
    pub fn into_service_error<SvcErr>(self) -> TlsError<TlsErr, SvcErr> {
        match self {
            Self::Tls(err) => TlsError::Tls(err),
            Self::Service(err) => match err {},
        }
    }
}

impl<IO: ActixStream + Any + 'static> ServiceFactory<IO> for Acceptor {
    type Response = TlsStream<IO>;
    type Error = TlsError<io::Error, Infallible>;
    type Config = ();
    type Service = AcceptorService;
    type InitError = ();
    type Future = FutReady<Result<Self::Service, Self::InitError>>;

    fn new_service(&self, _: ()) -> Self::Future {
        let res = Ok(AcceptorService {});

        ready(res)
    }
}

/// Rustls based acceptor service.
pub struct AcceptorService {}

impl<IO: ActixStream + Any + 'static> Service<IO> for AcceptorService {
    type Response = TlsStream<IO>;
    type Error = TlsError<io::Error, Infallible>;
    type Future = LocalBoxFuture<'static, Result<TlsStream<IO>, TlsError<io::Error, Infallible>>>;

    actix_service::always_ready!();

    fn call(&self, io: IO) -> Self::Future {
        Box::pin(async move {
            // TODO: gross
            let io_ref = (&io as &dyn Any).downcast_ref::<TcpStream>().unwrap();

            let mut header = [0; 12];
            // TODO: peek until header buf full
            io_ref.peek(&mut header).await.map_err(TlsError::Tls)?;

            let mut io = BufReader::new(io);

            if &header[..v1::SIGNATURE.len()] == v1::SIGNATURE.as_bytes() {
                tracing::debug!("v1");

                let mut buf = Vec::with_capacity(v1::MAX_HEADER_SIZE);
                let _len = io.read_until(b'\n', &mut buf).await.unwrap();

                eprintln!("{}", String::from_utf8_lossy(&buf));

                let (rest, header) = match v1::Header::try_from_bytes(&buf) {
                    Ok((rest, header)) => (rest, header),
                    Err(err) => {
                        match err {
                            nom::Err::Incomplete(_needed) => todo!(),
                            nom::Err::Error(err) => {
                                eprintln!(
                                    "err {:?}, input: {}",
                                    err.code,
                                    String::from_utf8_lossy(err.input)
                                )
                            }
                            nom::Err::Failure(_) => todo!(),
                        }
                        todo!();
                        // return Err(todo!());
                    }
                };

                eprintln!("{:02X?} - {:?}", rest, header);
            } else if header == v2::SIGNATURE {
                tracing::debug!("v2");
            } else {
                tracing::warn!("No proxy header");
            }

            Ok(TlsStream(io))
        })
    }
}
