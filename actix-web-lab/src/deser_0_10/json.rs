//! JSON extractor that uses deser 0.10 and retains error context.

use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, ready},
};

use ::deser_0_10::{ErrorCategory, de::DeserializeOwned};
use actix_web::{
    FromRequest, HttpMessage, HttpRequest, ResponseError, dev::Payload, http::StatusCode,
};
use derive_more::{Display, Error};
use deser_path_0_10::{Path, PathLayer};

use crate::{
    bytes::{BytesBody, BytesPayloadError},
    json::DEFAULT_JSON_LIMIT,
};

/// JSON extractor with const-generic payload size limit and error paths (using [`deser`]).
///
/// `DeserJson` is used to extract typed data from JSON request payloads with deser.
/// Requires the `deser-0_10` feature.
///
/// # Extractor
/// To extract typed data from a request body, the inner type `T` must implement the
/// [`DeserializeOwned`] trait.
///
/// Use the `LIMIT` const generic parameter to control the payload size limit. The default limit
/// that is exported ([`DEFAULT_JSON_LIMIT`]) is 2MiB.
///
/// Accepts JSON content types, including `application/*+json`. Does not read
/// [`actix_web::web::JsonConfig`] or decompress request bodies.
///
/// ```
/// use actix_web::{error, post, HttpResponse, Responder, ResponseError};
/// use actix_web_lab::deser_0_10::{DeserJson, DeserJsonPayloadError, DEFAULT_JSON_LIMIT};
/// use deser_0_10::Deserialize;
/// use serde::Serialize;
/// use serde_json::json;
///
/// #[derive(Deserialize, Serialize)]
/// #[deser(crate = deser_0_10)]
/// struct Info {
///     username: String,
/// }
///
/// /// Deserialize `Info` from the request body.
/// #[post("/")]
/// async fn index(info: DeserJson<Info>) -> String {
///     format!("Welcome {}!", info.username)
/// }
///
/// const LIMIT_32_MB: usize = 33_554_432;
///
/// /// Deserialize a payload with a higher 32MiB limit.
/// #[post("/big-payload")]
/// async fn big_payload(info: DeserJson<Info, LIMIT_32_MB>) -> String {
///     format!("Welcome {}!", info.username)
/// }
///
/// /// Capture an error that occurred while deserializing the body.
/// #[post("/normal-payload")]
/// async fn normal_payload(
///     res: Result<DeserJson<Info>, DeserJsonPayloadError>,
/// ) -> actix_web::Result<impl Responder> {
///     let item = res.map_err(|err| {
///         eprintln!("failed to deserialize JSON: {err}");
///         let path = match &err {
///             DeserJsonPayloadError::Deserialize { source } => Some(source.path().to_string()),
///             _ => None,
///         };
///
///         let res = HttpResponse::build(err.status_code()).json(json!({
///             "error": "invalid_json",
///             "detail": err.to_string(),
///             "path": path,
///         }));
///         error::InternalError::from_response(err, res)
///     })?;
///
///     Ok(HttpResponse::Ok().json(item.0))
/// }
/// ```
///
/// Deserialization errors expose their field and array index path through
/// [`DeserJsonDeserializeError::path`]. The path is empty when no path context is available.
/// [`DeserJsonDeserializeError::source`] retains the original [`::deser_0_10::Error`], including its
/// kind, category, message, source, location, and typed attachments. A [`deser_path_0_10::PathLayer`]
/// also attaches the path to the original error. The path and location appear in the error
/// message.
///
/// Syntax errors, incomplete JSON, and deser limit errors return HTTP 400. Data errors,
/// including missing fields, type conversion failures, and validation errors, return HTTP 422.
/// Unsupported types, API or configuration errors, and deserialization I/O errors return
/// HTTP 500. The status code uses [`::deser_0_10::Error::category`]. Unknown deserialization error
/// categories return HTTP 500. Exceeding the payload byte limit returns HTTP 413.
///
/// [`deser`]: deser_0_10
/// [`DeserializeOwned`]: ::deser_0_10::de::DeserializeOwned
#[derive(Debug, Display)]
pub struct DeserJson<T, const LIMIT: usize = DEFAULT_JSON_LIMIT>(pub T);

impl<T, const LIMIT: usize> std::ops::Deref for DeserJson<T, LIMIT> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, const LIMIT: usize> std::ops::DerefMut for DeserJson<T, LIMIT> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T, const LIMIT: usize> DeserJson<T, LIMIT> {
    /// Unwraps into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<T: DeserializeOwned, const LIMIT: usize> FromRequest for DeserJson<T, LIMIT> {
    type Error = DeserJsonPayloadError;
    type Future = DeserJsonExtractFut<T, LIMIT>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let can_parse_json =
            req.mime_type().ok().flatten().is_some_and(|mime| {
                mime.subtype() == mime::JSON || mime.suffix() == Some(mime::JSON)
            });

        let body = if can_parse_json {
            Ok(BytesBody::new(req, payload))
        } else {
            Err(Some(DeserJsonPayloadError::ContentType))
        };

        DeserJsonExtractFut {
            body,
            _res: PhantomData,
        }
    }
}

/// Future that extracts a JSON payload with deser.
#[allow(missing_debug_implementations)]
pub struct DeserJsonExtractFut<T, const LIMIT: usize> {
    body: Result<BytesBody<LIMIT>, Option<DeserJsonPayloadError>>,
    _res: PhantomData<fn() -> T>,
}

impl<T: DeserializeOwned, const LIMIT: usize> Future for DeserJsonExtractFut<T, LIMIT> {
    type Output = Result<DeserJson<T, LIMIT>, DeserJsonPayloadError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let body = match &mut this.body {
            Ok(body) => body,
            Err(err) => {
                return Poll::Ready(Err(err
                    .take()
                    .expect("Future should not be polled after completion")));
            }
        };

        let bytes = ready!(Pin::new(body).poll(cx)).map_err(|err| match err {
            BytesPayloadError::OverflowKnownLength { length, limit } => {
                DeserJsonPayloadError::Overflow {
                    limit,
                    length: Some(length),
                }
            }
            BytesPayloadError::Overflow { limit } => DeserJsonPayloadError::Overflow {
                limit,
                length: None,
            },
            BytesPayloadError::Payload(source) => DeserJsonPayloadError::Payload { source },
        })?;

        let json = deser_json_0_10::Deserializer::from_slice(&bytes)
            .deserialize_with(|driver| driver.push_layer(PathLayer::new()))
            .map(DeserJson)
            .map_err(|source| DeserJsonPayloadError::Deserialize {
                source: source.into(),
            });

        Poll::Ready(json)
    }
}

/// Errors that can occur while extracting JSON with deser.
#[derive(Debug, Display, Error)]
#[non_exhaustive]
pub enum DeserJsonPayloadError {
    /// The payload exceeds the configured size limit.
    #[display("JSON payload is larger than allowed (limit: {limit} bytes)")]
    Overflow {
        /// Configured payload size limit.
        limit: usize,

        /// The Content-Length, if sent.
        length: Option<usize>,
    },

    /// The request does not have a JSON content type.
    #[display("Content type error")]
    ContentType,

    /// JSON deserialization failed.
    #[display("JSON deserialization failed: {source}")]
    Deserialize {
        /// Deserialization error with its path and original deser error.
        source: DeserJsonDeserializeError,
    },

    /// Reading the request payload failed.
    #[display("Failed to read JSON payload: {source}")]
    Payload {
        /// Original payload error.
        source: actix_web::error::PayloadError,
    },
}

/// Deserialization errors that can occur while extracting a JSON payload with deser.
#[derive(Debug, Display, Error)]
#[display("{source}")]
pub struct DeserJsonDeserializeError {
    /// Path where the deserialization error occurred.
    path: Path,

    /// Original deserialization error.
    source: ::deser_0_10::Error,
}

impl DeserJsonDeserializeError {
    /// Returns the path at which the deserialization error occurred.
    ///
    /// The path is empty when no path context is available.
    pub fn path(&self) -> impl std::fmt::Display + '_ {
        &self.path
    }

    /// Returns the source error.
    pub fn source(&self) -> &::deser_0_10::Error {
        &self.source
    }
}

impl From<::deser_0_10::Error> for DeserJsonDeserializeError {
    fn from(source: ::deser_0_10::Error) -> Self {
        Self {
            path: source.attachment::<Path>().cloned().unwrap_or_default(),
            source,
        }
    }
}

impl ResponseError for DeserJsonPayloadError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Overflow { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::ContentType => StatusCode::NOT_ACCEPTABLE,
            Self::Deserialize { source } => match source.source().category() {
                ErrorCategory::Syntax | ErrorCategory::Eof | ErrorCategory::Limit => {
                    StatusCode::BAD_REQUEST
                }
                ErrorCategory::Data => StatusCode::UNPROCESSABLE_ENTITY,
                ErrorCategory::Unsupported | ErrorCategory::Usage | ErrorCategory::Io => {
                    StatusCode::INTERNAL_SERVER_ERROR
                }
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            },
            Self::Payload { source } => source.status_code(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error as _, str};

    use ::deser_0_10::{Deserialize, ErrorCategory, ErrorKind};
    use actix_web::{App, error::PayloadError, http::header, test, web};
    use bytes::Bytes;
    use deser_path_0_10::{Path, PathSegment};
    use futures_util::{Stream, stream};

    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    #[deser(crate = ::deser_0_10)]
    struct Server {
        host: String,
        port: u16,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[deser(crate = ::deser_0_10)]
    struct Config {
        servers: Vec<Server>,
    }

    #[actix_web::test]
    async fn deserialization_error_path_is_empty_without_context() {
        let err = DeserJsonDeserializeError::from(::deser_0_10::Error::new(
            ErrorKind::Syntax,
            "invalid JSON",
        ));

        assert!(err.path().to_string().is_empty());
    }

    #[actix_web::test]
    async fn deserialization_error_categories_have_expected_responses() {
        for (kind, status) in [
            (ErrorKind::Syntax, StatusCode::BAD_REQUEST),
            (ErrorKind::EndOfFile, StatusCode::BAD_REQUEST),
            (ErrorKind::LimitExceeded, StatusCode::BAD_REQUEST),
            (ErrorKind::InvalidType, StatusCode::UNPROCESSABLE_ENTITY),
            (
                ErrorKind::UnsupportedType,
                StatusCode::INTERNAL_SERVER_ERROR,
            ),
            (ErrorKind::InvalidState, StatusCode::INTERNAL_SERVER_ERROR),
            (ErrorKind::Configuration, StatusCode::INTERNAL_SERVER_ERROR),
            (ErrorKind::Io, StatusCode::INTERNAL_SERVER_ERROR),
        ] {
            let err = DeserJsonPayloadError::Deserialize {
                source: ::deser_0_10::Error::new(kind, "deserialization failed").into(),
            };

            assert_eq!(err.error_response().status(), status, "{kind:?}");
        }
    }

    #[actix_web::test]
    async fn extracts_json_content_types() {
        for content_type in [
            "application/json",
            "application/json; charset=utf-8",
            "application/vnd.api+json",
        ] {
            let (req, mut payload) = test::TestRequest::default()
                .insert_header((header::CONTENT_TYPE, content_type))
                .set_payload(r#"{"host":"localhost","port":8080}"#)
                .to_http_parts();

            let mut json = DeserJson::<Server>::from_request(&req, &mut payload)
                .await
                .unwrap();

            assert_eq!(json.host, "localhost");
            json.port = 80;
            assert_eq!(
                json.into_inner(),
                Server {
                    host: "localhost".into(),
                    port: 80
                }
            );
        }
    }

    #[actix_web::test]
    async fn retains_nested_error_path_and_location() {
        let body = "{\n  \"servers\": [{\"host\": \"a\", \"port\": 80}, {\"host\": \"b\", \"port\": \"bad\"}]\n}";
        let (req, mut payload) = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(body)
            .to_http_parts();

        let err = DeserJson::<Config>::from_request(&req, &mut payload)
            .await
            .unwrap_err();

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let DeserJsonPayloadError::Deserialize { source } = &err else {
            panic!("unexpected error: {err}");
        };

        assert_eq!(source.path().to_string(), "servers[1].port");

        let source = source.source();
        let path = source.attachment::<Path>().unwrap();
        assert_eq!(path.to_string(), "servers[1].port");
        assert_eq!(
            path.segments(),
            &[
                PathSegment::Key("servers".into()),
                PathSegment::Index(1),
                PathSegment::Key("port".into()),
            ]
        );
        assert_eq!(source.kind(), ErrorKind::InvalidType);
        assert_eq!(source.category(), ErrorCategory::Data);
        assert_eq!(source.line(), Some(2));
        assert_eq!(source.offset(), Some(body.find("\"bad\"").unwrap()));
        assert!(source.column().is_some());
        assert!(source.message().contains("expected u16"));
        assert!(err.to_string().contains("path: servers[1].port"));
        assert_eq!(err.source().unwrap().to_string(), source.to_string());
    }

    #[actix_web::test]
    async fn retains_missing_field_error() {
        let (req, mut payload) = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(r#"{"servers":[{"host":"a"}]}"#)
            .to_http_parts();

        let err = DeserJson::<Config>::from_request(&req, &mut payload)
            .await
            .unwrap_err();

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let DeserJsonPayloadError::Deserialize { source } = err else {
            panic!("unexpected error: {err}");
        };

        assert_eq!(source.path().to_string(), "servers[0]");

        let source = source.source();
        assert_eq!(source.kind(), ErrorKind::MissingField);
        assert_eq!(source.category(), ErrorCategory::Data);
        assert_eq!(
            source.attachment::<Path>().unwrap().to_string(),
            "servers[0]"
        );
        assert!(source.message().contains("port"));
    }

    #[actix_web::test]
    async fn rejects_out_of_range_numbers() {
        for body in ["65536", "-1"] {
            let (req, mut payload) = test::TestRequest::default()
                .insert_header(header::ContentType::json())
                .set_payload(body)
                .to_http_parts();

            let err = DeserJson::<u16>::from_request(&req, &mut payload)
                .await
                .unwrap_err();

            assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

            let DeserJsonPayloadError::Deserialize { source } = err else {
                panic!("unexpected error: {err}");
            };

            assert_eq!(source.source().kind(), ErrorKind::OutOfRange);
            assert_eq!(source.source().category(), ErrorCategory::Data);
        }
    }

    #[actix_web::test]
    async fn rejects_invalid_values() {
        let (req, mut payload) = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(r#""invalid address""#)
            .to_http_parts();

        let err = DeserJson::<std::net::IpAddr>::from_request(&req, &mut payload)
            .await
            .unwrap_err();

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let DeserJsonPayloadError::Deserialize { source } = err else {
            panic!("unexpected error: {err}");
        };

        assert_eq!(source.source().kind(), ErrorKind::InvalidValue);
        assert_eq!(source.source().category(), ErrorCategory::Data);
    }

    #[actix_web::test]
    async fn retains_custom_error_source_and_attachments() {
        #[derive(Debug)]
        struct ParsedPort;

        #[derive(Debug)]
        struct ValidationCode(&'static str);

        impl ::deser_0_10::ErrorAttachment for ValidationCode {}

        impl<'de> Deserialize<'de> for ParsedPort {
            fn deserialize_atom(
                slot: &mut ::deser_0_10::de::Slot<Self>,
                atom: ::deser_0_10::Atom<'_>,
                state: &mut ::deser_0_10::State,
            ) -> Result<(), ::deser_0_10::Error> {
                let mut text = None::<String>;
                String::deserialize_atom(::deser_0_10::de::Slot::wrap(&mut text), atom, state)?;

                text.unwrap().parse::<u16>().map_err(|source| {
                    let mut err = ::deser_0_10::Error::new(ErrorKind::Custom, "invalid port");
                    err.set_source(source);
                    err.set_attachment(ValidationCode("invalid_port"));
                    err
                })?;

                slot.set(Self);
                Ok(())
            }
        }

        let (req, mut payload) = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(r#"["bad"]"#)
            .to_http_parts();

        let err = DeserJson::<Vec<ParsedPort>>::from_request(&req, &mut payload)
            .await
            .unwrap_err();

        assert_eq!(err.status_code(), StatusCode::UNPROCESSABLE_ENTITY);

        let source = err
            .source()
            .unwrap()
            .downcast_ref::<DeserJsonDeserializeError>()
            .unwrap();

        assert_eq!(source.path().to_string(), "[0]");
        assert!(
            std::error::Error::source(source)
                .unwrap()
                .is::<::deser_0_10::Error>()
        );

        let source = source.source();
        assert_eq!(source.kind(), ErrorKind::Custom);
        assert_eq!(source.category(), ErrorCategory::Data);
        assert_eq!(source.message(), "invalid port");
        assert!(source.source().unwrap().is::<std::num::ParseIntError>());
        assert_eq!(
            source.attachment::<ValidationCode>().unwrap().0,
            "invalid_port"
        );
        assert_eq!(source.attachment::<Path>().unwrap().to_string(), "[0]");
        assert_eq!(source.offset(), Some(1));
        assert_eq!(source.line(), Some(1));
        assert_eq!(source.column(), Some(2));
    }

    #[actix_web::test]
    async fn rejects_invalid_json_and_trailing_values() {
        for (body, category) in [
            (&b""[..], ErrorCategory::Eof),
            (b"{", ErrorCategory::Eof),
            (br#"{"host": "a" "port": 80}"#, ErrorCategory::Syntax),
            (br#"{"host": "a", "port": 80,}"#, ErrorCategory::Syntax),
            (br#"{"host": "a", "port": 80} {}"#, ErrorCategory::Syntax),
            (b"{\"host\": \"\xff\", \"port\": 80}", ErrorCategory::Syntax),
        ] {
            let (req, mut payload) = test::TestRequest::default()
                .insert_header(header::ContentType::json())
                .set_payload(Bytes::from_static(body))
                .to_http_parts();

            let err = DeserJson::<Server>::from_request(&req, &mut payload)
                .await
                .unwrap_err();

            assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
            let DeserJsonPayloadError::Deserialize { source } = err else {
                panic!("unexpected error: {err}");
            };

            let source = source.source();
            assert_eq!(source.category(), category);
            assert!(source.line().is_some());
            assert!(source.column().is_some());
        }
    }

    #[actix_web::test]
    async fn rejects_non_json_content_types_without_consuming_payload() {
        for content_type in [None, Some("text/plain"), Some("invalid")] {
            let mut request = test::TestRequest::default().set_payload("42");
            if let Some(content_type) = content_type {
                request = request.insert_header((header::CONTENT_TYPE, content_type));
            }
            let (req, mut payload) = request.to_http_parts();

            let err = DeserJson::<u32>::from_request(&req, &mut payload)
                .await
                .unwrap_err();

            assert!(matches!(err, DeserJsonPayloadError::ContentType));
            assert_eq!(err.status_code(), StatusCode::NOT_ACCEPTABLE);
            assert_eq!(BytesBody::<2>::new(&req, &mut payload).await.unwrap(), "42");
        }
    }

    #[actix_web::test]
    async fn enforces_declared_limit() {
        let (req, mut payload) = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((header::CONTENT_LENGTH, 3))
            .to_http_parts();

        let err = DeserJson::<u32, 2>::from_request(&req, &mut payload)
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DeserJsonPayloadError::Overflow {
                limit: 2,
                length: Some(3)
            }
        ));
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[actix_web::test]
    async fn enforces_streamed_limit() {
        let req = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .to_http_request();
        let chunks = stream::iter([Ok(Bytes::from_static(b"4")), Ok(Bytes::from_static(b"2"))]);
        let mut payload = Payload::from(
            Box::pin(chunks) as Pin<Box<dyn Stream<Item = Result<Bytes, PayloadError>>>>
        );

        let err = DeserJson::<u32, 1>::from_request(&req, &mut payload)
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DeserJsonPayloadError::Overflow {
                limit: 1,
                length: None
            }
        ));
        assert_eq!(err.status_code(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[actix_web::test]
    async fn accepts_streamed_payload_at_limit() {
        let req = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .to_http_request();
        let chunks = stream::iter([Ok(Bytes::from_static(b"4")), Ok(Bytes::from_static(b"2"))]);
        let mut payload = Payload::from(
            Box::pin(chunks) as Pin<Box<dyn Stream<Item = Result<Bytes, PayloadError>>>>
        );

        let json = DeserJson::<u32, 2>::from_request(&req, &mut payload)
            .await
            .unwrap();

        assert_eq!(json.into_inner(), 42);
    }

    #[actix_web::test]
    async fn retains_payload_error() {
        let req = test::TestRequest::default()
            .insert_header(header::ContentType::json())
            .to_http_request();
        let chunks = stream::iter([Err(PayloadError::Incomplete(None))]);
        let mut payload = Payload::from(
            Box::pin(chunks) as Pin<Box<dyn Stream<Item = Result<Bytes, PayloadError>>>>
        );

        let err = DeserJson::<u32>::from_request(&req, &mut payload)
            .await
            .unwrap_err();

        assert!(matches!(
            err,
            DeserJsonPayloadError::Payload {
                source: PayloadError::Incomplete(None)
            }
        ));
        assert_eq!(
            err.status_code(),
            PayloadError::Incomplete(None).status_code()
        );
        assert!(err.source().unwrap().is::<PayloadError>());
    }

    #[actix_web::test]
    async fn handler_response_includes_error_context() {
        let app = test::init_service(App::new().route(
            "/",
            web::post().to(|json: DeserJson<Config>| async move { json.servers.len().to_string() }),
        ))
        .await;
        let request = test::TestRequest::post()
            .insert_header(header::ContentType::json())
            .set_payload(r#"{"servers":[{"host":"a","port":"bad"}]}"#)
            .to_request();

        let response = test::call_service(&app, request).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = test::read_body(response).await;
        let body = str::from_utf8(&body).unwrap();
        assert!(body.contains("path: servers[0].port"));
        assert!(body.contains("line 1 column"));
    }
}
