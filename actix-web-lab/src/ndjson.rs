use std::{convert::Infallible, error::Error as StdError, io::Write as _, sync::LazyLock};

use actix_web::{
    HttpResponse, Responder,
    body::{BodyStream, MessageBody},
};
use bytes::{Bytes, BytesMut};
use futures_core::Stream;
use futures_util::TryStreamExt as _;
use mime::Mime;
use pin_project_lite::pin_project;
use serde::Serialize;

use crate::util::{InfallibleStream, MutWriter};

static NDJSON_MIME: LazyLock<Mime> = LazyLock::new(|| "application/x-ndjson".parse().unwrap());

pin_project! {
    /// A buffered [NDJSON] serializing body stream.
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
    ///     NdJson::new_infallible(data_stream)
    ///         .into_responder()
    /// }
    /// ```
    ///
    /// [NDJSON]: https://github.com/ndjson/ndjson-spec
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

impl<S> NdJson<S> {
    /// Constructs a new `NdJson` from an infallible stream of items.
    pub fn new_infallible(stream: S) -> NdJson<InfallibleStream<S>> {
        NdJson::new(InfallibleStream::new(stream))
    }
}

impl<S, T, E> NdJson<S>
where
    S: Stream<Item = Result<T, E>>,
    T: Serialize,
    E: Into<Box<dyn StdError>> + 'static,
{
    /// Creates a chunked body stream that serializes as NDJSON on-the-fly.
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
            .content_type(NDJSON_MIME.clone())
            .message_body(self.into_body_stream())
            .unwrap()
    }

    /// Creates a stream of serialized chunks.
    pub fn into_chunk_stream(self) -> impl Stream<Item = Result<Bytes, E>> {
        self.stream.map_ok(serialize_json_line)
    }
}

impl NdJson<Infallible> {
    /// Returns the NDJSON MIME type (`application/x-ndjson`).
    pub fn mime() -> Mime {
        NDJSON_MIME.clone()
    }
}

fn serialize_json_line(item: impl Serialize) -> Bytes {
    let mut buf = BytesMut::new();
    let mut wrt = MutWriter(&mut buf);

    // serialize JSON line to buffer
    serde_json::to_writer(&mut wrt, &item).unwrap();

    // add line break to buffer
    wrt.write_all(b"\n").unwrap();

    buf.freeze()
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
        let ndjson_body = NdJson::new_infallible(stream::iter(vec![
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
