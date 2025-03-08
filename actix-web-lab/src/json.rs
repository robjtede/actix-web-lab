//! JSON extractor with const-generic payload size limit.

use std::{
    fmt,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::{
    FromRequest, HttpMessage, HttpRequest, ResponseError, dev::Payload, http::header, web,
};
use derive_more::{Display, Error};
use futures_core::Stream as _;
use http::StatusCode;
use serde::de::DeserializeOwned;
use tracing::debug;

/// Default JSON payload size limit of 2MiB.
pub const DEFAULT_JSON_LIMIT: usize = 2_097_152;

/// JSON extractor with const-generic payload size limit.
///
/// `Json` is used to extract typed data from JSON request payloads.
///
/// # Extractor
/// To extract typed data from a request body, the inner type `T` must implement the
/// [`serde::Deserialize`] trait.
///
/// Use the `LIMIT` const generic parameter to control the payload size limit. The default limit
/// that is exported (`DEFAULT_LIMIT`) is 2MiB.
///
/// ```
/// use actix_web::{error, post, App, HttpRequest, HttpResponse, Responder};
/// use actix_web_lab::extract::{Json, JsonPayloadError, DEFAULT_JSON_LIMIT};
/// use serde::{Deserialize, Serialize};
/// use serde_json::json;
///
/// #[derive(Deserialize, Serialize)]
/// struct Info {
///     username: String,
/// }
///
/// /// Deserialize `Info` from request's body.
/// #[post("/")]
/// async fn index(info: Json<Info>) -> String {
///     format!("Welcome {}!", info.username)
/// }
///
/// const LIMIT_32_MB: usize = 33_554_432;
///
/// /// Deserialize payload with a higher 32MiB limit.
/// #[post("/big-payload")]
/// async fn big_payload(info: Json<Info, LIMIT_32_MB>) -> String {
///     format!("Welcome {}!", info.username)
/// }
///
/// /// Capture the error that may have occurred when deserializing the body.
/// #[post("/normal-payload")]
/// async fn normal_payload(
///     res: Result<Json<Info>, JsonPayloadError>,
///     req: HttpRequest,
/// ) -> actix_web::Result<impl Responder> {
///     let item = res.map_err(|err| {
///         eprintln!("failed to deserialize JSON: {err}");
///         let res = HttpResponse::BadGateway().json(json!({
///             "error": "invalid_json",
///             "detail": err.to_string(),
///         }));
///         error::InternalError::from_response(err, res)
///     })?;
///
///     Ok(HttpResponse::Ok().json(item.0))
/// }
/// ```
#[derive(Debug)]
// #[derive(Debug, Deref, DerefMut, Display)]
pub struct Json<T, const LIMIT: usize = DEFAULT_JSON_LIMIT>(pub T);

mod waiting_on_derive_more_to_start_using_syn_2_due_to_proc_macro_panic {
    use super::*;

    impl<T, const LIMIT: usize> std::ops::Deref for Json<T, LIMIT> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T, const LIMIT: usize> std::ops::DerefMut for Json<T, LIMIT> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T: std::fmt::Display, const LIMIT: usize> std::fmt::Display for Json<T, LIMIT> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.0, f)
        }
    }
}

impl<T, const LIMIT: usize> Json<T, LIMIT> {
    /// Unwraps into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<T: DeserializeOwned, const LIMIT: usize> FromRequest for Json<T, LIMIT> {
    type Error = JsonPayloadError;
    type Future = JsonExtractFut<T, LIMIT>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        JsonExtractFut {
            req: Some(req.clone()),
            fut: JsonBody::new(req, payload),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct JsonExtractFut<T, const LIMIT: usize> {
    req: Option<HttpRequest>,
    fut: JsonBody<T, LIMIT>,
}

impl<T: DeserializeOwned, const LIMIT: usize> Future for JsonExtractFut<T, LIMIT> {
    type Output = Result<Json<T, LIMIT>, JsonPayloadError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let res = ready!(Pin::new(&mut this.fut).poll(cx));

        let res = match res {
            Err(err) => {
                let req = this.req.take().unwrap();
                debug!(
                    "Failed to deserialize Json<{}> from payload in handler: {}",
                    core::any::type_name::<T>(),
                    req.match_name().unwrap_or_else(|| req.path())
                );

                Err(err)
            }
            Ok(data) => Ok(Json(data)),
        };

        Poll::Ready(res)
    }
}

/// Future that resolves to some `T` when parsed from a JSON payload.
///
/// Can deserialize any type `T` that implements [`Deserialize`][serde::Deserialize].
///
/// Returns error if:
/// - `Content-Type` is not `application/json`.
/// - `Content-Length` is greater than `LIMIT`.
/// - The payload, when consumed, is not valid JSON.
pub enum JsonBody<T, const LIMIT: usize> {
    Error(Option<JsonPayloadError>),
    Body {
        /// Length as reported by `Content-Length` header, if present.
        #[allow(dead_code)]
        length: Option<usize>,
        // #[cfg(feature = "__compress")]
        // payload: Decompress<Payload>,
        // #[cfg(not(feature = "__compress"))]
        payload: Payload,
        buf: web::BytesMut,
        _res: PhantomData<T>,
    },
}

impl<T, const LIMIT: usize> Unpin for JsonBody<T, LIMIT> {}

impl<T: DeserializeOwned, const LIMIT: usize> JsonBody<T, LIMIT> {
    /// Create a new future to decode a JSON request payload.
    pub fn new(req: &HttpRequest, payload: &mut Payload) -> Self {
        // check content-type
        let can_parse_json = if let Ok(Some(mime)) = req.mime_type() {
            mime.subtype() == mime::JSON || mime.suffix() == Some(mime::JSON)
        } else {
            false
        };

        if !can_parse_json {
            return JsonBody::Error(Some(JsonPayloadError::ContentType));
        }

        let length = req
            .headers()
            .get(&header::CONTENT_LENGTH)
            .and_then(|l| l.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());

        let payload = payload.take();

        if let Some(len) = length {
            if len > LIMIT {
                return JsonBody::Error(Some(JsonPayloadError::Overflow {
                    limit: LIMIT,
                    length: Some(len),
                }));
            }
        }

        JsonBody::Body {
            length,
            payload,
            buf: web::BytesMut::with_capacity(8192),
            _res: PhantomData,
        }
    }
}

impl<T: DeserializeOwned, const LIMIT: usize> Future for JsonBody<T, LIMIT> {
    type Output = Result<T, JsonPayloadError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match this {
            JsonBody::Body { buf, payload, .. } => loop {
                let res = ready!(Pin::new(&mut *payload).poll_next(cx));

                match res {
                    Some(chunk) => {
                        let chunk =
                            chunk.map_err(|err| JsonPayloadError::Payload { source: err })?;

                        let buf_len = buf.len() + chunk.len();
                        if buf_len > LIMIT {
                            return Poll::Ready(Err(JsonPayloadError::Overflow {
                                limit: LIMIT,
                                length: None,
                            }));
                        } else {
                            buf.extend_from_slice(&chunk);
                        }
                    }

                    None => {
                        let mut de = serde_json::Deserializer::from_slice(buf);
                        let json = serde_path_to_error::deserialize(&mut de).map_err(|err| {
                            JsonPayloadError::Deserialize {
                                source: JsonDeserializeError {
                                    path: err.path().clone(),
                                    source: err.into_inner(),
                                },
                            }
                        })?;

                        return Poll::Ready(Ok(json));
                    }
                }
            },

            JsonBody::Error(err) => Poll::Ready(Err(err.take().unwrap())),
        }
    }
}

/// A set of errors that can occur during parsing json payloads
#[derive(Debug, Display, Error)]
#[non_exhaustive]
pub enum JsonPayloadError {
    /// Payload size is bigger than allowed header set.
    #[display(
        "JSON payload {}is larger than allowed (limit: {limit} bytes)",
        length.map(|length| format!("({length} bytes) ")).unwrap_or("".to_owned()),
    )]
    Overflow {
        /// Configured payload size limit.
        limit: usize,

        /// The Content-Length, if sent.
        length: Option<usize>,
    },

    /// Content type error.
    #[display("Content type error")]
    ContentType,

    /// Deserialization error.
    #[display("Deserialization error")]
    Deserialize {
        /// Deserialization error.
        source: JsonDeserializeError,
    },

    /// Payload error.
    #[display("Error that occur during reading payload")]
    Payload {
        /// Payload error.
        source: actix_web::error::PayloadError,
    },
}

/// Deserialization errors that can occur during parsing query strings.
#[derive(Debug, Error)]
pub struct JsonDeserializeError {
    /// Path where deserialization error occurred.
    path: serde_path_to_error::Path,

    /// Deserialization error.
    source: serde_json::Error,
}

impl JsonDeserializeError {
    /// Returns the path at which the deserialization error occurred.
    pub fn path(&self) -> impl fmt::Display + '_ {
        &self.path
    }

    /// Returns the source error.
    pub fn source(&self) -> &serde_json::Error {
        &self.source
    }
}

impl fmt::Display for JsonDeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("JSON deserialization failed")?;

        if self.path.iter().len() > 0 {
            write!(f, " at path: {}", &self.path)?;
        }

        Ok(())
    }
}

impl ResponseError for JsonPayloadError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Overflow { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Payload { source } => source.status_code(),
            Self::Deserialize { source: err } if err.source().is_data() => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            Self::Deserialize { .. } => StatusCode::BAD_REQUEST,
            Self::ContentType => StatusCode::NOT_ACCEPTABLE,
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{http::header, test::TestRequest, web::Bytes};
    use serde::Deserialize;

    use super::*;

    #[derive(Debug, PartialEq, Deserialize)]
    struct MyObject {
        name: String,
    }

    fn json_eq(err: JsonPayloadError, other: JsonPayloadError) -> bool {
        match err {
            JsonPayloadError::Overflow { .. } => {
                matches!(other, JsonPayloadError::Overflow { .. })
            }
            JsonPayloadError::ContentType => matches!(other, JsonPayloadError::ContentType),
            _ => false,
        }
    }

    #[actix_web::test]
    async fn test_extract() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let s = Json::<MyObject, DEFAULT_JSON_LIMIT>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.name, "test");
        assert_eq!(
            s.into_inner(),
            MyObject {
                name: "test".to_string()
            }
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let res = Json::<MyObject, 10>::from_request(&req, &mut pl).await;
        let err = res.unwrap_err();
        assert_eq!(
            "JSON payload (16 bytes) is larger than allowed (limit: 10 bytes)",
            err.to_string(),
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();
        let s = Json::<MyObject, 10>::from_request(&req, &mut pl).await;
        let err = s.unwrap_err();
        assert!(
            err.to_string().contains("larger than allowed"),
            "unexpected error string: {err:?}"
        );
    }

    #[actix_web::test]
    async fn test_json_body() {
        let (req, mut pl) = TestRequest::default().to_http_parts();
        let json = JsonBody::<MyObject, DEFAULT_JSON_LIMIT>::new(&req, &mut pl).await;
        assert!(json_eq(json.unwrap_err(), JsonPayloadError::ContentType));

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/text"),
            ))
            .to_http_parts();
        let json = JsonBody::<MyObject, DEFAULT_JSON_LIMIT>::new(&req, &mut pl).await;
        assert!(json_eq(json.unwrap_err(), JsonPayloadError::ContentType));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("10000"),
            ))
            .to_http_parts();

        let json = JsonBody::<MyObject, 100>::new(&req, &mut pl).await;
        assert!(json_eq(
            json.unwrap_err(),
            JsonPayloadError::Overflow {
                limit: 100,
                length: Some(10000),
            }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(Bytes::from_static(&[0u8; 1000]))
            .to_http_parts();

        let json = JsonBody::<MyObject, 100>::new(&req, &mut pl).await;

        assert!(json_eq(
            json.unwrap_err(),
            JsonPayloadError::Overflow {
                limit: 100,
                length: None
            }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let json = JsonBody::<MyObject, DEFAULT_JSON_LIMIT>::new(&req, &mut pl).await;
        assert_eq!(
            json.ok().unwrap(),
            MyObject {
                name: "test".to_owned()
            }
        );
    }

    #[actix_web::test]
    async fn test_with_json_and_bad_content_type() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/plain"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        Json::<MyObject, 4096>::from_request(&req, &mut pl)
            .await
            .unwrap_err();
    }

    #[actix_web::test]
    async fn test_with_config_in_data_wrapper() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((header::CONTENT_LENGTH, 16))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let res = Json::<MyObject, 10>::from_request(&req, &mut pl).await;
        let err = res.unwrap_err();
        assert_eq!(
            "JSON payload (16 bytes) is larger than allowed (limit: 10 bytes)",
            err.to_string(),
        );
    }

    #[actix_web::test]
    async fn json_deserialize_errors_contain_path() {
        #[derive(Debug, PartialEq, Deserialize)]
        struct Names {
            names: Vec<String>,
        }

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(Bytes::from_static(b"{\"names\": [\"test\", 1]}"))
            .to_http_parts();

        let res = Json::<Names>::from_request(&req, &mut pl).await;
        let err = res.unwrap_err();
        match err {
            JsonPayloadError::Deserialize { source: err } => {
                assert_eq!("names[1]", err.path().to_string());
            }
            err => panic!("unexpected error variant: {err}"),
        }
    }
}
