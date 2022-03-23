use actix_http::body::{BodyStream, SizedStream};
use bytes::Bytes;
use futures_core::Stream;

use crate::util::InfallibleStream;

/// Constructs a new [`BodyStream`] from an infallible byte chunk stream.
///
/// This could be stabilized into Actix Web as `BodyStream::from_infallible()`.
pub fn new_infallible_body_stream<S: Stream<Item = Bytes>>(
    stream: S,
) -> BodyStream<InfallibleStream<S>> {
    BodyStream::new(InfallibleStream::new(stream))
}

/// Constructs a new [`SizedStream`] from an infallible byte chunk stream.
///
/// This could be stabilized into Actix Web as `SizedStream::from_infallible()`.
pub fn new_infallible_sized_stream<S: Stream<Item = Bytes>>(
    size: u64,
    stream: S,
) -> SizedStream<InfallibleStream<S>> {
    SizedStream::new(size, InfallibleStream::new(stream))
}
