use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use pin_project_lite::pin_project;

use crate::util::MutWriter;

pin_project! {
    pub(crate) struct BufferedSerializingStream<S, F, E> {
        // The wrapped item stream.
        #[pin]
        stream: S,

        // function that converts stream item to chunk of bytes
        serialize_fn: F,

        // storage for deferred stream error when buffer needs flushing
        error: Option<E>
    }
}

impl<S, F, T, E> BufferedSerializingStream<S, F, E>
where
    S: Stream<Item = Result<T, E>>,
    F: for<'a> FnMut(&mut MutWriter<'a, BytesMut>, &T) -> io::Result<()>,
{
    pub fn new(stream: S, serialize_fn: F) -> Self {
        Self {
            stream,
            serialize_fn,
            error: None,
        }
    }
}

impl<S, F, T, E> Stream for BufferedSerializingStream<S, F, E>
where
    S: Stream<Item = Result<T, E>>,
    F: for<'a> FnMut(&mut MutWriter<'a, BytesMut>, &T) -> io::Result<()>,
{
    type Item = Result<Bytes, E>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // buffer up to 16KiB into payload buffer per poll_next
        const MAX_YIELD_CHUNK_SIZE: usize = 16_384;

        let mut this = self.project();

        // yield a previously stored error
        if let Some(err) = this.error.take() {
            return Poll::Ready(Some(Err(err)));
        }

        let mut buf = BytesMut::with_capacity(MAX_YIELD_CHUNK_SIZE);
        let mut wrt = MutWriter(&mut buf);

        loop {
            // if exceeded chunk size then break and return buffer
            if wrt.get_ref().len() > MAX_YIELD_CHUNK_SIZE {
                break;
            }

            let item = match this.stream.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(item))) => item,

                // if stream error and nothing buffered then return error
                Poll::Ready(Some(Err(err))) if wrt.get_ref().is_empty() => {
                    return Poll::Ready(Some(Err(err)));
                }

                // otherwise store error, break, and return buffer
                // next poll will yield the error
                Poll::Ready(Some(Err(err))) => {
                    *this.error = Some(err);
                    break;
                }

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

    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, ub) = self.stream.size_hint();

        // We cannot say what the lower bound is because of how we buffer items.
        // We can say that the upper bound is the number of inner stream items plus the possible
        // extra error item.
        (0, ub.map(|ub| ub + 1))
    }
}

#[cfg(test)]
mod tests {
    use std::{fmt::Display, future::Future as _, io::Write};

    use futures_util::{
        future::poll_fn, pin_mut, stream, task::noop_waker, StreamExt as _, TryStreamExt,
    };

    use crate::util::{InfallibleStream, PollSeq};

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

    macro_rules! assert_poll_is_error {
        ($stream:expr, $cx:expr) => {{
            let poll = poll_fn(|cx| $stream.as_mut().poll_next(cx));
            pin_mut!(poll);
            let p = poll.poll(&mut $cx);
            assert!(
                matches!(p, Poll::Ready(Some(Err(_)))),
                "Poll was not error: {:?}",
                p
            );
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

    fn serialize_display<T: Display>(
        wrt: &mut MutWriter<'_, BytesMut>,
        item: &T,
    ) -> io::Result<()> {
        writeln!(wrt, "{}", item)
    }

    #[actix_web::test]
    async fn empty_stream() {
        let mut value_stream = BufferedSerializingStream::new(
            InfallibleStream::new(stream::empty::<u32>()),
            serialize_display,
        );
        assert!(value_stream.next().await.is_none());
        // test that stream is fused
        assert!(value_stream.next().await.is_none());
    }

    #[actix_web::test]
    async fn serializes_chunks() {
        let poll_seq = PollSeq::from([
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
        ])
        .into_stream();

        let stream =
            BufferedSerializingStream::new(InfallibleStream::new(poll_seq), serialize_display);
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

        let poll_seq = PollSeq::from([
            Poll::Ready(Some(ten_kb_str.clone())),
            Poll::Ready(Some(ten_kb_str.clone())),
            Poll::Ready(Some(ten_kb_str.clone())),
        ])
        .into_stream();

        let stream =
            BufferedSerializingStream::new(InfallibleStream::new(poll_seq), serialize_display);
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
    async fn error_stops_stream() {
        let poll_seq = PollSeq::from([
            Poll::Ready(Some(Ok(123))),
            Poll::Pending,
            Poll::Ready(Some(Ok(123))),
            Poll::Ready(Some(Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "",
            )))),
        ])
        .into_stream();

        let stream = BufferedSerializingStream::new(poll_seq, serialize_display);
        pin_mut!(stream);

        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        assert_poll_next!(stream, cx, Bytes::from("123\n"));
        assert_poll_next!(stream, cx, Bytes::from("123\n"));
        assert_poll_is_error!(stream, cx);
        assert_poll_is_none!(stream, cx);

        let poll_seq = PollSeq::from([
            Poll::Ready(Some(Ok(123))),
            Poll::Ready(Some(Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "",
            )))),
            Poll::Ready(Some(Ok(123))),
        ])
        .into_stream();

        let stream = BufferedSerializingStream::new(poll_seq, serialize_display);
        pin_mut!(stream);

        assert_poll_next!(stream, cx, Bytes::from("123\n"));
        assert!(stream.try_collect::<Vec<_>>().await.is_err());
    }
}
