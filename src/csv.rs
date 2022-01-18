use std::{
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
use pin_project_lite::pin_project;
use serde::Serialize;

use crate::utils::MutWriter;

pin_project! {
    /// A buffered CSV serializing stream.
    ///
    /// This has significant memory efficiency advantages over returning an array of CSV objects
    /// when the data set is very large because it avoids buffering the entire response.
    ///
    /// # Examples
    /// ```
    /// # use actix_web::Responder;
    /// # use actix_web_lab::respond::Csv;
    /// # use futures_core::Stream;
    /// fn streaming_data_source() -> impl Stream<Item = [String, String]> {
    ///     // get item stream from source
    ///     # futures_util::stream::empty()
    /// }
    ///
    /// async fn handler() -> impl Responder {
    ///     let data_stream = streaming_data_source();
    ///
    ///     Csv::new(data_stream)
    ///         .into_responder()
    /// }
    /// ```
    pub struct Csv<S> {
        // The wrapped item stream.
        #[pin]
        stream: S,
    }
}

impl Csv<()> {
    /// Returns the CSV MIME type (`text/csv; charset=utf-8`).
    pub fn mime() -> Mime {
        mime::TEXT_CSV_UTF_8
    }
}

impl<S> Csv<S> {
    /// Constructs a new `Csv` from a stream of rows.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }

    /// Creates a chunked body stream that serializes as CSV on-the-fly.
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
            .content_type(mime::TEXT_CSV_UTF_8)
            .message_body(self.into_body_stream())
            .unwrap()
    }
}

impl<S> Stream for Csv<S>
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
            let mut csv_wrt = csv::Writer::from_writer(&mut wrt);
            csv_wrt.serialize(&item).unwrap();
        }

        debug_assert!(!buf.is_empty(), "buffer should not yield an empty chunk");

        Poll::Ready(Some(Ok(buf.freeze())))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, error::Error as StdError, future::Future as _};

    use actix_web::body;
    use futures_util::{future::poll_fn, pin_mut, stream, task::noop_waker, StreamExt as _};

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
        let mut value_stream = Csv::new(stream::empty::<()>());
        assert!(value_stream.next().await.is_none());
        // test that stream is fused
        assert!(value_stream.next().await.is_none());
    }

    #[actix_web::test]
    async fn serializes_chunks() {
        let mut poll_seq = VecDeque::from([
            Poll::Ready(Some([123, 456])),
            Poll::Pending,
            Poll::Ready(Some([789, 12])),
            Poll::Ready(Some([345, 678])),
            Poll::Pending,
            Poll::Pending,
            Poll::Ready(Some([901, 234])),
            Poll::Ready(Some([456, 789])),
            Poll::Pending,
            Poll::Ready(Some([123, 456])),
        ]);

        let stream = Csv::new(stream::poll_fn(|_cx| match poll_seq.pop_front() {
            Some(item) => item,
            None => Poll::Ready(None),
        }));

        pin_mut!(stream);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        assert_poll_next!(stream, cx, Bytes::from("123,456\n"));
        assert_poll_next!(stream, cx, Bytes::from("789,12\n345,678\n"));
        assert_poll_is_pending!(stream, cx);
        assert_poll_next!(stream, cx, Bytes::from("901,234\n456,789\n"));
        assert_poll_next!(stream, cx, Bytes::from("123,456\n"));
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

        let stream = Csv::new(stream::poll_fn(|_cx| match poll_seq.pop_front() {
            Some(item) => item,
            None => Poll::Ready(None),
        }));

        pin_mut!(stream);

        // only yields two of the chunks because the limit is 16 KiB
        let chunk1 = next_stream_chunk!(stream);
        // len + 1 line break per msg
        let exp_len = (ten_kb_str.len() + 1) * 2;
        assert_eq!(chunk1.len(), exp_len);

        let chunk2 = next_stream_chunk!(stream);
        // len + 1 line break per msg
        assert_eq!(chunk2.len(), ten_kb_str.len() + 1);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        assert_poll_is_none!(stream, cx);
    }

    #[actix_web::test]
    async fn serializes_into_body() {
        let ndjson_body = Csv::new(stream::iter([
            [123, 456],
            [789, 12],
            [345, 678],
            [901, 234],
            [456, 789],
        ]))
        .into_body_stream();

        let body_bytes = body::to_bytes(ndjson_body)
            .await
            .map_err(Into::<Box<dyn StdError>>::into)
            .unwrap();

        const EXP_BYTES: &str = "123,456\n\
        789,12\n\
        345,678\n\
        901,234\n\
        456,789\n";

        assert_eq!(body_bytes, EXP_BYTES);
    }
}
