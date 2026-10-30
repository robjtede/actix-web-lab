//! HTTP servers that run on the current thread.

use std::{fmt, io, net};

use actix_http::{Extensions, HttpService, Request, Response};
use actix_service::{
    IntoServiceFactory, Service, ServiceFactory, ServiceFactoryExt as _, map_config,
};
use actix_web::{Error, body::MessageBody, dev::AppConfig};
use tokio::net::TcpStream;

type ConnectCallback = Box<dyn Fn(&TcpStream, &mut Extensions)>;

/// An HTTP/1 server that owns its app and runs on the current thread.
///
/// The app is initialized once. Connections run concurrently as local tasks. The server does not
/// create threads, and the app and connection callback do not need to implement `Send` or `Sync`.
///
/// Await the server directly in `#[tokio::main]` or `#[tokio::main(flavor = "local")]`. On a
/// multi-thread runtime, the server uses an internal [`LocalSet`](tokio::task::LocalSet) to keep
/// the app and connections on the calling thread. On a local runtime, it uses the runtime directly.
///
/// TLS, HTTP/2, signal handling, and graceful shutdown are not supported. Dropping the serving
/// future aborts its connection tasks.
///
/// # Application Configuration
///
/// The app receives [`AppConfig::default()`]. Its local address is `127.0.0.1:8080` and its fallback
/// host is `localhost:8080`, regardless of the listener address. Actix Web does not currently expose
/// a supported constructor to set these values.
///
/// # Examples
///
/// ```no_run
/// use std::{cell::Cell, net::TcpListener};
///
/// use actix_web::{App, Responder, get};
/// use actix_web_lab::{extract::LocalData, server::LocalHttpServer};
///
/// #[get("/")]
/// async fn index(count: LocalData<Cell<usize>>) -> impl Responder {
///     count.set(count.get() + 1);
///
///     count.get().to_string()
/// }
///
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     let count = LocalData::new(Cell::new(0usize));
///     let app = App::new().app_data(count).service(index);
///
///     let listener = TcpListener::bind(("127.0.0.1", 8080))?;
///
///     LocalHttpServer::new(app).serve(listener).await
/// }
/// ```
#[must_use]
pub struct LocalHttpServer<A> {
    app: A,
    on_connect: Option<ConnectCallback>,
}

impl<A> fmt::Debug for LocalHttpServer<A> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LocalHttpServer")
            .field("on_connect", &self.on_connect.is_some())
            .finish_non_exhaustive()
    }
}

impl<A> LocalHttpServer<A> {
    /// Creates a server from an owned app.
    pub fn new(app: A) -> Self {
        Self {
            app,
            on_connect: None,
        }
    }

    /// Sets a callback that runs once for each accepted connection, before HTTP processing starts.
    ///
    /// Data inserted into the extensions is available through
    /// [`HttpRequest::conn_data()`](actix_web::HttpRequest::conn_data) for every request on that
    /// connection. The callback runs on the serving thread and can capture local state.
    pub fn on_connect<F>(mut self, callback: F) -> Self
    where
        F: Fn(&TcpStream, &mut Extensions) + 'static,
    {
        self.on_connect = Some(Box::new(callback));
        self
    }

    /// Serves HTTP/1 connections on the listener.
    ///
    /// Sets the listener to nonblocking mode and initializes the app before accepting connections.
    /// This future runs until an error occurs or it is dropped. Connection errors are logged and
    /// do not stop the server. Dropping this future aborts all connection tasks without draining
    /// pending requests.
    ///
    /// This future is not `Send`. Await it on the calling thread instead of using [`tokio::spawn()`].
    ///
    /// # Errors
    ///
    /// Returns an error if listener setup, app initialization, service readiness, or acceptance of
    /// a connection fails.
    ///
    /// # Panics
    ///
    /// Panics if polled outside a Tokio runtime with I/O and time enabled. A `current_thread` runtime
    /// requires a caller-provided [`LocalSet`](tokio::task::LocalSet); use `flavor = "local"` to run
    /// without one.
    pub async fn serve<S, B>(self, listener: net::TcpListener) -> io::Result<()>
    where
        A: IntoServiceFactory<S, Request>,
        S: ServiceFactory<Request, Config = AppConfig> + 'static,
        S::Error: Into<Error> + 'static,
        S::InitError: fmt::Debug,
        S::Response: Into<Response<B>> + 'static,
        S::Future: 'static,
        S::Service: 'static,
        <S::Service as Service<Request>>::Future: 'static,
        B: MessageBody + 'static,
    {
        let addr = listener.local_addr()?;

        actix_local_server::serve(listener, async move {
            let mut builder = HttpService::build().local_addr(addr);

            if let Some(callback) = self.on_connect {
                builder =
                    builder.on_connect_ext(move |stream, extensions| callback(stream, extensions));
            }

            let app = self
                .app
                .into_factory()
                .map_err(|err| err.into().error_response());

            builder
                .h1(map_config(app, |_| AppConfig::default()))
                .tcp()
                .new_service(())
                .await
                .map_err(|()| io::Error::other("failed to initialize HTTP service"))
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        future::{Future, pending, ready},
        net::SocketAddr,
        thread,
        time::Duration,
    };

    use actix_web::{App, HttpRequest, HttpResponse, middleware, web};
    use tokio::{
        io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader},
        sync::{Notify, oneshot},
        time::timeout,
    };

    use super::*;
    use crate::extract::LocalData;

    const TIMEOUT: Duration = Duration::from_secs(5);

    async fn with_server(
        server: impl Future<Output = io::Result<()>>,
        test: impl Future<Output = ()>,
    ) {
        tokio::select! {
            result = server => panic!("server stopped: {result:?}"),
            result = timeout(TIMEOUT, test) => result.expect("test timed out"),
        }
    }

    async fn send_request(stream: &mut BufReader<TcpStream>, path: &str) {
        stream
            .get_mut()
            .write_all(format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes())
            .await
            .unwrap();
    }

    async fn read_response(stream: &mut BufReader<TcpStream>) -> String {
        let mut line = String::new();
        stream.read_line(&mut line).await.unwrap();
        assert_eq!(line, "HTTP/1.1 200 OK\r\n");

        let mut content_length = None;

        loop {
            line.clear();
            assert_ne!(stream.read_line(&mut line).await.unwrap(), 0);

            if line == "\r\n" {
                break;
            }

            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = Some(value.trim().parse::<usize>().unwrap());
            }
        }

        let mut body = vec![0; content_length.expect("response has no content length")];
        stream.read_exact(&mut body).await.unwrap();

        String::from_utf8(body).unwrap()
    }

    async fn request(stream: &mut BufReader<TcpStream>, path: &str) -> String {
        send_request(stream, path).await;
        read_response(stream).await
    }

    #[tokio::test(flavor = "local")]
    async fn owned_app_is_initialized_once_on_current_thread() {
        check_owned_app().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn owned_app_is_initialized_once_on_multi_thread_runtime() {
        check_owned_app().await;
    }

    async fn check_owned_app() {
        let current_thread = thread::current().id();
        let initializations = LocalData::new(Cell::new(0));
        let initialized = initializations.clone();
        let requests = LocalData::new(Cell::new(0usize));

        let app = App::new()
            .data_factory(move || {
                let initialized = initialized.clone();

                async move {
                    tokio::task::spawn_local(async move {
                        assert_eq!(thread::current().id(), current_thread);
                        initialized.set(initialized.get() + 1);

                        Ok::<_, ()>("initialized")
                    })
                    .await
                    .unwrap()
                }
            })
            .app_data(requests.clone())
            .wrap(middleware::Condition::new(
                true,
                middleware::DefaultHeaders::new().add(("x-local-server", "yes")),
            ))
            .route(
                "/",
                web::get().to(move |requests: LocalData<Cell<usize>>| async move {
                    assert_eq!(thread::current().id(), current_thread);
                    requests.set(requests.get() + 1);

                    requests.get().to_string()
                }),
            );

        let listener = net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        with_server(LocalHttpServer::new(app).serve(listener), async {
            let mut first = BufReader::new(TcpStream::connect(addr).await.unwrap());
            assert_eq!(request(&mut first, "/").await, "1");

            let mut second = BufReader::new(TcpStream::connect(addr).await.unwrap());
            assert_eq!(request(&mut second, "/").await, "2");
            assert_eq!(request(&mut first, "/").await, "3");

            assert_eq!(initializations.get(), 1);
            assert_eq!(requests.get(), 3);
        })
        .await;
    }

    #[tokio::test(flavor = "local")]
    async fn on_connect_data_is_shared_by_requests_on_each_connection() {
        check_on_connect_data().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn on_connect_data_is_shared_on_multi_thread_runtime() {
        check_on_connect_data().await;
    }

    async fn check_on_connect_data() {
        let current_thread = thread::current().id();
        let connections = LocalData::new(Cell::new(0usize));
        let connected = connections.clone();

        let app = App::new().route(
            "/",
            web::get().to(|req: HttpRequest| async move {
                let (id, peer) = req.conn_data::<(usize, SocketAddr)>().unwrap();
                assert_eq!(req.peer_addr(), Some(*peer));

                format!("{id}:{peer}")
            }),
        );

        let server = LocalHttpServer::new(app).on_connect(move |stream, extensions| {
            assert_eq!(thread::current().id(), current_thread);
            connected.set(connected.get() + 1);
            extensions.insert((connected.get(), stream.peer_addr().unwrap()));
        });

        let listener = net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        with_server(server.serve(listener), async {
            let mut first = BufReader::new(TcpStream::connect(addr).await.unwrap());
            let expected = format!("1:{}", first.get_ref().local_addr().unwrap());

            assert_eq!(request(&mut first, "/").await, expected);
            assert_eq!(request(&mut first, "/").await, expected);
            assert_eq!(connections.get(), 1);

            let mut second = BufReader::new(TcpStream::connect(addr).await.unwrap());
            let expected = format!("2:{}", second.get_ref().local_addr().unwrap());

            assert_eq!(request(&mut second, "/").await, expected);
            assert_eq!(connections.get(), 2);
        })
        .await;
    }

    #[tokio::test(flavor = "local")]
    async fn pending_request_does_not_block_other_connections() {
        check_concurrent_requests().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn pending_request_does_not_block_other_connections_on_multi_thread_runtime() {
        check_concurrent_requests().await;
    }

    async fn check_concurrent_requests() {
        let started = LocalData::new(Notify::new());
        let release = LocalData::new(Notify::new());
        let handler_started = started.clone();
        let handler_release = release.clone();

        let app = App::new()
            .route(
                "/wait",
                web::get().to(move || {
                    let started = handler_started.clone();
                    let release = handler_release.clone();

                    async move {
                        started.notify_one();
                        release.notified().await;

                        "released"
                    }
                }),
            )
            .route("/ready", web::get().to(|| async { "ready" }));

        let listener = net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        with_server(LocalHttpServer::new(app).serve(listener), async {
            let mut waiting = BufReader::new(TcpStream::connect(addr).await.unwrap());
            send_request(&mut waiting, "/wait").await;
            started.notified().await;

            let mut other = BufReader::new(TcpStream::connect(addr).await.unwrap());
            assert_eq!(request(&mut other, "/ready").await, "ready");

            release.notify_one();
            assert_eq!(read_response(&mut waiting).await, "released");
        })
        .await;
    }

    #[tokio::test(flavor = "local")]
    async fn dropping_server_cancels_requests_and_closes_connections() {
        check_cancellation().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn dropping_server_cancels_requests_and_closes_connections_on_multi_thread_runtime() {
        check_cancellation().await;
    }

    async fn check_cancellation() {
        let started = LocalData::new(Notify::new());
        let handler_started = started.clone();
        let (dropped_tx, dropped_rx) = oneshot::channel::<()>();
        let dropped = LocalData::new(Cell::new(Some(dropped_tx)));

        let app = App::new()
            .app_data(dropped)
            .route(
                "/wait",
                web::get().to(
                    move |dropped: LocalData<Cell<Option<oneshot::Sender<()>>>>| {
                        let started = handler_started.clone();

                        async move {
                            let dropped = dropped.take().unwrap();
                            started.notify_one();
                            pending::<()>().await;

                            drop(dropped);
                            HttpResponse::Ok().finish()
                        }
                    },
                ),
            )
            .route("/ready", web::get().to(|| async { "ready" }));

        let listener = net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        timeout(TIMEOUT, async {
            let mut server = Box::pin(LocalHttpServer::new(app).serve(listener));

            let (mut idle, mut active) = tokio::select! {
                result = &mut server => panic!("server stopped: {result:?}"),
                streams = async {
                    let mut idle = BufReader::new(TcpStream::connect(addr).await.unwrap());
                    assert_eq!(request(&mut idle, "/ready").await, "ready");

                    let mut active = BufReader::new(TcpStream::connect(addr).await.unwrap());
                    send_request(&mut active, "/wait").await;
                    started.notified().await;

                    (idle, active)
                } => streams,
            };

            drop(server);

            assert!(dropped_rx.await.is_err());

            assert_eq!(idle.read(&mut [0]).await.unwrap(), 0);
            assert_eq!(active.read(&mut [0]).await.unwrap(), 0);
        })
        .await
        .expect("test timed out");
    }

    #[tokio::test(flavor = "local")]
    async fn app_initialization_error_is_returned() {
        check_initialization_error().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn app_initialization_error_is_returned_on_multi_thread_runtime() {
        check_initialization_error().await;
    }

    async fn check_initialization_error() {
        let app = App::new().data_factory(|| ready(Err::<(), _>("initialization failed")));
        let listener = net::TcpListener::bind(("127.0.0.1", 0)).unwrap();

        let err = timeout(TIMEOUT, LocalHttpServer::new(app).serve(listener))
            .await
            .expect("server did not return initialization error")
            .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::Other);
    }

    #[tokio::test(flavor = "local")]
    async fn malformed_connection_does_not_stop_server() {
        check_malformed_connection().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn malformed_connection_does_not_stop_server_on_multi_thread_runtime() {
        check_malformed_connection().await;
    }

    async fn check_malformed_connection() {
        let app = App::new().route("/", web::get().to(|| async { "ready" }));
        let listener = net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        with_server(LocalHttpServer::new(app).serve(listener), async {
            let mut malformed = TcpStream::connect(addr).await.unwrap();
            malformed
                .write_all(b"invalid request\r\n\r\n")
                .await
                .unwrap();

            let mut response = String::new();
            malformed.read_to_string(&mut response).await.unwrap();
            assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));

            let mut valid = BufReader::new(TcpStream::connect(addr).await.unwrap());
            assert_eq!(request(&mut valid, "/").await, "ready");
        })
        .await;
    }
}
