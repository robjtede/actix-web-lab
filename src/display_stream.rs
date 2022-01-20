use std::{
    error::Error as StdError,
    fmt,
    io::{self, Write as _},
};

use actix_web::{
    body::{BodyStream, MessageBody},
    HttpResponse, Responder,
};
use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use pin_project_lite::pin_project;

use crate::{
    buffered_serializing_stream::BufferedSerializingStream,
    utils::{InfallibleStream, MutWriter},
};

pin_project! {
    /// A buffered line formatting body stream.
    ///
    /// Each item yielded by the stream will be written to the response body using its
    /// `Display` implementation.
    ///
    /// This has significant memory efficiency advantages over returning an array of lines when the
    /// data set is very large because it avoids buffering the entire response.
    ///
    /// # Examples
    /// ```
    /// # use actix_web::Responder;
    /// # use actix_web_lab::respond::DisplayStream;
    /// # use futures_core::Stream;
    /// fn streaming_data_source() -> impl Stream<Item = u32> {
    ///     // get item stream from source
    ///     # futures_util::stream::empty()
    /// }
    ///
    /// async fn handler() -> impl Responder {
    ///     let data_stream = streaming_data_source();
    ///
    ///     DisplayStream::new_infallible(data_stream)
    ///         .into_responder()
    /// }
    /// ```
    pub struct DisplayStream<S> {
        // The wrapped item stream.
        #[pin]
        stream: S,
    }
}

impl<S> DisplayStream<S> {
    /// Constructs a new `DisplayStream` from a stream of lines.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }
}

impl<S> DisplayStream<S> {
    /// Constructs a new `DisplayStream` from an infallible stream of lines.
    pub fn new_infallible(stream: S) -> DisplayStream<InfallibleStream<S>> {
        DisplayStream::new(InfallibleStream::new(stream))
    }
}

impl<S, T, E> DisplayStream<S>
where
    S: Stream<Item = Result<T, E>>,
    T: fmt::Display,
    E: Into<Box<dyn StdError>> + 'static,
{
    /// Creates a chunked body stream that serializes as CSV on-the-fly.
    pub fn into_body_stream(self) -> impl MessageBody {
        BodyStream::new(self.into_chunk_stream())
    }

    /// Creates a `Responder` type with a serializing stream and correct Content-Type header.
    pub fn into_responder(self) -> impl Responder
    where
        S: 'static,
        T: 'static,
        E: 'static,
    {
        HttpResponse::Ok()
            .content_type(mime::TEXT_PLAIN_UTF_8)
            .message_body(self.into_body_stream())
            .unwrap()
    }

    /// Creates a stream of serialized chunks.
    pub fn into_chunk_stream(self) -> impl Stream<Item = Result<Bytes, E>> {
        BufferedSerializingStream::new(self.stream, write_display)
    }
}

fn write_display<T: fmt::Display>(wrt: &mut MutWriter<BytesMut>, item: &T) -> io::Result<()> {
    writeln!(wrt, "{}", item)
}

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;

    use actix_web::body;
    use futures_util::stream;

    use super::*;

    #[actix_web::test]
    async fn serializes_into_body() {
        let ndjson_body = DisplayStream::new_infallible(stream::iter([123, 789, 345, 901, 456]))
            .into_body_stream();

        let body_bytes = body::to_bytes(ndjson_body)
            .await
            .map_err(Into::<Box<dyn StdError>>::into)
            .unwrap();

        const EXP_BYTES: &str = "123\n\
        789\n\
        345\n\
        901\n\
        456\n";

        assert_eq!(body_bytes, EXP_BYTES);
    }
}
