use std::{
    convert::Infallible,
    io,
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::web::BufMut;
use futures_core::{ready, Stream};

/// An `io::Write`r that only requires mutable reference and assumes that there is space available
/// in the buffer for every write operation or that it can be extended implicitly (like
/// `bytes::BytesMut`, for example).
///
/// This is slightly faster (~10%) than `bytes::buf::Writer` in such cases because it does not
/// perform a remaining length check before writing.
pub(crate) struct MutWriter<'a, B>(pub(crate) &'a mut B);

impl<'a, B> MutWriter<'a, B> {
    pub fn get_ref(&self) -> &B {
        self.0
    }
}

impl<'a, B: BufMut> io::Write for MutWriter<'a, B> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.put_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pin_project_lite::pin_project! {
    /// Converts stream with item `T` into `Result<T, Infallible>`.
    pub struct InfallibleStream<S> {
        #[pin]
        stream: S,
    }
}

impl<S> InfallibleStream<S> {
    /// Constructs new `InfallibleStream` stream.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }
}

impl<S: Stream> Stream for InfallibleStream<S> {
    type Item = Result<S::Item, Infallible>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Poll::Ready(ready!(self.project().stream.poll_next(cx)).map(Ok))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.stream.size_hint()
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub(crate) struct PollSeq<T> {
    seq: std::collections::VecDeque<T>,
}

#[cfg(test)]
mod poll_seq_impls {
    use std::collections::VecDeque;

    use futures_util::stream;

    use super::*;

    impl<T> PollSeq<T> {
        pub fn new(seq: VecDeque<T>) -> Self {
            Self { seq }
        }
    }

    impl<T> PollSeq<Poll<Option<T>>> {
        pub fn into_stream(mut self) -> impl Stream<Item = T> {
            stream::poll_fn(move |_cx| match self.seq.pop_front() {
                Some(item) => item,
                None => Poll::Ready(None),
            })
        }
    }

    impl<T, const N: usize> From<[T; N]> for PollSeq<T> {
        fn from(seq: [T; N]) -> Self {
            Self::new(VecDeque::from(seq))
        }
    }
}
