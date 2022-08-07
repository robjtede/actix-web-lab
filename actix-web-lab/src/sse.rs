//! Semantic server-sent events (SSE) responder with a channel-like interface.
//!
//! See docs for [`sse()`].
//!
//! Usage examples can be found in the examples directory of the source code repo.

use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use actix_web::{
    body::{BodySize, BoxBody, MessageBody},
    HttpRequest, HttpResponse, Responder,
};
use bytes::{BufMut as _, Bytes, BytesMut};
use bytestring::ByteString;
use derive_more::{Display, Error};
use tokio::{
    sync::mpsc,
    time::{interval, Interval},
};

use crate::{
    header::{CacheControl, CacheDirective},
    BoxError,
};

/// Error returned from [`SseSender::send()`].
#[derive(Debug, Display, Error)]
#[display(fmt = "channel closed")]
#[non_exhaustive]
pub struct SseSendError;

/// Error returned from [`SseSender::try_send()`].
///
/// In each case, the original message is returned back to you.
#[derive(Debug, Display, Error)]
#[non_exhaustive]
pub enum SseTrySendError<M> {
    /// The SSE send buffer is full.
    #[display(fmt = "buffer full")]
    Full(M),

    /// The receiving ([`Sse`]) has been dropped, likely because the client disconnected.
    #[display(fmt = "channel closed")]
    Closed(M),
}

/// Server-sent events data message containing a `data` field and optional `id` and `event` fields.
///
/// Since it implements `Into<SseMessage>`, this can be passed directly to [`send`](SseSender::send)
/// or [`try_send`](SseSender::try_send).
#[derive(Debug)]
pub struct SseData {
    id: Option<ByteString>,
    event: Option<ByteString>,
    data: ByteString,
}

impl SseData {
    /// Constructs a new SSE data message with just the `data` field.
    #[must_use]
    pub fn new(data: impl Into<ByteString>) -> Self {
        Self {
            id: None,
            event: None,
            data: data.into(),
        }
    }

    /// Sets `data` field.
    pub fn set_data(&mut self, data: impl Into<ByteString>) {
        self.data = data.into();
    }

    /// Sets `id` field, returning a new data message.
    #[must_use]
    pub fn id(mut self, id: impl Into<ByteString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Sets `id` field.
    pub fn set_id(&mut self, id: impl Into<ByteString>) {
        self.id = Some(id.into());
    }

    /// Sets `event` name field, returning a new data message.
    #[must_use]
    pub fn event(mut self, event: impl Into<ByteString>) -> Self {
        self.event = Some(event.into());
        self
    }

    /// Sets `event` name field.
    pub fn set_event(&mut self, event: impl Into<ByteString>) {
        self.event = Some(event.into());
    }
}

impl From<SseData> for SseMessage {
    fn from(data: SseData) -> Self {
        Self::Data(data)
    }
}

/// Server-sent events message containing one or more fields.
#[derive(Debug)]
pub enum SseMessage {
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
    Data(SseData),

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

impl SseMessage {
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
            SseMessage::Data(SseData { id, event, data }) => {
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

            SseMessage::Comment(text) => Self::line_split_with_prefix(&mut buf, ": ", text),
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

/// Sender half of a server-sent events stream.
#[derive(Debug, Clone)]
pub struct SseSender {
    tx: mpsc::Sender<SseMessage>,
}

impl SseSender {
    /// Send an SSE message.
    ///
    /// # Errors
    /// Errors if the receiving ([`Sse`]) has been dropped, likely because the client disconnected.
    ///
    /// # Examples
    /// ```
    /// #[actix_web::main] async fn test() {
    /// use actix_web_lab::sse::{sse, SseData, SseMessage};
    ///
    /// let (sender, sse_stream) = sse(5);
    /// sender.send(SseData::new("my data").event("my event name")).await.unwrap();
    /// sender.send(SseMessage::Comment("my comment".into())).await.unwrap();
    /// # } test();
    /// ```
    pub async fn send(&self, msg: impl Into<SseMessage>) -> Result<(), SseSendError> {
        self.tx.send(msg.into()).await.map_err(|_| SseSendError)
    }

    /// Tries to send SSE message.
    ///
    /// # Errors
    /// Errors if:
    /// - the the SSE buffer is currently full;
    /// - the receiving ([`Sse`]) has been dropped, likely because the client disconnected.
    ///
    /// # Examples
    /// ```
    /// #[actix_web::main] async fn test() {
    /// use actix_web_lab::sse::{sse, SseData, SseMessage};
    ///
    /// let (sender, sse_stream) = sse(5);
    /// sender.try_send(SseData::new("my data").event("my event name")).unwrap();
    /// sender.try_send(SseMessage::Comment("my comment".into())).unwrap();
    /// # } test();
    /// ```
    pub fn try_send(&self, msg: impl Into<SseMessage>) -> Result<(), SseTrySendError<SseMessage>> {
        self.tx.try_send(msg.into()).map_err(|err| match err {
            mpsc::error::TrySendError::Full(msg) => SseTrySendError::Full(msg),
            mpsc::error::TrySendError::Closed(msg) => SseTrySendError::Closed(msg),
        })
    }

    /// Send SSE data.
    ///
    /// # Errors
    /// Errors if the receiving ([`Sse`]) has been dropped, likely because the client disconnected.
    #[doc(hidden)]
    #[deprecated(since = "0.16.9", note = "Use `SseData` builder API with `send()`.")]
    pub async fn data(&self, data: impl Into<ByteString>) -> Result<(), SseSendError> {
        self.send(SseData::new(data)).await
    }

    /// Send SSE data with associated `event` name.
    ///
    /// # Errors
    /// Errors if the receiving ([`Sse`]) has been dropped, likely because the client disconnected.
    #[doc(hidden)]
    #[deprecated(since = "0.16.9", note = "Use `SseData` builder API with `send()`.")]
    pub async fn data_with_event(
        &self,
        event: impl Into<ByteString>,
        data: impl Into<ByteString>,
    ) -> Result<(), SseSendError> {
        self.send(SseData::new(data).event(event)).await
    }

    /// Send SSE data with associated `id`.
    ///
    /// # Errors
    /// Errors if the receiving ([`Sse`]) has been dropped, likely because the client disconnected.
    #[doc(hidden)]
    #[deprecated(since = "0.16.9", note = "Use `SseData` builder API with `send()`.")]
    pub async fn data_with_id(
        &self,
        id: impl Into<ByteString>,
        data: impl Into<ByteString>,
    ) -> Result<(), SseSendError> {
        self.send(SseData::new(data).id(id)).await
    }

    /// Send SSE comment.
    ///
    /// # Errors
    /// Errors if the receiving ([`Sse`]) has been dropped, likely because the client disconnected.
    #[doc(hidden)]
    #[deprecated(since = "0.16.9", note = "Use `SseMessage` with `send()`.")]
    pub async fn comment(&self, text: impl Into<ByteString>) -> Result<(), SseSendError> {
        self.send(SseMessage::Comment(text.into())).await
    }
}

/// Server-sent events (`text/event-stream`) responder.
#[doc(
    alias = "server sent",
    alias = "server-sent",
    alias = "server sent events",
    alias = "server-sent events",
    alias = "event-stream"
)]
#[derive(Debug)]
pub struct Sse {
    rx: mpsc::Receiver<SseMessage>,
    keep_alive: Option<Interval>,
    retry_interval: Option<Duration>,
}

impl Sse {
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

impl Responder for Sse {
    type Body = BoxBody;

    fn respond_to(self, _req: &HttpRequest) -> HttpResponse<Self::Body> {
        HttpResponse::Ok()
            .content_type(mime::TEXT_EVENT_STREAM)
            .insert_header(CacheControl(vec![CacheDirective::NoCache]))
            .body(self)
    }
}

impl MessageBody for Sse {
    type Error = BoxError;

    fn size(&self) -> BodySize {
        BodySize::Stream
    }

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        if let Some(retry) = self.retry_interval.take() {
            cx.waker().wake_by_ref();
            return Poll::Ready(Some(Ok(SseMessage::retry_to_bytes(retry))));
        }

        if let Poll::Ready(msg) = self.rx.poll_recv(cx) {
            return match msg {
                Some(msg) => Poll::Ready(Some(Ok(msg.into_bytes()))),
                None => Poll::Ready(None),
            };
        }

        if let Some(ref mut keep_alive) = self.keep_alive {
            if keep_alive.poll_tick(cx).is_ready() {
                return Poll::Ready(Some(Ok(SseMessage::keep_alive_bytes())));
            }
        }

        Poll::Pending
    }
}

/// Create server-sent events (SSE) channel-like pair.
///
/// The `buffer` argument controls how many unsent messages can be stored without waiting.
///
/// The first item in the tuple is the sender half. Much like a regular channel, it can be cloned,
/// sent to another thread/task, and send event messages to the response stream. It provides several
/// methods that represent the event-stream format.
///
/// The second item is the responder and can, therefore, be used as a handler return type directly.
/// The stream will be closed after all [senders](SseSender) are dropped.
///
/// Read more about server-sent events in [this MDN article][mdn-sse].
///
/// # Examples
/// ```no_run
/// use std::time::Duration;
/// use actix_web::{Responder, get};
/// use actix_web_lab::sse::sse;
///
/// #[get("/sse")]
/// async fn events() -> impl Responder {
///     let (sender, sse_stream) = sse(10);
///
///     let _ = sender.comment("my comment").await;
///     let _ = sender.data_with_event("chat_msg", "my data").await;
///
///     sse_stream.with_keep_alive(Duration::from_secs(5))
///         .with_retry_duration(Duration::from_secs(10))
/// }
/// ```
///
/// [mdn-sse]: https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events
#[doc(
    alias = "server sent",
    alias = "server-sent",
    alias = "server sent events",
    alias = "server-sent events",
    alias = "event-stream"
)]
pub fn sse(buffer: usize) -> (SseSender, Sse) {
    let (tx, rx) = mpsc::channel(buffer);

    (
        SseSender { tx },
        Sse {
            rx,
            keep_alive: None,
            retry_interval: None,
        },
    )
}

#[cfg(test)]
mod tests {
    use futures_util::{future::poll_fn, task::noop_waker, FutureExt as _};
    use tokio::time::sleep;

    use super::*;

    #[test]
    fn format_retry_message() {
        assert_eq!(
            SseMessage::retry_to_bytes(Duration::from_millis(1)),
            "retry: 1\n\n",
        );
        assert_eq!(
            SseMessage::retry_to_bytes(Duration::from_secs(10)),
            "retry: 10000\n\n",
        );
    }

    #[test]
    fn line_split_format() {
        let mut buf = BytesMut::new();
        SseMessage::line_split_with_prefix(&mut buf, "data: ", ByteString::from("foo"));
        assert_eq!(buf, "data: foo\n");

        let mut buf = BytesMut::new();
        SseMessage::line_split_with_prefix(&mut buf, "data: ", ByteString::from("foo\nbar"));
        assert_eq!(buf, "data: foo\ndata: bar\n");
    }

    #[test]
    fn into_bytes_format() {
        assert_eq!(SseMessage::Comment("foo".into()).into_bytes(), ": foo\n\n");

        assert_eq!(
            SseMessage::Data(SseData {
                id: None,
                event: None,
                data: "foo".into()
            })
            .into_bytes(),
            "data: foo\n\n"
        );

        assert_eq!(
            SseMessage::Data(SseData {
                id: None,
                event: None,
                data: "\n".into()
            })
            .into_bytes(),
            "data: \ndata: \n\n"
        );

        assert_eq!(
            SseMessage::Data(SseData {
                id: Some("42".into()),
                event: None,
                data: "foo".into()
            })
            .into_bytes(),
            "id: 42\ndata: foo\n\n"
        );

        assert_eq!(
            SseMessage::Data(SseData {
                id: None,
                event: Some("bar".into()),
                data: "foo".into()
            })
            .into_bytes(),
            "event: bar\ndata: foo\n\n"
        );

        assert_eq!(
            SseMessage::Data(SseData {
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

        {
            let (_sender, mut sse) = sse(9);
            assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());
        }

        {
            let (_sender, sse) = sse(9);
            let mut sse = sse.with_retry_duration(Duration::from_millis(42));
            match Pin::new(&mut sse).poll_next(&mut cx) {
                Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, "retry: 42\n\n"),
                res => panic!("poll should return retry message, got {res:?}"),
            }
        }
    }

    #[actix_web::test]
    async fn dropping_responder_causes_send_fails() {
        let (sender, sse) = sse(9);
        drop(sse);

        assert!(sender.send(SseData::new("late data")).await.is_err());
    }

    #[actix_web::test]
    async fn messages_are_received_from_sender() {
        let (sender, mut sse) = sse(9);

        assert!(poll_fn(|cx| Pin::new(&mut sse).poll_next(cx))
            .now_or_never()
            .is_none());

        sender.send(SseData::new("bar").event("foo")).await.unwrap();

        match poll_fn(|cx| Pin::new(&mut sse).poll_next(cx)).now_or_never() {
            Some(Some(Ok(bytes))) => assert_eq!(bytes, "event: foo\ndata: bar\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }
    }

    #[actix_web::test]
    async fn keep_alive_is_sent() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let (sender, sse) = sse(9);
        let mut sse = sse.with_keep_alive(Duration::from_millis(4));

        assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());

        sleep(Duration::from_millis(20)).await;

        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, ": keep-alive\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }

        assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());

        sender.send(SseData::new("foo")).await.unwrap();

        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, "data: foo\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }
    }
}
