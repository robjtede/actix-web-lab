//! Utilities for AWC 3.

use std::{fmt, io};

use awc_3::{ClientResponse, FrozenClientRequest, error::PayloadError};
use bytes::Bytes;
use bytestring::ByteString;
use futures_util::{Stream, StreamExt as _, TryStreamExt as _, stream::LocalBoxStream};
use tokio::{
    sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel},
    task::{JoinHandle, spawn_local},
};
use tokio_util::{codec::FramedRead, io::StreamReader};

use crate::{Decoder, Error, Event};

mod sealed {
    use super::*;

    pub trait Sealed {}
    impl<S> Sealed for ClientResponse<S> {}
}

/// SSE extension methods for AWC 3.
pub trait AwcExt: sealed::Sealed {
    /// Returns a local stream of server-sent events.
    ///
    /// The stream is not `Send` because AWC responses are not `Send`.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use futures_util::StreamExt as _;
    /// use russe::awc_3::AwcExt as _;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let response = awc_3::Client::default()
    ///     .get("https://sse.dev/test")
    ///     .send()
    ///     .await?;
    /// let mut events = response.sse_stream();
    ///
    /// while let Some(event) = events.next().await {
    ///     println!("{:?}", event?);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    fn sse_stream(self) -> LocalBoxStream<'static, Result<Event, Error>>;
}

impl<S> AwcExt for ClientResponse<S>
where
    S: Stream<Item = Result<Bytes, PayloadError>> + Unpin + 'static,
{
    fn sse_stream(self) -> LocalBoxStream<'static, Result<Event, Error>> {
        let body_stream = self.map_err(io::Error::other);
        let body_reader = StreamReader::new(body_stream);

        let frame_reader = FramedRead::new(body_reader, Decoder::default());

        Box::pin(frame_reader)
    }
}

/// An SSE request manager that delivers events through a channel.
///
/// Runs on an Actix runtime or a Tokio [`LocalSet`](tokio::task::LocalSet).
pub struct Manager {
    req: FrozenClientRequest,
    last_event_id: Option<ByteString>,
    tx: UnboundedSender<Result<Event, Error>>,
    rx: Option<UnboundedReceiver<Result<Event, Error>>>,
}

impl fmt::Debug for Manager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            req,
            last_event_id,
            tx,
            rx,
        } = self;

        f.debug_struct("Manager")
            .field("method", req.get_method())
            .field("uri", req.get_uri())
            .field("last_event_id", last_event_id)
            .field("tx", tx)
            .field("rx", rx)
            .finish()
    }
}

impl Manager {
    /// Constructs a new SSE request manager from a frozen request.
    ///
    /// The request is sent with an empty body. No attempts are made to validate or
    /// modify the given request.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use russe::awc_3::Manager;
    ///
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = awc_3::Client::default();
    /// let request = client.get("https://sse.dev/test").freeze()?;
    /// let mut manager = Manager::new(request);
    /// let (task, mut events) = manager.send().await?;
    /// drop(manager);
    ///
    /// while let Some(event) = events.recv().await {
    ///     println!("{:?}", event?);
    /// }
    /// task.await?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(req: FrozenClientRequest) -> Self {
        let (tx, rx) = unbounded_channel();

        Self {
            req,
            last_event_id: None,
            tx,
            rx: Some(rx),
        }
    }

    /// Sends the request and returns a task handle and a receiver of events.
    ///
    /// Request and stream errors are sent through the receiver.
    ///
    /// # Panics
    ///
    /// Panics if called more than once, or outside an Actix runtime or a Tokio
    /// [`LocalSet`](tokio::task::LocalSet).
    pub async fn send(
        &mut self,
    ) -> Result<(JoinHandle<()>, UnboundedReceiver<Result<Event, Error>>), Error> {
        let rx = self.rx.take().expect("Manager::send must be called once");
        let req = self.req.clone();
        let tx = self.tx.clone();

        let task_handle = spawn_local(async move {
            let res = match req.send().await {
                Ok(res) => res,
                Err(err) => {
                    // AWC request errors can contain sources that are not Send or Sync.
                    let _ = tx.send(Err(io::Error::other(err.to_string()).into()));
                    return;
                }
            };

            let mut stream = res.sse_stream();

            while let Some(ev) = stream.next().await {
                let _ = tx.send(ev);
            }
        });

        Ok((task_handle, rx))
    }

    /// Stores the latest event ID for this manager.
    ///
    /// The stored ID is reserved for reconnect support.
    pub fn commit_id(&mut self, id: impl Into<ByteString>) {
        self.last_event_id = Some(id.into());
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use awc_3::{Client, error::PayloadError, test::TestResponse};
    use bytes::Bytes;
    use futures_util::{StreamExt as _, TryStreamExt as _, stream};
    use tokio::{
        io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader},
        net::TcpListener,
        task::JoinHandle,
    };
    use tokio_stream::wrappers::UnboundedReceiverStream;

    use super::{AwcExt as _, Manager};
    use crate::{Error, Event, Message};

    async fn http_server(response: String) -> (String, JoinHandle<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let addr = listener.local_addr().unwrap();

        let task = tokio::spawn(async move {
            let (socket, _) = listener.accept().await.unwrap();
            let mut socket = BufReader::new(socket);
            let mut line = String::new();

            loop {
                line.clear();
                assert_ne!(socket.read_line(&mut line).await.unwrap(), 0);

                if line == "\r\n" {
                    break;
                }
            }

            socket
                .get_mut()
                .write_all(response.as_bytes())
                .await
                .unwrap();
        });

        (format!("http://{addr}/events"), task)
    }

    async fn sse_server() -> (String, JoinHandle<()>) {
        let body = "id: 42\nevent: update\ndata: hello\n\n: heartbeat\n\n";

        http_server(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len(),
        ))
        .await
    }

    #[actix_rt::test]
    async fn response_stream_decodes_sse_events() {
        let (url, server) = sse_server().await;
        let client = Client::builder().timeout(Duration::from_secs(5)).finish();

        let events = client
            .get(url)
            .send()
            .await
            .unwrap()
            .sse_stream()
            .try_collect::<Vec<_>>()
            .await
            .unwrap();

        assert_eq!(
            events,
            vec![
                Event::Message(Message {
                    data: "hello".into(),
                    event: Some("update".into()),
                    retry: None,
                    id: Some("42".into()),
                }),
                Event::Comment("heartbeat".into()),
            ],
        );

        server.await.unwrap();
    }

    #[tokio::test]
    async fn response_stream_decodes_custom_payload() {
        let response = TestResponse::default()
            .finish()
            .map_body::<_, stream::Empty<Result<Bytes, PayloadError>>>(|_, _| {
                Bytes::from_static(b"data: hello\n\n").into()
            });

        let events = response.sse_stream().try_collect::<Vec<_>>().await.unwrap();

        assert_eq!(
            events,
            vec![Event::Message(Message {
                data: "hello".into(),
                event: None,
                retry: None,
                id: None,
            })],
        );
    }

    #[tokio::test]
    async fn response_stream_decodes_events_before_eof() {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let response = TestResponse::default()
            .finish()
            .map_body(|_, _| UnboundedReceiverStream::new(rx).boxed_local().into());
        let mut events = response.sse_stream();

        tx.send(Ok(Bytes::from_static(b"data: hel"))).unwrap();

        futures_test::assert_stream_pending!(events);

        tx.send(Ok(Bytes::from_static(b"lo\n\n"))).unwrap();

        let event = tokio::time::timeout(Duration::from_secs(5), events.try_next())
            .await
            .unwrap()
            .unwrap();

        assert_eq!(
            event,
            Some(Event::Message(Message {
                data: "hello".into(),
                event: None,
                retry: None,
                id: None,
            })),
        );

        drop(tx);

        assert!(events.try_next().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn response_stream_reports_payload_errors() {
        let response = TestResponse::default().finish().map_body(|_, _| {
            stream::iter([Err(PayloadError::Incomplete(None))])
                .boxed_local()
                .into()
        });

        let error = response.sse_stream().try_next().await.unwrap_err();

        let Error::Io { source } = error else {
            panic!("Expected an I/O error; got: {error:?}");
        };

        assert!(matches!(
            source.get_ref().unwrap().downcast_ref::<PayloadError>(),
            Some(PayloadError::Incomplete(None)),
        ));
    }

    #[tokio::test]
    async fn response_stream_reports_invalid_utf8() {
        let response = TestResponse::default()
            .set_payload(Bytes::from_static(b"data: \xff\n\n"))
            .finish();

        let error = response.sse_stream().try_next().await.unwrap_err();

        assert!(matches!(error, Error::InvalidUtf8 { .. }));
    }

    #[actix_rt::test]
    async fn manager_delivers_sse_events() {
        let (url, server) = sse_server().await;
        let client = Client::builder().timeout(Duration::from_secs(5)).finish();
        let req = client.get(url).freeze().unwrap();
        let mut manager = Manager::new(req);

        let (task, events) = manager.send().await.unwrap();
        drop(manager);

        let events = tokio::time::timeout(
            Duration::from_secs(5),
            UnboundedReceiverStream::new(events).try_collect::<Vec<_>>(),
        )
        .await
        .unwrap()
        .unwrap();

        assert_eq!(
            events,
            vec![
                Event::Message(Message {
                    data: "hello".into(),
                    event: Some("update".into()),
                    retry: None,
                    id: Some("42".into()),
                }),
                Event::Comment("heartbeat".into()),
            ],
        );

        task.await.unwrap();
        server.await.unwrap();
    }

    #[actix_rt::test]
    async fn manager_reports_request_errors() {
        let (url, server) = http_server("invalid HTTP\r\n\r\n".to_owned()).await;
        let client = Client::builder().timeout(Duration::from_secs(5)).finish();
        let req = client.get(url).freeze().unwrap();
        let mut manager = Manager::new(req);

        let (task, mut events) = manager.send().await.unwrap();
        drop(manager);

        let error = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap()
            .unwrap_err();

        assert!(matches!(error, Error::Io { .. }));
        assert!(events.recv().await.is_none());

        task.await.unwrap();
        server.await.unwrap();
    }
}
