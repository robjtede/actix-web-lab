//! Semantic server-sent events (SSE) responder.
//!
//! Message data and comments can contain multiple lines. CRLF and CR line endings are converted
//! to LF. An ID with NUL, CR, or LF, or an event name with CR or LF, causes the response body to
//! return an encoding error.
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
use bytes::Bytes;
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
    /// Encodes the event in event-stream format.
    fn into_bytes(self) -> Result<Bytes, BoxError> {
        let event = match self {
            Self::Data(Data { id, event, data }) => russe::Event::Message(russe::Message {
                data,
                event,
                id,
                retry: None,
            }),

            Self::Comment(comment) => russe::Event::Comment(comment),
        };

        event
            .into_bytestring()
            .map(ByteString::into_bytes)
            .map_err(Into::into)
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
    /// Enables "keep-alive" messages to be sent in the event stream after a period of inactivity.
    ///
    /// By default, no keep-alive is set up.
    pub fn with_keep_alive(mut self, keep_alive_period: Duration) -> Self {
        let mut int = interval(keep_alive_period);
        int.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        self.keep_alive = Some(int);
        self
    }

    /// Queues the first event to inform the client of a custom retry period.
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
            .content_type(russe::MEDIA_TYPE_STR)
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
            return Poll::Ready(Some(
                russe::Event::Retry(retry)
                    .into_bytestring()
                    .map(ByteString::into_bytes)
                    .map_err(Into::into),
            ));
        }

        if let Poll::Ready(msg) = this.stream.poll_next(cx) {
            return match msg {
                Some(Ok(msg)) => Poll::Ready(Some(msg.into_bytes())),
                Some(Err(err)) => Poll::Ready(Some(Err(err.into()))),
                None => Poll::Ready(None),
            };
        }

        if let Some(keep_alive) = this.keep_alive
            && keep_alive.poll_tick(cx).is_ready()
        {
            return Poll::Ready(Some(
                russe::Event::Comment("keep-alive".into())
                    .into_bytestring()
                    .map(ByteString::into_bytes)
                    .map_err(Into::into),
            ));
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
    use crate::assert_response_matches;

    #[actix_web::test]
    async fn existing_data_api_is_preserved() {
        let mut data = Data::new("old data").id("old ID").event("old event");

        data.set_data("payload");
        data.set_id("42");
        data.set_event("update");

        let json = Data::new_json(serde_json::json!({ "bar": 42 })).unwrap();
        let events = stream::iter([Event::Data(data), json.into()]);
        let sse = Sse::from_infallible_stream(events);

        assert_eq!(
            body::to_bytes(sse).await.unwrap(),
            "id: 42\nevent: update\ndata: payload\n\ndata: {\"bar\":42}\n\n",
        );
    }

    #[actix_web::test]
    async fn message_fields_are_encoded() {
        let events = stream::iter([
            Event::Comment("foo".into()),
            Data::new("\n").into(),
            Data::new("foo").id("42").event("bar").into(),
        ]);
        let sse = Sse::from_infallible_stream(events);

        assert_eq!(
            body::to_bytes(sse).await.unwrap(),
            ": foo\n\ndata: \ndata: \n\nid: 42\nevent: bar\ndata: foo\n\n",
        );
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
    async fn normalizes_line_endings() {
        let events = stream::iter([
            Event::Data(Data::new("first\r\nsecond\rthird\n")),
            Event::Comment("first\r\nsecond\rthird\n".into()),
        ]);
        let sse = Sse::from_infallible_stream(events);

        assert_eq!(
            body::to_bytes(sse).await.unwrap(),
            "data: first\ndata: second\ndata: third\ndata: \n\n: first\n: second\n: third\n: \n\n",
        );
    }

    #[actix_web::test]
    async fn rejects_invalid_message_fields() {
        for (id, event) in [
            ("bad\nid", "update"),
            ("bad\rid", "update"),
            ("bad\0id", "update"),
            ("42", "bad\nevent"),
            ("42", "bad\revent"),
        ] {
            let events = stream::iter([Event::Data(Data::new("payload").id(id).event(event))]);
            let sse = Sse::from_infallible_stream(events);

            let err = body::to_bytes(sse).await.unwrap_err();

            assert!(matches!(
                err.downcast_ref::<russe::Error>(),
                Some(russe::Error::Invalid),
            ));
        }
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
    async fn retry_precedes_stream_events() {
        let events = stream::iter([Event::Comment("first".into())]);
        let sse =
            Sse::from_infallible_stream(events).with_retry_duration(Duration::from_millis(42));

        assert_eq!(
            body::to_bytes(sse).await.unwrap(),
            "retry: 42\n\n: first\n\n",
        );
    }

    #[actix_web::test]
    async fn receiver_errors_are_preserved() {
        let (sender, receiver) = mpsc::channel(1);
        let sse = Sse::from_receiver(receiver);

        sender
            .send(Err(std::io::Error::other("stream failed")))
            .await
            .unwrap();

        let err = body::to_bytes(sse).await.unwrap_err();

        assert_eq!(
            err.downcast_ref::<std::io::Error>().unwrap().to_string(),
            "stream failed",
        );
    }

    #[actix_web::test]
    async fn closed_receiver_ends_with_keep_alive() {
        let (sender, receiver) = mpsc::channel(1);
        let sse = Sse::from_infallible_receiver(receiver).with_keep_alive(Duration::from_millis(4));

        drop(sender);

        assert_eq!(body::to_bytes(sse).await.unwrap(), "");
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
            .send(Event::Data(Data::new("bar").event("foo")))
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
            res => panic!("poll should return keep-alive message, got {res:?}"),
        }

        assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());

        sender.send(Event::Data(Data::new("foo"))).await.unwrap();

        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, "data: foo\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }
    }
}
