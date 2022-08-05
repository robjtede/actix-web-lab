//! Semantic server-sent events (SSE) responder with a channel-like interface.
//!
//! See docs for [`Sse`] and [`SseSender`].

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
use futures_core::ready;
use tokio::sync::mpsc;

use crate::{
    header::{CacheControl, CacheDirective},
    BoxError,
};

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
                    buf.put_slice(text.as_bytes())
                }

                if let Some(text) = event {
                    buf.put_slice(text.as_bytes())
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
}

/// Sender half of a server-sent events stream.
#[derive(Debug)]
pub struct SseSender {
    tx: mpsc::Sender<SseMessage>,
}

impl SseSender {
    /// Send SSE data with associated ID and `event` name.
    pub async fn data_with_id_and_event(
        &self,
        id: Option<impl Into<ByteString>>,
        event: Option<impl Into<ByteString>>,
        data: impl Into<ByteString>,
    ) -> Result<(), ()> {
        if self
            .tx
            .send(SseMessage::Data(SseData {
                id: id.map(Into::into),
                event: event.map(Into::into),
                data: data.into(),
            }))
            .await
            .is_err()
        {
            return Err(());
        }

        Ok(())
    }

    /// Send SSE data with associated ID.
    pub async fn data_with_id(
        &self,
        id: Option<impl Into<ByteString>>,
        data: impl Into<ByteString>,
    ) -> Result<(), ()> {
        self.data_with_id_and_event(id, None::<String>, data).await
    }

    /// Send SSE data with associated `event` name.
    pub async fn data_with_event(
        &self,
        event: Option<impl Into<ByteString>>,
        data: impl Into<ByteString>,
    ) -> Result<(), ()> {
        self.data_with_id_and_event(None::<String>, event, data)
            .await
    }

    /// Send SSE data.
    pub async fn data(&self, data: impl Into<ByteString>) -> Result<(), ()> {
        self.data_with_id_and_event(None::<String>, None::<String>, data)
            .await
    }

    /// Send SSE comment.
    pub async fn comment(&self, text: impl Into<ByteString>) -> Result<(), ()> {
        if self
            .tx
            .send(SseMessage::Comment(text.into()))
            .await
            .is_err()
        {
            return Err(());
        }

        Ok(())
    }
}

/// Server-sent events (`text/event-stream`) responder.
#[doc(
    alias = "server sent events",
    alias = "server-sent events",
    alias = "event stream"
)]
#[derive(Debug)]
pub struct Sse {
    rx: mpsc::Receiver<SseMessage>,
    keep_alive: bool,
    retry: Option<Duration>,
}

impl Sse {
    /// Queues first event message to inform client of custom retry period.
    ///
    /// Browsers default to retry every 3 seconds or so.
    pub fn with_retry_duration(mut self, retry: Duration) -> Self {
        self.retry = Some(retry);
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
        if let Some(retry) = self.retry.take() {
            return Poll::Ready(Some(Ok(SseMessage::retry_to_bytes(retry))));
        }

        match ready!(self.rx.poll_recv(cx)) {
            Some(msg) => Poll::Ready(Some(Ok(msg.into_bytes()))),
            None => return Poll::Ready(None),
        }
    }
}

/// Create server-sent events (SSE) channel-like pair.
pub fn sse() -> (SseSender, Sse) {
    let (tx, rx) = mpsc::channel(10);

    (
        SseSender { tx },
        Sse {
            rx,
            keep_alive: false,
            retry: None,
        },
    )
}

#[cfg(test)]
mod tests {
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
    fn format_line_split() {
        let mut buf = BytesMut::new();
        SseMessage::line_split_with_prefix(&mut buf, "data: ", ByteString::from("foo"));
        assert_eq!(buf, "data: foo\n");

        let mut buf = BytesMut::new();
        SseMessage::line_split_with_prefix(&mut buf, "data: ", ByteString::from("foo\nbar"));
        assert_eq!(buf, "data: foo\ndata: bar\n");
    }
}
