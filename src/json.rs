//! JSON extractor with const-generic payload size limit.

use std::{
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use derive_more::{Deref, DerefMut, Display};
use futures_core::{ready, Stream as _};
use serde::de::DeserializeOwned;

// #[cfg(feature = "__compress")]
// use crate::dev::Decompress;
use actix_web::{
    dev::Payload, error::JsonPayloadError, http::header, web, Error, FromRequest, HttpMessage,
    HttpRequest,
};
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
/// When const generics defaults are supported (due for Rust 1.59) it will be possible to omit the
/// default limit.
///
/// ```
/// use actix_web::{post, App};
/// use actix_web_lab::extract::{DEFAULT_JSON_LIMIT, Json};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
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
/// ```
#[derive(Debug, Deref, DerefMut, Display)]
pub struct Json<T, const LIMIT: usize = DEFAULT_JSON_LIMIT>(pub T);

impl<T, const LIMIT: usize> Json<T, LIMIT> {
    /// Unwrap into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<T: DeserializeOwned, const LIMIT: usize> FromRequest for Json<T, LIMIT> {
    type Error = Error;
    type Future = JsonExtractFut<T, LIMIT>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        JsonExtractFut {
            req: Some(req.clone()),
            fut: JsonBody::new(req, payload),
        }
    }
}

pub struct JsonExtractFut<T, const LIMIT: usize> {
    req: Option<HttpRequest>,
    fut: JsonBody<T, LIMIT>,
}

impl<T: DeserializeOwned, const LIMIT: usize> Future for JsonExtractFut<T, LIMIT> {
    type Output = Result<Json<T, LIMIT>, Error>;

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

                Err(err.into())
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
    // #[allow(clippy::borrow_interior_mutable_const)]
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

        // Notice the content-length is not checked against limit of json config here.
        // As the internal usage always call JsonBody::limit after JsonBody::new.
        // And limit check to return an error variant of JsonBody happens there.

        let payload = {
            // cfg_if::cfg_if! {
            //     if #[cfg(feature = "__compress")] {
            //         Decompress::from_headers(payload.take(), req.headers())
            //     } else {
            payload.take()
            //     }
            // }
        };

        if let Some(len) = length {
            if len > LIMIT {
                return JsonBody::Error(Some(JsonPayloadError::OverflowKnownLength {
                    length: len,
                    limit: LIMIT,
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
                        let chunk = chunk?;
                        let buf_len = buf.len() + chunk.len();
                        if buf_len > LIMIT {
                            return Poll::Ready(Err(JsonPayloadError::Overflow { limit: LIMIT }));
                        } else {
                            buf.extend_from_slice(&chunk);
                        }
                    }

                    None => {
                        let json = serde_json::from_slice::<T>(buf)
                            .map_err(JsonPayloadError::Deserialize)?;
                        return Poll::Ready(Ok(json));
                    }
                }
            },

            JsonBody::Error(e) => Poll::Ready(Err(e.take().unwrap())),
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{http::header, test::TestRequest, web::Bytes};
    use serde::{Deserialize, Serialize};

    use super::*;

    #[derive(Serialize, Deserialize, PartialEq, Debug)]
    struct MyObject {
        name: String,
    }

    fn json_eq(err: JsonPayloadError, other: JsonPayloadError) -> bool {
        match err {
            JsonPayloadError::Overflow { .. } => {
                matches!(other, JsonPayloadError::Overflow { .. })
            }
            JsonPayloadError::OverflowKnownLength { .. } => {
                matches!(other, JsonPayloadError::OverflowKnownLength { .. })
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

        let s = Json::<MyObject, 10>::from_request(&req, &mut pl).await;
        let err = format!("{}", s.unwrap_err());
        assert!(
            err.contains("JSON payload (16 bytes) is larger than allowed (limit: 10 bytes)."),
            "unexpected error string: {:?}",
            err
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
        let err = format!("{}", s.unwrap_err());
        assert!(
            err.contains("larger than allowed"),
            "unexpected error string: {:?}",
            err
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
            JsonPayloadError::OverflowKnownLength {
                length: 10000,
                limit: 100
            }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(Bytes::from_static(&[0u8; 1000]))
            .to_http_parts();

        let json = JsonBody::<MyObject, 100>::new(&req, &mut pl).await;

        assert!(json_eq(
            json.unwrap_err(),
            JsonPayloadError::Overflow { limit: 100 }
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

        let s = Json::<MyObject, 4096>::from_request(&req, &mut pl).await;
        assert!(s.is_err())
    }

    #[actix_web::test]
    async fn test_with_config_in_data_wrapper() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header((header::CONTENT_LENGTH, 16))
            .set_payload(Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let s = Json::<MyObject, 10>::from_request(&req, &mut pl).await;
        assert!(s.is_err());

        let err_str = s.unwrap_err().to_string();
        assert!(
            err_str.contains("JSON payload (16 bytes) is larger than allowed (limit: 10 bytes).")
        );
    }
}
