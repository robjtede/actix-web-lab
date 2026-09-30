//! JSON extractor that uses deser and retains error context.

use std::{
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::{
    FromRequest, HttpMessage, HttpRequest, ResponseError, dev::Payload, http::StatusCode,
};
use derive_more::{Display, Error};
use deser::de::DeserializeOwned;
use deser_path::PathLayer;

use crate::{
    bytes::{BytesBody, BytesPayloadError},
    json_serde::DEFAULT_JSON_LIMIT,
};

/// JSON extractor that uses deser 0.9 and retains error context.
///
/// Requires the `deser` feature. The inner type must implement
/// [`deser::de::DeserializeOwned`]. The default payload size limit is 2MiB. Use the `LIMIT`
/// const generic parameter to set a different limit.
///
/// Accepts JSON content types, including `application/*+json`. Does not read
/// [`actix_web::web::JsonConfig`] or decompress request bodies.
///
/// Deserialization errors retain the original [`deser::Error`], including its kind, message,
/// source, location, and attachments. A [`deser_path::PathLayer`] attaches the field and array
/// index path when available. Read it with `source.attachment::<deser_path::Path>()` in the
/// [`DeserJsonPayloadError::Deserialize`] variant. The path and location also appear in the
/// error message.
///
/// Deserialization errors return HTTP 400. Deser uses the same error kind for some syntax and
/// type errors, so these errors use the same status code.
///
/// # Examples
/// ```
/// use actix_web::post;
/// use actix_web_lab::json::DeserJson;
/// use deser::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     username: String,
/// }
///
/// #[post("/")]
/// async fn index(info: DeserJson<Info>) -> String {
///     format!("Welcome {}!", info.username)
/// }
/// ```
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
    /// Unwraps the inner value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

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
                return Poll::Ready(Err(err.take().expect("future polled after completion")));
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

        let json = ::deser_json::Deserializer::from_slice(&bytes)
            .deserialize_with(|driver| driver.push_layer(PathLayer::new()))
            .map(DeserJson)
            .map_err(|source| DeserJsonPayloadError::Deserialize { source });

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

    /// The JSON is invalid or does not match the target type.
    #[display("JSON deserialization failed: {source}")]
    Deserialize {
        /// Original deser error with its location and path attachment, when available.
        source: deser::Error,
    },

    /// Reading the request payload failed.
    #[display("Failed to read JSON payload: {source}")]
    Payload {
        /// Original payload error.
        source: actix_web::error::PayloadError,
    },
}

impl ResponseError for DeserJsonPayloadError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Overflow { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::ContentType => StatusCode::NOT_ACCEPTABLE,
            Self::Deserialize { .. } => StatusCode::BAD_REQUEST,
            Self::Payload { source } => source.status_code(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use actix_web::{App, error::PayloadError, http::header, test, web};
    use deser::Deserialize;
    use deser_path::{Path, PathSegment};
    use futures_util::stream;

    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Server {
        host: String,
        port: u16,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct Config {
        servers: Vec<Server>,
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

        let DeserJsonPayloadError::Deserialize { source } = &err else {
            panic!("unexpected error: {err}");
        };

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
        assert_eq!(source.kind(), deser::ErrorKind::Unexpected);
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

        let DeserJsonPayloadError::Deserialize { source } = err else {
            panic!("unexpected error: {err}");
        };

        assert_eq!(source.kind(), deser::ErrorKind::MissingField);
        assert_eq!(
            source.attachment::<Path>().unwrap().to_string(),
            "servers[0]"
        );
        assert!(source.message().contains("port"));
    }

    #[actix_web::test]
    async fn rejects_invalid_json_and_trailing_values() {
        for body in [
            &b""[..],
            b"{",
            br#"{"host": "a", "port": 80,}"#,
            br#"{"host": "a", "port": 80} {}"#,
            b"{\"host\": \"\xff\", \"port\": 80}",
        ] {
            let (req, mut payload) = test::TestRequest::default()
                .insert_header(header::ContentType::json())
                .set_payload(web::Bytes::from_static(body))
                .to_http_parts();

            let err = DeserJson::<Server>::from_request(&req, &mut payload)
                .await
                .unwrap_err();

            assert_eq!(err.status_code(), StatusCode::BAD_REQUEST);
            let DeserJsonPayloadError::Deserialize { source } = err else {
                panic!("unexpected error: {err}");
            };
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
        let chunks = stream::iter([
            Ok(web::Bytes::from_static(b"4")),
            Ok(web::Bytes::from_static(b"2")),
        ]);
        let mut payload = Payload::from(Box::pin(chunks)
            as Pin<Box<dyn futures_core::Stream<Item = Result<web::Bytes, PayloadError>>>>);

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
        let chunks = stream::iter([
            Ok(web::Bytes::from_static(b"4")),
            Ok(web::Bytes::from_static(b"2")),
        ]);
        let mut payload = Payload::from(Box::pin(chunks)
            as Pin<Box<dyn futures_core::Stream<Item = Result<web::Bytes, PayloadError>>>>);

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
        let mut payload = Payload::from(Box::pin(chunks)
            as Pin<Box<dyn futures_core::Stream<Item = Result<web::Bytes, PayloadError>>>>);

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
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = test::read_body(response).await;
        let body = std::str::from_utf8(&body).unwrap();
        assert!(body.contains("path: servers[0].port"));
        assert!(body.contains("line 1 column"));
    }
}
