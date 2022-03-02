use actix_web::{
    body::{BodyStream, MessageBody},
    HttpResponse, Responder,
};
use bytes::Bytes;
pub use csv_stream::{QuoteStyle, Terminator};
use futures_core::Stream;
use futures_util::TryStreamExt;
use serde::Serialize;
use std::marker::PhantomData;

/// A buffered CSV serializing body stream.
///
/// This has significant memory efficiency advantages over returning an array of CSV rows when
/// the data set is very large because it avoids buffering the entire response.
///
/// # Examples
/// ```
/// # use actix_web::Responder;
/// # use actix_web_lab::respond::Csv;
/// # use futures_core::Stream;
/// # use serde::Serialize;
/// // Row type to write out in CSV form
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
///     Csv::new(data_stream).into_responder()
/// }
/// ```
pub struct Csv<S> {
    stream: csv_stream::Stream<S>,
}

impl<S> Csv<S> {
    /// Create a new Csv stream with the default confituration.
    /// See [`CsvBuilder`] for details on how to configure the generated CSV
    pub fn new(stream: S) -> Self {
        Self::builder().build(stream)
    }

    /// Create a new [`CsvBuilder`]
    pub fn builder() -> CsvBuilder<S> {
        CsvBuilder::default()
    }
}

/// Configuration for a [`Csv`] output format
#[derive(Debug)]
pub struct CsvBuilder<S> {
    builder: csv_stream::WriterBuilder,
    _stream: PhantomData<S>,
}

impl<S> Default for CsvBuilder<S> {
    fn default() -> Self {
        Self {
            builder: Default::default(),
            _stream: Default::default(),
        }
    }
}

impl<S> CsvBuilder<S> {
    /// The field delimiter to use when writing CSV.
    ///
    /// The default is `b','`.
    ///
    /// # Example
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::Csv;
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { a: &'static str, b: &'static str }
    ///     let row_stream = stream::iter([
    ///         Row{ a: "foo", b: "bar" },
    ///         Row{ a: "x", b: "y" },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .delimiter(b';')
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "a;b\nfoo;bar\nx;y\n");
    ///     Ok(())
    /// }
    /// ```
    pub fn delimiter(&mut self, delimiter: u8) -> &mut Self {
        self.builder.delimiter(delimiter);
        self
    }

    /// Whether to write a header row before writing any other row.
    ///
    /// When this is enabled and the `serialize` method is used to write data
    /// with something that contains field names (i.e., a struct), then a
    /// header row is written containing the field names before any other row
    /// is written.
    ///
    /// This option has no effect when using other methods to write rows. That
    /// is, if you don't use `serialize`, then you must write your header row
    /// explicitly if you want a header row.
    ///
    /// This is enabled by default.
    ///
    /// # Example: with headers
    ///
    /// This shows how the header will be automatically written from the field
    /// names of a struct.
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::{Csv, Terminator};
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row<'a> {
    ///         city: &'a str,
    ///         country: &'a str,
    ///         // Serde allows us to name our headers exactly,
    ///         // even if they don't match our struct field names.
    ///         #[serde(rename = "popcount")]
    ///         population: u64,
    ///     }
    ///
    ///     let row_stream = stream::iter([
    ///         Row {
    ///             city: "Boston",
    ///             country: "United States",
    ///             population: 4628910,
    ///         },
    ///         Row {
    ///             city: "Concord",
    ///             country: "United States",
    ///             population: 42695,
    ///         },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "\
    /// city,country,popcount
    /// Boston,United States,4628910
    /// Concord,United States,42695
    /// ");
    ///     Ok(())
    /// }
    /// ```
    pub fn has_headers(&mut self, yes: bool) -> &mut Self {
        self.builder.has_headers(yes);
        self
    }

    /// The record terminator to use when writing CSV.
    ///
    /// A record terminator can be any single byte. The default is `\n`.
    ///
    /// Note that RFC 4180 specifies that record terminators should be `\r\n`.
    /// To use `\r\n`, use the special `Terminator::CRLF` value.
    ///
    /// # Example: CRLF
    ///
    /// This shows how to use RFC 4180 compliant record terminators.
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::{Csv, Terminator};
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { a: &'static str, b: &'static str }
    ///     let row_stream = stream::iter([
    ///         Row{ a: "foo", b: "bar" },
    ///         Row{ a: "x", b: "y" },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .terminator(Terminator::CRLF)
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "a,b\r\nfoo,bar\r\nx,y\r\n");
    ///     Ok(())
    /// }
    /// ```
    pub fn terminator(&mut self, term: Terminator) -> &mut Self {
        self.builder.terminator(term);
        self
    }

    /// The quoting style to use when writing CSV.
    ///
    /// By default, this is set to `QuoteStyle::Necessary`, which will only
    /// use quotes when they are necessary to preserve the integrity of data.
    ///
    /// Note that unless the quote style is set to `Never`, an empty field is
    /// quoted if it is the only field in a record.
    ///
    /// # Example: non-numeric quoting
    ///
    /// This shows how to quote non-numeric fields only.
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::{Csv, QuoteStyle};
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { a: &'static str, b: usize }
    ///     let row_stream = stream::iter([
    ///         Row{ a: "foo", b: 5 },
    ///         Row{ a: "bar", b: 42 },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .quote_style(QuoteStyle::NonNumeric)
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "\"a\",\"b\"\n\"foo\",5\n\"bar\",42\n");
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Example: never quote
    ///
    /// This shows how the CSV writer can be made to never write quotes, even
    /// if it sacrifices the integrity of the data.
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::{Csv, QuoteStyle};
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { a: &'static str, b: &'static str }
    ///     let row_stream = stream::iter([
    ///         Row{ a: "foo", b: "bar\nbaz" },
    ///         Row{ a: "g'h'i", b: "y\"y\"y" },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .quote_style(QuoteStyle::Never)
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "a,b\nfoo,bar\nbaz\ng'h'i,y\"y\"y\n");
    ///     Ok(())
    /// }
    /// ```
    pub fn quote_style(&mut self, style: QuoteStyle) -> &mut Self {
        self.builder.quote_style(style);
        self
    }

    /// The quote character to use when writing CSV.
    ///
    /// The default is `b'"'`.
    ///
    /// # Example
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::Csv;
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { a: &'static str, b: &'static str }
    ///     let row_stream = stream::iter([
    ///         Row{ a: "foo", b: "bar\nbaz" },
    ///         Row{ a: "g'h'i", b: "y\"y\"y" },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .quote(b'\'')
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "a,b\nfoo,'bar\nbaz'\n'g''h''i',y\"y\"y\n");
    ///     Ok(())
    /// }
    /// ```
    pub fn quote(&mut self, quote: u8) -> &mut Self {
        self.builder.quote(quote);
        self
    }

    /// Enable double quote escapes.
    ///
    /// This is enabled by default, but it may be disabled. When disabled,
    /// quotes in field data are escaped instead of doubled.
    ///
    /// # Example
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::Csv;
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { a: &'static str, b: &'static str }
    ///     let row_stream = stream::iter([
    ///         Row{ a: "foo", b: "bar\"baz" },
    ///         Row{ a: "x", b: "y" },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .double_quote(false)
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "a,b\nfoo,\"bar\\\"baz\"\nx,y\n");
    ///     Ok(())
    /// }
    /// ```
    pub fn double_quote(&mut self, yes: bool) -> &mut Self {
        self.builder.double_quote(yes);
        self
    }

    /// The escape character to use when writing CSV.
    ///
    /// In some variants of CSV, quotes are escaped using a special escape
    /// character like `\` (instead of escaping quotes by doubling them).
    ///
    /// By default, writing these idiosyncratic escapes is disabled, and is
    /// only used when `double_quote` is disabled.
    ///
    /// # Example
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::Csv;
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { a: &'static str, b: &'static str }
    ///     let row_stream = stream::iter([
    ///         Row{ a: "foo", b: "bar\"baz" },
    ///         Row{ a: "x", b: "y" },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder()
    ///         .double_quote(false)
    ///         .escape(b'$')
    ///         .build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "a,b\nfoo,\"bar$\"baz\"\nx,y\n");
    ///     Ok(())
    /// }
    /// ```
    pub fn escape(&mut self, escape: u8) -> &mut Self {
        self.builder.escape(escape);
        self
    }

    /// Create a new stream for creating CSVs from the given stream of rows
    ///
    /// # Example
    ///
    /// ```
    /// # use std::error::Error;
    /// # use actix_web_lab::respond::Csv;
    /// # use serde::Serialize;
    /// # use actix_web::body;
    /// # use futures_util::stream;
    /// # #[actix_web::main]
    /// # async fn main() { example().await.unwrap(); }
    /// async fn example() -> Result<(), Box<dyn Error>> {
    ///     #[derive(Serialize)]
    ///     struct Row { foo: usize, bar: usize }
    ///     let row_stream = stream::iter([
    ///         Row{ foo: 1, bar: 2 },
    ///         Row{ foo: 3, bar: 4 },
    ///     ]);
    ///
    ///     let mut csv_stream = Csv::builder().build(row_stream);
    ///
    ///     let data = body::to_bytes(csv_stream.into_body_stream()).await?;
    ///
    ///     assert_eq!(data, "foo,bar\n1,2\n3,4\n");
    ///     Ok(())
    /// }
    /// ```
    pub fn build(&self, stream: S) -> Csv<S> {
        Csv {
            stream: self.builder.build_stream(stream),
        }
    }
}

impl<S> Csv<S>
where
    S: Stream,
    S::Item: Serialize,
{
    /// Creates a chunked body stream that serializes as CSV on-the-fly.
    pub fn into_body_stream(self) -> impl MessageBody<Error = csv_stream::Error> {
        BodyStream::new(self.stream.map_ok(Bytes::from))
    }

    /// Creates a `Responder` type with a serializing stream and correct `Content-Type` header.
    pub fn into_responder(self) -> impl Responder
    where
        S: 'static,
    {
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
        let csv_body = Csv::new(stream::iter([
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
        let csv_body = Csv::builder()
            .has_headers(false)
            .build(stream::iter([
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
        let csv_body = Csv::builder()
            .quote_style(csv_stream::QuoteStyle::Always)
            .delimiter(b'\t')
            .build(stream::iter([
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

    #[actix_web::test]
    async fn serializes_into_body_lines() {
        let csv_body = Csv::new(stream::iter([
            [123, 456],
            [789, 12],
            [345, 678],
            [901, 234],
            [456, 789],
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
}
