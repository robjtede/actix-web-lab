//! Semantic server-sent events (SSE) responder
//!
//! # Examples
//! ```no_run
//! use std::{convert::Infallible, time::Duration};
//!
//! use actix_web::{Responder, get};
//! use actix_web_lab::sse;
//!
//! #[get("/from-channel")]
//! async fn from_channel() -> impl Responder {
//!     let (tx, rx) = tokio::sync::mpsc::channel(10);
//!
//!     // note: sender will typically be spawned or handed off somewhere else
//!     let _ = tx.send(sse::Event::Comment("my comment".into())).await;
//!     let _ = tx
//!         .send(sse::Data::new("my data").event("chat_msg").into())
//!         .await;
//!
//!     sse::Sse::from_infallible_receiver(rx).with_retry_duration(Duration::from_secs(10))
//! }
//!
//! #[get("/from-stream")]
//! async fn from_stream() -> impl Responder {
//!     let event_stream = futures_util::stream::iter([Ok::<_, Infallible>(sse::Event::Data(
//!         sse::Data::new("foo"),
//!     ))]);
//!
//!     sse::Sse::from_stream(event_stream).with_keep_alive(Duration::from_secs(5))
//! }
//! ```
//!
//! Complete usage examples can be found in the examples directory of the source code repo.
#![doc(
    alias = "server sent",
    alias = "server-sent",
    alias = "server sent events",
    alias = "server-sent events",
    alias = "event-stream"
)]

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use actix_web::{
    HttpRequest, HttpResponse, Responder,
    body::{BodySize, BoxBody, MessageBody},
    http::header::ContentEncoding,
};
use bytes::{BufMut as _, Bytes, BytesMut};
use bytestring::ByteString;
use futures_core::Stream;
use pin_project_lite::pin_project;
use serde::Serialize;
use tokio::{
    sync::mpsc,
    time::{Interval, interval},
};
use tokio_stream::wrappers::ReceiverStream;

use crate::{
    BoxError,
    header::{CacheControl, CacheDirective},
    util::InfallibleStream,
};

/// Server-sent events data message containing a `data` field and optional `id` and `event` fields.
///
/// # Examples
/// ```
/// # #[actix_web::main] async fn test() {
/// use std::convert::Infallible;
///
/// use actix_web::body;
/// use actix_web_lab::sse;
/// use futures_util::stream;
/// use serde::Serialize;
///
/// #[derive(serde::Serialize)]
/// struct Foo {
///     bar: u32,
/// }
///
/// let sse = sse::Sse::from_stream(stream::iter([
///     Ok::<_, Infallible>(sse::Event::Data(sse::Data::new("foo"))),
///     Ok::<_, Infallible>(sse::Event::Data(
///         sse::Data::new_json(Foo { bar: 42 }).unwrap(),
///     )),
/// ]));
///
/// assert_eq!(
///     body::to_bytes(sse).await.unwrap(),
///     "data: foo\n\ndata: {\"bar\":42}\n\n",
/// );
/// # }; test();
/// ```
#[must_use]
#[derive(Debug, Clone)]
pub struct Data {
    id: Option<ByteString>,
    event: Option<ByteString>,
    data: ByteString,
}

impl Data {
    /// Constructs a new SSE data message with just the `data` field.
    ///
    /// # Examples
    /// ```
    /// use actix_web_lab::sse;
    /// let event = sse::Event::Data(sse::Data::new("foo"));
    /// ```
    pub fn new(data: impl Into<ByteString>) -> Self {
        Self {
            id: None,
            event: None,
            data: data.into(),
        }
    }

    /// Constructs a new SSE data message the `data` field set to `data` serialized as JSON.
    ///
    /// # Examples
    /// ```
    /// use actix_web_lab::sse;
    ///
    /// #[derive(serde::Serialize)]
    /// struct Foo {
    ///     bar: u32,
    /// }
    ///
    /// let event = sse::Event::Data(sse::Data::new_json(Foo { bar: 42 }).unwrap());
    /// ```
    pub fn new_json(data: impl Serialize) -> Result<Self, serde_json::Error> {
        Ok(Self {
            id: None,
            event: None,
            data: serde_json::to_string(&data)?.into(),
        })
    }

    /// Sets `data` field.
    pub fn set_data(&mut self, data: impl Into<ByteString>) {
        self.data = data.into();
    }

    /// Sets `id` field, returning a new data message.
    pub fn id(mut self, id: impl Into<ByteString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets `id` field.
    pub fn set_id(&mut self, id: impl Into<ByteString>) {
        self.id = Some(id.into());
    }

    /// Sets `event` name field, returning a new data message.
    pub fn event(mut self, event: impl Into<ByteString>) -> Self {
        self.event = Some(event.into());
        self
    }

    /// Sets `event` name field.
    pub fn set_event(&mut self, event: impl Into<ByteString>) {
        self.event = Some(event.into());
    }
}

impl From<Data> for Event {
    fn from(data: Data) -> Self {
        Self::Data(data)
    }
}

/// Server-sent events message containing one or more fields.
#[must_use]
#[derive(Debug, Clone)]
pub enum Event {
    /// A `data` message with optional ID and event name.
    ///
    /// Data messages looks like this in the response stream.
    /// ```plain
    /// event: foo
    /// id: 42
    /// data: my data
    ///
    /// data: {
    /// data:   "multiline": "data"
    /// data: }
    /// ```
    Data(Data),

    /// A comment message.
    ///
    /// Comments look like this in the response stream.
    /// ```plain
    /// : my comment
    ///
    /// : another comment
    /// ```
    Comment(ByteString),
}

impl Event {
    /// Splits data into lines and prepend each line with `prefix`.
    fn line_split_with_prefix(buf: &mut BytesMut, prefix: &'static str, data: ByteString) {
        // initial buffer size guess is len(data) + 10 lines of prefix + EOLs + EOF
        buf.reserve(data.len() + (10 * (prefix.len() + 1)) + 1);

        // append prefix + space + line to buffer
        for line in data.split('\n') {
            buf.put_slice(prefix.as_bytes());
            buf.put_slice(line.as_bytes());
            buf.put_u8(b'\n');
        }
    }

    /// Serializes message into event-stream format.
    fn into_bytes(self) -> Bytes {
        let mut buf = BytesMut::new();

        match self {
            Event::Data(Data { id, event, data }) => {
                if let Some(text) = id {
                    buf.put_slice(b"id: ");
                    buf.put_slice(text.as_bytes());
                    buf.put_u8(b'\n');
                }

                if let Some(text) = event {
                    buf.put_slice(b"event: ");
                    buf.put_slice(text.as_bytes());
                    buf.put_u8(b'\n');
                }

                Self::line_split_with_prefix(&mut buf, "data: ", data);
            }

            Event::Comment(text) => Self::line_split_with_prefix(&mut buf, ": ", text),
        }

        // final newline to mark end of message
        buf.put_u8(b'\n');

        buf.freeze()
    }

    /// Serializes retry message into event-stream format.
    fn retry_to_bytes(retry: Duration) -> Bytes {
        Bytes::from(format!("retry: {}\n\n", retry.as_millis()))
    }

    /// Serializes a keep-alive event-stream comment message into bytes.
    const fn keep_alive_bytes() -> Bytes {
        Bytes::from_static(b": keep-alive\n\n")
    }
}

pin_project! {
    /// Server-sent events (`text/event-stream`) responder.
    ///
    /// Constructed using a [Tokio channel](Self::from_receiver) or using your [own
    /// stream](Self::from_stream).
    #[must_use]
    #[derive(Debug)]
    pub struct Sse<S> {
        #[pin]
        stream: S,
        keep_alive: Option<Interval>,
        retry_interval: Option<Duration>,
    }
}

impl<S, E> Sse<S>
where
    S: Stream<Item = Result<Event, E>> + 'static,
    E: Into<BoxError>,
{
    /// Create an SSE response from a stream that yields SSE [Event]s.
    pub fn from_stream(stream: S) -> Self {
        Self {
            stream,
            keep_alive: None,
            retry_interval: None,
        }
    }
}

impl<S> Sse<InfallibleStream<S>>
where
    S: Stream<Item = Event> + 'static,
{
    /// Create an SSE response from an infallible stream that yields SSE [Event]s.
    pub fn from_infallible_stream(stream: S) -> Self {
        Sse::from_stream(InfallibleStream::new(stream))
    }
}

impl<E> Sse<ReceiverStream<Result<Event, E>>>
where
    E: Into<BoxError> + 'static,
{
    /// Create an SSE response from a receiver that yields SSE [Event]s.
    pub fn from_receiver(receiver: mpsc::Receiver<Result<Event, E>>) -> Self {
        Self::from_stream(ReceiverStream::new(receiver))
    }
}

impl Sse<InfallibleStream<ReceiverStream<Event>>> {
    /// Create an SSE response from a receiver that yields SSE [Event]s.
    pub fn from_infallible_receiver(receiver: mpsc::Receiver<Event>) -> Self {
        Self::from_stream(InfallibleStream::new(ReceiverStream::new(receiver)))
    }
}

impl<S> Sse<S> {
    /// Enables "keep-alive" messages to be send in the event stream after a period of inactivity.
    ///
    /// By default, no keep-alive is set up.
    pub fn with_keep_alive(mut self, keep_alive_period: Duration) -> Self {
        let mut int = interval(keep_alive_period);
        int.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        self.keep_alive = Some(int);
        self
    }

    /// Queues first event message to inform client of custom retry period.
    ///
    /// Browsers default to retry every 3 seconds or so.
    pub fn with_retry_duration(mut self, retry: Duration) -> Self {
        self.retry_interval = Some(retry);
        self
    }
}

impl<S, E> Responder for Sse<S>
where
    S: Stream<Item = Result<Event, E>> + 'static,
    E: Into<BoxError>,
{
    type Body = BoxBody;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        HttpResponse::Ok()
            .content_type(mime::TEXT_EVENT_STREAM)
            .insert_header(ContentEncoding::Identity)
            .insert_header(CacheControl(vec![CacheDirective::NoCache]))
            .body(self)
    }
}

impl<S, E> MessageBody for Sse<S>
where
    S: Stream<Item = Result<Event, E>>,
    E: Into<BoxError>,
{
    type Error = BoxError;

    fn size(&self) -> BodySize {
        BodySize::Stream
    }

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        let this = self.project();

        if let Some(retry) = this.retry_interval.take() {
            cx.waker().wake_by_ref();
            return Poll::Ready(Some(Ok(Event::retry_to_bytes(retry))));
        }

        if let Poll::Ready(msg) = this.stream.poll_next(cx) {
            return match msg {
                Some(Ok(msg)) => Poll::Ready(Some(Ok(msg.into_bytes()))),
                Some(Err(err)) => Poll::Ready(Some(Err(err.into()))),
                None => Poll::Ready(None),
            };
        }

        if let Some(keep_alive) = this.keep_alive {
            if keep_alive.poll_tick(cx).is_ready() {
                return Poll::Ready(Some(Ok(Event::keep_alive_bytes())));
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use actix_web::{body, test::TestRequest};
    use futures_util::{FutureExt as _, StreamExt as _, future::poll_fn, stream, task::noop_waker};
    use tokio::time::sleep;

    use super::*;
    use crate::{assert_response_matches, util::InfallibleStream};

    #[test]
    fn format_retry_message() {
        assert_eq!(
            Event::retry_to_bytes(Duration::from_millis(1)),
            "retry: 1\n\n",
        );
        assert_eq!(
            Event::retry_to_bytes(Duration::from_secs(10)),
            "retry: 10000\n\n",
        );
    }

    #[test]
    fn line_split_format() {
        let mut buf = BytesMut::new();
        Event::line_split_with_prefix(&mut buf, "data: ", ByteString::from("foo"));
        assert_eq!(buf, "data: foo\n");

        let mut buf = BytesMut::new();
        Event::line_split_with_prefix(&mut buf, "data: ", ByteString::from("foo\nbar"));
        assert_eq!(buf, "data: foo\ndata: bar\n");
    }

    #[test]
    fn into_bytes_format() {
        assert_eq!(Event::Comment("foo".into()).into_bytes(), ": foo\n\n");

        assert_eq!(
            Event::Data(Data {
                id: None,
                event: None,
                data: "foo".into()
            })
            .into_bytes(),
            "data: foo\n\n"
        );

        assert_eq!(
            Event::Data(Data {
                id: None,
                event: None,
                data: "\n".into()
            })
            .into_bytes(),
            "data: \ndata: \n\n"
        );

        assert_eq!(
            Event::Data(Data {
                id: Some("42".into()),
                event: None,
                data: "foo".into()
            })
            .into_bytes(),
            "id: 42\ndata: foo\n\n"
        );

        assert_eq!(
            Event::Data(Data {
                id: None,
                event: Some("bar".into()),
                data: "foo".into()
            })
            .into_bytes(),
            "event: bar\ndata: foo\n\n"
        );

        assert_eq!(
            Event::Data(Data {
                id: Some("42".into()),
                event: Some("bar".into()),
                data: "foo".into()
            })
            .into_bytes(),
            "id: 42\nevent: bar\ndata: foo\n\n"
        );
    }

    #[test]
    fn retry_is_first_msg() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let mut sse = Sse::from_stream(InfallibleStream::new(tokio_stream::empty()))
            .with_retry_duration(Duration::from_millis(42));
        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, "retry: 42\n\n"),
            res => panic!("poll should return retry message, got {res:?}"),
        }
    }

    #[actix_web::test]
    async fn sse_from_external_streams() {
        let st = stream::empty::<Result<_, Infallible>>();
        let sse = Sse::from_stream(st);
        assert_eq!(body::to_bytes(sse).await.unwrap(), "");

        let st = stream::once(async { Ok::<_, Infallible>(Event::Data(Data::new("foo"))) });
        let sse = Sse::from_stream(st);
        assert_eq!(body::to_bytes(sse).await.unwrap(), "data: foo\n\n");

        let st = stream::repeat(Ok::<_, Infallible>(Event::Data(Data::new("foo")))).take(2);
        let sse = Sse::from_stream(st);
        assert_eq!(
            body::to_bytes(sse).await.unwrap(),
            "data: foo\n\ndata: foo\n\n",
        );
    }

    #[actix_web::test]
    async fn appropriate_headers_are_set_on_responder() {
        let st = stream::empty::<Result<_, Infallible>>();
        let sse = Sse::from_stream(st);

        let res = sse.respond_to(&TestRequest::default().to_http_request());

        assert_response_matches!(res, OK;
            "content-type" => "text/event-stream"
            "content-encoding" => "identity"
            "cache-control" => "no-cache"
        );
    }

    #[actix_web::test]
    async fn messages_are_received_from_sender() {
        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        let mut sse = Sse::from_infallible_receiver(receiver);

        assert!(
            poll_fn(|cx| Pin::new(&mut sse).poll_next(cx))
                .now_or_never()
                .is_none()
        );

        sender
            .send(Data::new("bar").event("foo").into())
            .await
            .unwrap();

        match poll_fn(|cx| Pin::new(&mut sse).poll_next(cx)).now_or_never() {
            Some(Some(Ok(bytes))) => assert_eq!(bytes, "event: foo\ndata: bar\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }
    }

    #[actix_web::test]
    async fn keep_alive_is_sent() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        let mut sse =
            Sse::from_infallible_receiver(receiver).with_keep_alive(Duration::from_millis(4));

        assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());

        sleep(Duration::from_millis(20)).await;

        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, ": keep-alive\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }

        assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());

        sender.send(Data::new("foo").into()).await.unwrap();

        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, "data: foo\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }
    }
}
