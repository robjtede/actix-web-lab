use std::{convert::Infallible, io};

use actix_web::{
    body::{BodyStream, MessageBody},
    Error, HttpResponse, Responder,
};
use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use mime::Mime;
use pin_project_lite::pin_project;
use serde::Serialize;

use crate::{buffered_serializing_stream::BufferedSerializingStream, utils::MutWriter};

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
    /// fn streaming_data_source() -> impl Stream<Item = [String; 2]> {
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

impl<S> Csv<S> {
    /// Constructs a new `Csv` from a stream of rows.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }
}

impl<S> Csv<S>
where
    S: Stream,
    S::Item: Serialize,
{
    /// Creates a chunked body stream that serializes as CSV on-the-fly.
    pub fn into_body_stream(self) -> impl MessageBody {
        BodyStream::new(BufferedSerializingStream::new(
            self.stream,
            serialize_csv_row,
        ))
    }

    /// Creates a `Responder` type with a serializing stream and correct Content-Type header.
    pub fn into_responder(self) -> impl Responder
    where
        S: 'static,
    {
        HttpResponse::Ok()
            .content_type(mime::TEXT_CSV_UTF_8)
            .message_body(self.into_body_stream())
            .unwrap()
    }

    /// Creates a stream of serialized chunks.
    pub fn into_chunk_stream(self) -> impl Stream<Item = Result<Bytes, Error>> {
        BufferedSerializingStream::new(self.stream, serialize_csv_row)
    }
}

impl Csv<Infallible> {
    /// Returns the CSV MIME type (`text/csv; charset=utf-8`).
    pub fn mime() -> Mime {
        mime::TEXT_CSV_UTF_8
    }
}

fn serialize_csv_row<T: Serialize>(wrt: &mut MutWriter<BytesMut>, item: &T) -> io::Result<()> {
    // serialize CSV row to buffer
    let mut csv_wrt = csv::Writer::from_writer(wrt);
    csv_wrt.serialize(&item).unwrap();
    csv_wrt.flush()
}

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;

    use actix_web::body;
    use futures_util::stream;

    use super::*;

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
