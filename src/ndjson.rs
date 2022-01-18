use std::{
    convert::Infallible,
    io::{self, Write as _},
};

use actix_web::{
    body::{BodyStream, MessageBody},
    Error, HttpResponse, Responder,
};
use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use mime::Mime;
use once_cell::sync::Lazy;
use pin_project_lite::pin_project;
use serde::Serialize;

use crate::{buffered_serializing_stream::BufferedSerializingStream, utils::MutWriter};

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

impl<S> NdJson<S> {
    /// Constructs a new `NdJson` from a stream of items.
    pub fn new(stream: S) -> Self {
        Self { stream }
    }
}

impl<S> NdJson<S>
where
    S: Stream,
    S::Item: Serialize,
{
    /// Creates a chunked body stream that serializes as NDJSON on-the-fly.
    pub fn into_body_stream(self) -> impl MessageBody {
        BodyStream::new(BufferedSerializingStream::new(
            self.stream,
            serialize_json_line,
        ))
    }

    /// Creates a `Responder` type with a serializing stream and correct Content-Type header.
    pub fn into_responder(self) -> impl Responder
    where
        S: 'static,
    {
        HttpResponse::Ok()
            .content_type(NDJSON_MIME.clone())
            .message_body(self.into_body_stream())
            .unwrap()
    }

    /// Creates a stream of serialized chunks.
    pub fn into_chunk_stream(self) -> impl Stream<Item = Result<Bytes, Error>> {
        BufferedSerializingStream::new(self.stream, serialize_json_line)
    }
}

impl NdJson<Infallible> {
    /// Returns the NDJSON MIME type (`application/x-ndjson`).
    pub fn mime() -> Mime {
        NDJSON_MIME.clone()
    }
}

fn serialize_json_line<T: Serialize>(
    mut wrt: &mut MutWriter<BytesMut>,
    item: &T,
) -> io::Result<()> {
    // serialize JSON line to buffer
    serde_json::to_writer(&mut wrt, &item)?;

    // add line break to buffer
    wrt.write_all(b"\n")
}

#[cfg(test)]
mod tests {
    use std::error::Error as StdError;

    use actix_web::body;
    use futures_util::stream;
    use serde_json::json;

    use super::*;

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
