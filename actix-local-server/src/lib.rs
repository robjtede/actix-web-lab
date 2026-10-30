//! Run Actix TCP services on the current thread.
//!
//! `serve()` initializes a `Service<TcpStream>`, then passes each accepted TCP stream to it.
//! Connections run concurrently as local tasks. The service and its futures can own state that is
//! not `Send` or `Sync`.
//!
//! The crate manages TCP acceptance and connection tasks. The caller supplies protocol handling,
//! such as an `actix_http::HttpService`. Await the server directly in `#[tokio::main]` or
//! `#[tokio::main(flavor = "local")]`. On a multi-thread runtime, the server creates an internal
//! `LocalSet`. On a local runtime, it uses the runtime directly. The server does not create threads
//! or install signal handlers.
//!
//! # Examples
//!
//! ```no_run
//! use std::{convert::Infallible, io, net::TcpListener};
//!
//! use actix_http::{HttpService, Request, Response, StatusCode, body::MessageBody};
//! use actix_service::ServiceFactory as _;
//!
//! async fn index(_req: Request) -> Result<Response<impl MessageBody>, Infallible> {
//!     Ok(Response::build(StatusCode::OK).body("Hello!"))
//! }
//!
//! #[tokio::main]
//! async fn main() -> io::Result<()> {
//!     let listener = TcpListener::bind(("127.0.0.1", 8080))?;
//!     let addr = listener.local_addr()?;
//!
//!     actix_local_server::serve(listener, async move {
//!         HttpService::build()
//!             .local_addr(addr)
//!             .h1(index)
//!             .tcp()
//!             .new_service(())
//!             .await
//!             .map_err(|()| io::Error::other("failed to initialize HTTP service"))
//!     })
//!     .await
//! }
//! ```

use std::{
    fmt,
    future::{Future, poll_fn},
    io, net,
};

use actix_service::Service;
use tokio::{
    net::TcpStream,
    runtime::{Handle, RuntimeFlavor},
    task::{JoinSet, LocalSet},
};

/// Initializes a service and passes accepted TCP connections to it on the current thread.
///
/// Sets the listener to nonblocking mode, then awaits `service` in the local execution context.
/// Construct the service inside this future if construction starts local tasks, as
/// `actix_http::HttpService` does. For an existing service, use `std::future::ready(Ok(service))`.
///
/// On a Tokio multi-thread runtime, creates a [`LocalSet`] to run initialization and connections.
/// On a local runtime, uses the runtime directly. Await this future on the calling thread; it is
/// not `Send` and cannot be passed to [`tokio::spawn()`].
///
/// For each accepted connection, waits for service readiness, then starts the connection future as
/// a local task. Other connection tasks can make progress while readiness is pending. Completed
/// tasks are removed and connection errors are logged.
///
/// This future runs until a listener or readiness error occurs, or until it is dropped. Dropping it
/// closes the listener and aborts all connection tasks without draining them. Tasks spawned by the
/// service itself are not tracked on a local runtime. On a multi-thread runtime, dropping the
/// internal `LocalSet` also cancels those tasks.
///
/// # Errors
///
/// Returns listener setup, service initialization, and accept errors directly. Service readiness
/// errors are returned as [`io::ErrorKind::Other`] with the error's display message. Connection errors
/// do not stop the server.
///
/// # Panics
///
/// Panics if polled outside a Tokio runtime with I/O enabled. A `current_thread` runtime requires
/// a caller-provided [`LocalSet`]; use `flavor = "local"` to run without one.
pub async fn serve<S>(
    listener: net::TcpListener,
    service: impl Future<Output = io::Result<S>>,
) -> io::Result<()>
where
    S: Service<TcpStream, Response = ()>,
    S::Error: fmt::Display + 'static,
    S::Future: 'static,
{
    let server = async move {
        listener.set_nonblocking(true)?;
        let listener = tokio::net::TcpListener::from_std(listener)?;
        let service = service.await?;

        let mut connections = JoinSet::new();

        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (stream, _) = accepted?;

                    poll_fn(|cx| service.poll_ready(cx))
                        .await
                        .map_err(|err| io::Error::other(err.to_string()))?;

                    connections.spawn_local(service.call(stream));
                }

                Some(result) = connections.join_next(), if !connections.is_empty() => {
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(err)) => tracing::debug!(error = %err, "TCP connection failed"),
                        Err(err) => tracing::error!(error = %err, "TCP connection task failed"),
                    }
                }
            }
        }
    };

    match Handle::current().runtime_flavor() {
        RuntimeFlavor::MultiThread => LocalSet::new().run_until(server).await,
        _ => server.await,
    }
}
