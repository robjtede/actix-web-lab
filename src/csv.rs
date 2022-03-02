use actix_web::{
    body::{BodyStream, MessageBody},
    HttpResponse, Responder,
};
use bytes::Bytes;
use futures_core::Stream;
use futures_util::TryStreamExt;
use serde::Serialize;

/// A buffered CSV serializing body stream.
///
/// This has significant memory efficiency advantages over returning an array of CSV rows when
/// the data set is very large because it avoids buffering the entire response.
///
/// # Examples
/// ```
/// # use actix_web::Responder;
/// # use actix_web_lab::respond::CsvResponder;
/// # use futures_core::Stream;
/// # use serde::Serialize;
/// #[derive(Serialize)]
/// struct Row { a: usize, b: usize }
///
/// fn streaming_data_source() -> impl Stream<Item = Row> {
///     // get item stream from source
///     # futures_util::stream::empty()
/// }
///
/// async fn handler() -> impl Responder {
///     let data_stream = streaming_data_source();
///
///     csv_stream::WriterBuilder::default()
///         .build_stream(data_stream)
///         .into_responder()
/// }
/// ```
pub trait CsvResponder {
    type MessageBody: MessageBody;
    fn into_body_stream(self) -> Self::MessageBody;

    type Responder: Responder;
    fn into_responder(self) -> Self::Responder;
}

impl<S> CsvResponder for csv_stream::Stream<S>
where
    S: Stream + 'static,
    S::Item: Serialize,
{
    type MessageBody =
        BodyStream<futures_util::stream::MapOk<Self, fn(std::vec::Vec<u8>) -> bytes::Bytes>>;
    /// Creates a chunked body stream that serializes as CSV on-the-fly.
    fn into_body_stream(self) -> Self::MessageBody {
        BodyStream::new(self.map_ok(Bytes::from))
    }

    type Responder = HttpResponse<Self::MessageBody>;
    /// Creates a `Responder` type with a serializing stream and correct `Content-Type` header.
    fn into_responder(self) -> Self::Responder {
        HttpResponse::Ok()
            .content_type(mime::TEXT_CSV_UTF_8)
            .message_body(self.into_body_stream())
            .unwrap()
    }
}

#[cfg(test)]
mod tests {
    use actix_web::body;
    use futures_util::stream;

    use super::*;

    #[derive(Serialize)]
    struct Row {
        a: usize,
        b: usize,
    }

    #[actix_web::test]
    async fn serializes_into_body() {
        let csv_body = csv_stream::WriterBuilder::default()
            .build_stream(stream::iter([
                Row { a: 123, b: 456 },
                Row { a: 789, b: 12 },
                Row { a: 345, b: 678 },
                Row { a: 901, b: 234 },
                Row { a: 456, b: 789 },
            ]))
            .into_body_stream();

        let body_bytes = body::to_bytes(csv_body).await.unwrap();

        const EXP_BYTES: &str = "a,b\n\
        123,456\n\
        789,12\n\
        345,678\n\
        901,234\n\
        456,789\n";

        assert_eq!(body_bytes, EXP_BYTES);
    }

    #[actix_web::test]
    async fn serializes_into_body_without_headers() {
        let csv_body = csv_stream::WriterBuilder::default()
            .has_headers(false)
            .build_stream(stream::iter([
                Row { a: 123, b: 456 },
                Row { a: 789, b: 12 },
                Row { a: 345, b: 678 },
                Row { a: 901, b: 234 },
                Row { a: 456, b: 789 },
            ]))
            .into_body_stream();

        let body_bytes = body::to_bytes(csv_body).await.unwrap();

        const EXP_BYTES: &str = "123,456\n\
        789,12\n\
        345,678\n\
        901,234\n\
        456,789\n";

        assert_eq!(body_bytes, EXP_BYTES);
    }

    #[actix_web::test]
    async fn serializes_into_body_with_tabs_and_quotes() {
        let csv_body = csv_stream::WriterBuilder::default()
            .quote_style(csv_stream::QuoteStyle::Always)
            .delimiter(b'\t')
            .build_stream(stream::iter([
                Row { a: 123, b: 456 },
                Row { a: 789, b: 12 },
                Row { a: 345, b: 678 },
                Row { a: 901, b: 234 },
                Row { a: 456, b: 789 },
            ]))
            .into_body_stream();

        let body_bytes = body::to_bytes(csv_body).await.unwrap();

        const EXP_BYTES: &str = "\"a\"\t\"b\"\n\
        \"123\"\t\"456\"\n\
        \"789\"\t\"12\"\n\
        \"345\"\t\"678\"\n\
        \"901\"\t\"234\"\n\
        \"456\"\t\"789\"\n";

        assert_eq!(body_bytes, EXP_BYTES);
    }
}
