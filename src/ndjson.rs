use std::{
    io::Write,
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::{
    body::{BodyStream, MessageBody},
    web::{Bytes, BytesMut},
    Error, HttpResponse, Responder,
};
use futures_core::Stream;
use mime::Mime;
use once_cell::sync::Lazy;
use pin_project_lite::pin_project;
use serde::Serialize;

use crate::utils::MutWriter;

static NDJSON_MIME: Lazy<Mime> = Lazy::new(|| "application/x-ndjson".parse().unwrap());

pin_project! {
    /// A buffered [NDJSON] serializing stream.
    ///
    /// This has significant memory efficiency advantages over returning an array of JSON objects
    /// when the data set is very large because it avoids buffering the entire response.
    ///
    /// # Examples
    /// ```
    /// # use actix_web::Responder;
    /// # use actix_web_lab::respond::NdJson;
    /// # use futures_core::Stream;
    /// fn streaming_data_source() -> impl Stream<Item = serde_json::Value> {
    ///     // get item stream from source
    ///     # futures_util::stream::empty()
    /// }
    ///
    /// async fn handler() -> impl Responder {
    ///     let data_stream = streaming_data_source();
    ///
    ///     NdJson::new(data_stream)
    ///         .into_responder()
    /// }
    /// ```
    ///
    /// [NDJSON]: https://ndjson.org/
    pub struct NdJson<S> {
        // The wrapped item stream.
        #[pin]
        stream: S,
    }
}

impl NdJson<()> {
    /// Returns the NDJSON MIME type (`application/x-ndjson`).
    pub fn mime() -> Mime {
        NDJSON_MIME.clone()
    }
}

impl<S> NdJson<S> {
    /// Constructs a new `NdJson` from a stream of items.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }

    /// Creates a chunked body stream that serializes as NDJSON on-the-fly.
    pub fn into_body_stream(self) -> impl MessageBody
    where
        S: Stream,
        S::Item: Serialize,
    {
        BodyStream::new(self)
    }

    /// Creates a `Responder` type with a serializing stream and correct Content-Type header.
    pub fn into_responder(self) -> impl Responder
    where
        S: Stream + 'static,
        S::Item: Serialize,
    {
        HttpResponse::Ok()
            .content_type(NDJSON_MIME.clone())
            .message_body(self.into_body_stream())
            .unwrap()
    }
}

impl<S> Stream for NdJson<S>
where
    S: Stream,
    S::Item: Serialize,
{
    type Item = Result<Bytes, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // buffer up to 16KiB into payload buffer per poll_next
        const MAX_YIELD_CHUNK_SIZE: usize = 16_384;

        let mut this = self.project();

        let mut buf = BytesMut::with_capacity(MAX_YIELD_CHUNK_SIZE);
        let mut wrt = MutWriter(&mut buf);

        loop {
            // if exceeded chunk size then break and return buffer
            if wrt.get_ref().len() > MAX_YIELD_CHUNK_SIZE {
                break;
            }

            let item = match this.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(item)) => item,

                // if end-of-stream and nothing buffered then forward poll
                Poll::Ready(None) if wrt.get_ref().is_empty() => return Poll::Ready(None),

                // otherwise break and return buffer
                Poll::Ready(None) => break,

                // if pending and nothing buffered then forward poll
                Poll::Pending if wrt.get_ref().is_empty() => return Poll::Pending,

                // otherwise break and return buffer
                Poll::Pending => break,
            };

            // serialize JSON line to buffer
            serde_json::to_writer(&mut wrt, &item).unwrap();

            // add line break to buffer
            let _ = wrt.write(b"\n").unwrap();
        }

        debug_assert!(!buf.is_empty(), "buffer should not yield an empty chunk");

        Poll::Ready(Some(Ok(buf.freeze())))
    }
}

#[cfg(test)]
mod test_super {
    use std::{collections::VecDeque, error::Error as StdError, future::Future as _};

    use actix_web::body;
    use futures_util::{future::poll_fn, pin_mut, stream, task::noop_waker, StreamExt as _};
    use serde_json::json;

    use super::*;

    macro_rules! next_stream_chunk {
        ($stream:expr) => {{
            poll_fn(|cx| $stream.as_mut().poll_next(cx))
                .await
                .unwrap()
                .unwrap()
        }};
    }

    macro_rules! assert_poll_next {
        ($stream:expr, $cx:expr, $expected:expr) => {{
            let poll = poll_fn(|cx| $stream.as_mut().poll_next(cx).map_err(|_| ()));
            pin_mut!(poll);
            assert_eq!(poll.poll(&mut $cx), Poll::Ready(Some(Ok($expected))));
        }};
    }

    macro_rules! assert_poll_is_none {
        ($stream:expr, $cx:expr) => {{
            let poll = poll_fn(|cx| $stream.as_mut().poll_next(cx).map_err(|_| ()));
            pin_mut!(poll);
            assert_eq!(poll.poll(&mut $cx), Poll::Ready(None));
        }};
    }

    macro_rules! assert_poll_is_pending {
        ($stream:expr, $cx:expr) => {{
            let poll = poll_fn(|cx| $stream.as_mut().poll_next(cx).map_err(|_| ()));
            pin_mut!(poll);
            assert_eq!(poll.poll(&mut $cx), Poll::Pending);
        }};
    }

    #[actix_web::test]
    async fn empty_stream() {
        let mut value_stream = NdJson::new(stream::empty::<()>());
        assert!(value_stream.next().await.is_none());
        // test that stream is fused
        assert!(value_stream.next().await.is_none());
    }

    #[actix_web::test]
    async fn serializes_chunks() {
        let mut poll_seq = VecDeque::from([
            Poll::Ready(Some(json!(null))),
            Poll::Pending,
            Poll::Ready(Some(json!(null))),
            Poll::Ready(Some(json!(1u32))),
            Poll::Pending,
            Poll::Pending,
            Poll::Ready(Some(json!("123"))),
            Poll::Ready(Some(json!({ "abc": "123" }))),
            Poll::Pending,
            Poll::Ready(Some(json!(["abc", 123u32]))),
        ]);

        let stream = NdJson::new(stream::poll_fn(|_cx| match poll_seq.pop_front() {
            Some(item) => item,
            None => Poll::Ready(None),
        }));

        pin_mut!(stream);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        assert_poll_next!(stream, cx, Bytes::from("null\n"));
        assert_poll_next!(stream, cx, Bytes::from("null\n1\n"));
        assert_poll_is_pending!(stream, cx);
        assert_poll_next!(stream, cx, Bytes::from("\"123\"\n{\"abc\":\"123\"}\n"));
        assert_poll_next!(stream, cx, Bytes::from("[\"abc\",123]\n"));
        assert_poll_is_none!(stream, cx);
    }

    #[actix_web::test]
    async fn chunk_size_limit() {
        let ten_kb_str = "0123456789".repeat(1000);
        assert_eq!(ten_kb_str.len(), 10_000, "test string should be 10KB");

        let mut poll_seq = VecDeque::from([
            Poll::Ready(Some(ten_kb_str.clone())),
            Poll::Ready(Some(ten_kb_str.clone())),
            Poll::Ready(Some(ten_kb_str.clone())),
        ]);

        let stream = NdJson::new(stream::poll_fn(|_cx| match poll_seq.pop_front() {
            Some(item) => item,
            None => Poll::Ready(None),
        }));

        pin_mut!(stream);

        // only yields two of the chunks because the limit is 16 KiB
        let chunk1 = next_stream_chunk!(stream);
        // len + 2 quote parks per msg + 1 line breaks per msg
        let exp_len = (ten_kb_str.len() + 2 + 1) * 2;
        assert_eq!(chunk1.len(), exp_len);

        let chunk2 = next_stream_chunk!(stream);
        // len + 2 quote parks per msg + 1 line breaks per msg
        assert_eq!(chunk2.len(), ten_kb_str.len() + 2 + 1);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert_poll_is_none!(stream, cx);
    }

    #[actix_web::test]
    async fn serializes_into_body() {
        let ndjson_body = NdJson::new(stream::iter(vec![
            json!(null),
            json!(1u32),
            json!("123"),
            json!({ "abc": "123" }),
            json!(["abc", 123u32]),
        ]))
        .into_body_stream();

        let body_bytes = body::to_bytes(ndjson_body)
            .await
            .map_err(Into::<Box<dyn StdError>>::into)
            .unwrap();

        const EXP_BYTES: &str = "null\n\
            1\n\
            \"123\"\n\
            {\"abc\":\"123\"}\n\
            [\"abc\",123]\n";

        assert_eq!(body_bytes, EXP_BYTES);
    }
}
