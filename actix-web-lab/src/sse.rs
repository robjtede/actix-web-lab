//! Semantic server-sent events (SSE) responder with a channel-like interface.
//!
//! See docs for [`sse()`].

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

/// Error returned from sender operations when client has disconnected.
#[derive(Debug, Display, Error)]
#[display(fmt = "channel closed")]
#[non_exhaustive]
pub struct SseSendError;

/// Server-sent events data message containing a `data` field and optional `id` and `event` fields.
#[derive(Debug)]
struct SseData {
    id: Option<ByteString>,
    event: Option<ByteString>,
    data: ByteString,
}

/// Server-sent events message containing one or more fields.
#[derive(Debug)]
enum SseMessage {
    Data(SseData),
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
    /// Send SSE data.
    ///
    /// # Errors
    /// Errors if used and the receiving end ([`Sse`]) has been dropped, likely because the client
    /// disconnected.
    pub async fn data(&self, data: impl Into<ByteString>) -> Result<(), SseSendError> {
        self.send_data_message(None::<String>, None::<String>, data)
            .await
    }

    /// Send SSE data with associated `event` name.
    ///
    /// # Errors
    /// Errors if used and the receiving end ([`Sse`]) has been dropped, likely because the client
    /// disconnected.
    pub async fn data_with_event(
        &self,
        event: impl Into<ByteString>,
        data: impl Into<ByteString>,
    ) -> Result<(), SseSendError> {
        self.send_data_message(None::<String>, Some(event), data)
            .await
    }

    /// Send SSE data with associated `id`.
    ///
    /// # Errors
    /// Errors if used and the receiving end ([`Sse`]) has been dropped, likely because the client
    /// disconnected.
    pub async fn data_with_id(
        &self,
        id: impl Into<ByteString>,
        data: impl Into<ByteString>,
    ) -> Result<(), SseSendError> {
        self.send_data_message(Some(id), None::<String>, data).await
    }

    /// Send SSE data with associated `id` and `event` name.
    ///
    /// # Errors
    /// Errors if used and the receiving end ([`Sse`]) has been dropped, likely because the client
    /// disconnected.
    pub async fn data_with_id_and_event(
        &self,
        id: impl Into<ByteString>,
        event: impl Into<ByteString>,
        data: impl Into<ByteString>,
    ) -> Result<(), SseSendError> {
        self.send_data_message(Some(id), Some(event), data).await
    }

    /// Send SSE data message.
    ///
    /// # Errors
    /// Errors if used and the receiving end ([`Sse`]) has been dropped, likely because the client
    /// disconnected.
    async fn send_data_message(
        &self,
        id: Option<impl Into<ByteString>>,
        event: Option<impl Into<ByteString>>,
        data: impl Into<ByteString>,
    ) -> Result<(), SseSendError> {
        let msg = SseMessage::Data(SseData {
            id: id.map(Into::into),
            event: event.map(Into::into),
            data: data.into(),
        });

        self.tx.send(msg).await.map_err(|_| SseSendError)
    }

    /// Send SSE comment.
    ///
    /// # Errors
    /// Errors if used and the receiving end ([`Sse`]) has been dropped, likely because the client
    /// disconnected.
    pub async fn comment(&self, text: impl Into<ByteString>) -> Result<(), SseSendError> {
        self.tx
            .send(SseMessage::Comment(text.into()))
            .await
            .map_err(|_| SseSendError)
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
///     let (sender, sse) = sse();
///
///     let _ = sender.comment("my comment").await;
///     let _ = sender.data_with_event("chat_msg", "my data").await;
///
///     sse.with_keep_alive(Duration::from_secs(5))
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
pub fn sse() -> (SseSender, Sse) {
    let (tx, rx) = mpsc::channel(10);

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
            let (_sender, mut sse) = sse();
            assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());
        }

        {
            let (_sender, sse) = sse();
            let mut sse = sse.with_retry_duration(Duration::from_millis(42));
            match Pin::new(&mut sse).poll_next(&mut cx) {
                Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, "retry: 42\n\n"),
                res => panic!("poll should return retry message, got {res:?}"),
            }
        }
    }

    #[actix_web::test]
    async fn dropping_responder_causes_send_fails() {
        let (sender, sse) = sse();
        drop(sse);

        assert!(sender.data("late data").await.is_err());
    }

    #[actix_web::test]
    async fn messages_are_received_from_sender() {
        let (sender, mut sse) = sse();

        assert!(poll_fn(|cx| Pin::new(&mut sse).poll_next(cx))
            .now_or_never()
            .is_none());

        sender.data_with_event("foo", "bar").await.unwrap();

        match poll_fn(|cx| Pin::new(&mut sse).poll_next(cx)).now_or_never() {
            Some(Some(Ok(bytes))) => assert_eq!(bytes, "event: foo\ndata: bar\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }
    }

    #[actix_web::test]
    async fn keep_alive_is_sent() {
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        let (sender, sse) = sse();
        let mut sse = sse.with_keep_alive(Duration::from_millis(4));

        assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());

        sleep(Duration::from_millis(20)).await;

        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, ": keep-alive\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }

        assert!(Pin::new(&mut sse).poll_next(&mut cx).is_pending());

        sender.data("foo").await.unwrap();

        match Pin::new(&mut sse).poll_next(&mut cx) {
            Poll::Ready(Some(Ok(bytes))) => assert_eq!(bytes, "data: foo\n\n"),
            res => panic!("poll should return data message, got {res:?}"),
        }
    }
}
