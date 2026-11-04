//! Tests TCP acceptance, service readiness, and connection task ownership.

use std::{
    cell::{Cell, RefCell},
    future::{Future, Ready, ready},
    io,
    net::TcpListener,
    rc::Rc,
    task::{Context, Poll, Waker},
    thread,
    time::Duration,
};

use actix_local_server::serve;
use actix_service::{Service, fn_service};
use tokio::{
    io::{AsyncReadExt as _, AsyncWriteExt as _},
    net::TcpStream,
    sync::{Notify, mpsc},
    time::timeout,
};

async fn with_timeout(test: impl Future<Output = ()>) {
    timeout(Duration::from_secs(5), test)
        .await
        .expect("test timed out");
}

#[tokio::test(flavor = "local")]
async fn local_connections_run_concurrently_and_close_when_server_is_dropped() {
    check_connections().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connections_stay_on_current_thread_and_close_when_server_is_dropped_on_multi_thread_runtime()
 {
    check_connections().await;
}

async fn check_connections() {
    with_timeout(async {
        let current_thread = thread::current().id();
        let calls = Rc::new(Cell::new(0u8));
        let called = Rc::clone(&calls);
        let (started_tx, mut started_rx) = mpsc::unbounded_channel();

        let service = fn_service(move |mut stream: TcpStream| {
            assert_eq!(thread::current().id(), current_thread);
            called.set(called.get() + 1);

            let id = called.get();
            let called = Rc::clone(&called);
            let started_tx = started_tx.clone();

            async move {
                started_tx.send(id).unwrap();
                stream.read_exact(&mut [0]).await?;

                assert_eq!(thread::current().id(), current_thread);
                assert!(called.get() >= id);
                stream.write_all(&[id]).await?;

                Ok::<_, io::Error>(())
            }
        });

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();
        let mut first = TcpStream::connect(addr).await.unwrap();
        let mut server = Box::pin(serve(listener, ready(Ok(service))));

        tokio::select! {
            result = &mut server => panic!("server stopped: {result:?}"),
            () = async {
                assert_eq!(started_rx.recv().await, Some(1));

                let mut second = TcpStream::connect(addr).await.unwrap();
                second.write_all(b"x").await.unwrap();

                let mut response = [0];
                second.read_exact(&mut response).await.unwrap();
                assert_eq!(response, [2]);
                assert_eq!(calls.get(), 2);
            } => {}
        }

        drop(server);

        assert_eq!(first.read(&mut [0]).await.unwrap(), 0);
    })
    .await;
}

struct ReadinessGate {
    ready: Cell<bool>,
    waker: RefCell<Option<Waker>>,
    polled: Notify,
    calls: Cell<usize>,
}

impl Service<TcpStream> for ReadinessGate {
    type Response = ();
    type Error = io::Error;
    type Future = Ready<io::Result<()>>;

    fn poll_ready(&self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        if self.ready.get() {
            return Poll::Ready(Ok(()));
        }

        *self.waker.borrow_mut() = Some(cx.waker().clone());
        self.polled.notify_one();

        Poll::Pending
    }

    fn call(&self, _stream: TcpStream) -> Self::Future {
        assert!(self.ready.get());
        self.calls.set(self.calls.get() + 1);

        ready(Ok(()))
    }
}

#[tokio::test(flavor = "local")]
async fn waits_for_service_readiness_before_passing_connection() {
    check_readiness().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn waits_for_service_readiness_on_multi_thread_runtime() {
    check_readiness().await;
}

async fn check_readiness() {
    with_timeout(async {
        let service = Rc::new(ReadinessGate {
            ready: Cell::new(false),
            waker: RefCell::new(None),
            polled: Notify::new(),
            calls: Cell::new(0),
        });

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();

        tokio::select! {
            result = serve(listener, ready(Ok(Rc::clone(&service)))) => panic!("server stopped: {result:?}"),
            () = async {
                service.polled.notified().await;
                assert_eq!(service.calls.get(), 0);

                service.ready.set(true);
                service.waker.borrow_mut().take().unwrap().wake();

                assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
                assert_eq!(service.calls.get(), 1);
            } => {}
        }
    })
    .await;
}

struct FailedReadiness;

impl Service<TcpStream> for FailedReadiness {
    type Response = ();
    type Error = io::Error;
    type Future = Ready<io::Result<()>>;

    fn poll_ready(&self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Err(io::Error::other("service is unavailable")))
    }

    fn call(&self, _stream: TcpStream) -> Self::Future {
        panic!("service must not be called after a readiness error");
    }
}

#[tokio::test(flavor = "local")]
async fn returns_service_readiness_errors() {
    check_readiness_error().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn returns_service_readiness_errors_on_multi_thread_runtime() {
    check_readiness_error().await;
}

async fn check_readiness_error() {
    with_timeout(async {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();

        let err = serve(listener, ready(Ok(FailedReadiness)))
            .await
            .unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert_eq!(err.to_string(), "service is unavailable");
        assert_eq!(client.read(&mut [0]).await.unwrap(), 0);
    })
    .await;
}

#[tokio::test(flavor = "local")]
async fn connection_error_does_not_stop_accepting() {
    check_connection_error().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connection_error_does_not_stop_accepting_on_multi_thread_runtime() {
    check_connection_error().await;
}

async fn check_connection_error() {
    with_timeout(async {
        let calls = Cell::new(0);

        let service = fn_service(move |mut stream: TcpStream| {
            calls.set(calls.get() + 1);
            let id = calls.get();

            async move {
                if id == 1 {
                    return Err(Rc::<str>::from("connection failed"));
                }

                stream.write_all(b"ok").await.unwrap();

                Ok(())
            }
        });

        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::select! {
            result = serve(listener, ready(Ok(service))) => panic!("server stopped: {result:?}"),
            () = async {
                let mut first = TcpStream::connect(addr).await.unwrap();
                assert_eq!(first.read(&mut [0]).await.unwrap(), 0);

                let mut second = TcpStream::connect(addr).await.unwrap();
                let mut response = [0; 2];
                second.read_exact(&mut response).await.unwrap();
                assert_eq!(&response, b"ok");
            } => {}
        }
    })
    .await;
}
