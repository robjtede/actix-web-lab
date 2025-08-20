//! Utilities for working with Actix Web types.

// stuff in here comes in and out of usage
#![allow(dead_code)]

use std::{
    convert::Infallible,
    io,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_http::{BoxedPayloadStream, error::PayloadError};
use actix_web::{dev, web::BufMut};
use futures_core::Stream;
use futures_util::StreamExt as _;
use local_channel::mpsc;

/// Returns an effectively cloned payload that supports streaming efficiently.
///
/// The cloned payload:
/// - yields identical chunks;
/// - does not poll ahead of the original;
/// - does not poll significantly slower than the original;
/// - receives an error signal if the original errors, but details are opaque to the copy.
///
/// If the payload is forked in one of the extractors used in a handler, then the original _must_ be
/// read in another extractor or else the request will hang.
pub fn fork_request_payload(orig_payload: &mut dev::Payload) -> dev::Payload {
    const TARGET: &str = concat!(module_path!(), "::fork_request_payload");

    let payload = orig_payload.take();

    let (tx, rx) = mpsc::channel();

    let proxy_stream: BoxedPayloadStream = Box::pin(payload.inspect(move |res| {
        match res {
            Ok(chunk) => {
                tracing::trace!(target: TARGET, "yielding {} byte chunk", chunk.len());
                tx.send(Ok(chunk.clone())).unwrap();
            }

            Err(err) => tx
                .send(Err(PayloadError::Io(io::Error::other(format!(
                    "error from original stream: {err}"
                )))))
                .unwrap(),
        }
    }));

    tracing::trace!(target: TARGET, "creating proxy payload");
    *orig_payload = dev::Payload::from(proxy_stream);

    dev::Payload::Stream {
        payload: Box::pin(rx),
    }
}

/// An `io::Write`r that only requires mutable reference and assumes that there is space available
/// in the buffer for every write operation or that it can be extended implicitly (like
/// `bytes::BytesMut`, for example).
///
/// This is slightly faster (~10%) than `bytes::buf::Writer` in such cases because it does not
/// perform a remaining length check before writing.
pub(crate) struct MutWriter<'a, B>(pub(crate) &'a mut B);

impl<B> MutWriter<'_, B> {
    pub fn get_ref(&self) -> &B {
        self.0
    }
}

impl<B: BufMut> io::Write for MutWriter<'_, B> {
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
