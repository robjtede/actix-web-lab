use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::Error;
use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use pin_project_lite::pin_project;

use crate::utils::MutWriter;

pin_project! {
    pub(crate) struct BufferedSerializingStream<S, F> {
        // The wrapped item stream.
        #[pin]
        stream: S,

        serialize_fn: F,
    }
}

impl<S, F> BufferedSerializingStream<S, F>
where
    S: Stream,
    F: FnMut(&mut MutWriter<BytesMut>, &S::Item) -> io::Result<()>,
{
    pub fn new(stream: S, serialize_fn: F) -> Self {
        Self {
            stream,
            serialize_fn,
        }
    }
}

impl<S, F> Stream for BufferedSerializingStream<S, F>
where
    S: Stream,
    F: FnMut(&mut MutWriter<BytesMut>, &S::Item) -> io::Result<()>,
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

            (this.serialize_fn)(&mut wrt, &item).unwrap();
        }

        debug_assert!(!buf.is_empty(), "buffer should not yield an empty chunk");

        Poll::Ready(Some(Ok(buf.freeze())))
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::VecDeque, fmt::Display, future::Future as _, io::Write};

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

    fn serialize_display<T: Display>(wrt: &mut MutWriter<BytesMut>, item: &T) -> io::Result<()> {
        writeln!(wrt, "{}", item)
    }

    #[actix_web::test]
    async fn empty_stream() {
        let mut value_stream =
            BufferedSerializingStream::new(stream::empty::<u32>(), serialize_display);
        assert!(value_stream.next().await.is_none());
        // test that stream is fused
        assert!(value_stream.next().await.is_none());
    }

    #[actix_web::test]
    async fn serializes_chunks() {
        let mut poll_seq = VecDeque::from([
            Poll::Ready(Some(123)),
            Poll::Pending,
            Poll::Ready(Some(789)),
            Poll::Ready(Some(345)),
            Poll::Pending,
            Poll::Pending,
            Poll::Ready(Some(901)),
            Poll::Ready(Some(456)),
            Poll::Pending,
            Poll::Ready(Some(123)),
        ]);

        let inner_stream = stream::poll_fn(|_cx| match poll_seq.pop_front() {
            Some(item) => item,
            None => Poll::Ready(None),
        });

        let stream = BufferedSerializingStream::new(inner_stream, serialize_display);

        pin_mut!(stream);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        assert_poll_next!(stream, cx, Bytes::from("123\n"));
        assert_poll_next!(stream, cx, Bytes::from("789\n345\n"));
        assert_poll_is_pending!(stream, cx);
        assert_poll_next!(stream, cx, Bytes::from("901\n456\n"));
        assert_poll_next!(stream, cx, Bytes::from("123\n"));
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

        let inner_stream = stream::poll_fn(|_cx| match poll_seq.pop_front() {
            Some(item) => item,
            None => Poll::Ready(None),
        });

        let stream = BufferedSerializingStream::new(inner_stream, serialize_display);

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
}
