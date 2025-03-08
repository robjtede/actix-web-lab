//! URL-encoded form extractor with const-generic payload size limit.

use std::{
    fmt,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::{
    Error, FromRequest, HttpMessage, HttpRequest, ResponseError,
    dev::Payload,
    error::PayloadError,
    http::{StatusCode, header},
    web,
};
use derive_more::{Display, Error};
use futures_core::Stream as _;
use serde::de::DeserializeOwned;
use tracing::debug;

/// Default URL-encoded form payload size limit of 2MiB.
pub const DEFAULT_URL_ENCODED_FORM_LIMIT: usize = 2_097_152;

/// URL-encoded form extractor with const-generic payload size limit.
///
/// `UrlEncodedForm` is used to extract typed data from URL-encoded request payloads.
///
/// # Extractor
/// To extract typed data from a request body, the inner type `T` must implement the
/// [`serde::Deserialize`] trait.
///
/// Use the `LIMIT` const generic parameter to control the payload size limit. The default limit
/// that is exported (`DEFAULT_LIMIT`) is 2MiB.
///
/// ```
/// use actix_web::{App, post};
/// use actix_web_lab::extract::{DEFAULT_URL_ENCODED_FORM_LIMIT, UrlEncodedForm};
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct Info {
///     username: String,
/// }
///
/// /// Deserialize `Info` from request's body.
/// #[post("/")]
/// async fn index(info: UrlEncodedForm<Info>) -> String {
///     format!("Welcome {}!", info.username)
/// }
///
/// const LIMIT_32_MB: usize = 33_554_432;
///
/// /// Deserialize payload with a higher 32MiB limit.
/// #[post("/big-payload")]
/// async fn big_payload(info: UrlEncodedForm<Info, LIMIT_32_MB>) -> String {
///     format!("Welcome {}!", info.username)
/// }
/// ```
#[doc(alias = "html_form", alias = "html form", alias = "form")]
#[derive(Debug)]
// #[derive(Debug, Deref, DerefMut, Display)]
pub struct UrlEncodedForm<T, const LIMIT: usize = DEFAULT_URL_ENCODED_FORM_LIMIT>(pub T);

mod waiting_on_derive_more_to_start_using_syn_2_due_to_proc_macro_panic {
    use super::*;

    impl<T, const LIMIT: usize> std::ops::Deref for UrlEncodedForm<T, LIMIT> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T, const LIMIT: usize> std::ops::DerefMut for UrlEncodedForm<T, LIMIT> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T: std::fmt::Display, const LIMIT: usize> std::fmt::Display for UrlEncodedForm<T, LIMIT> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.0, f)
        }
    }
}

impl<T, const LIMIT: usize> UrlEncodedForm<T, LIMIT> {
    /// Unwraps into inner `T` value.
    pub fn into_inner(self) -> T {
        self.0
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<T: DeserializeOwned, const LIMIT: usize> FromRequest for UrlEncodedForm<T, LIMIT> {
    type Error = Error;
    type Future = UrlEncodedFormExtractFut<T, LIMIT>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        UrlEncodedFormExtractFut {
            req: Some(req.clone()),
            fut: UrlEncodedFormBody::new(req, payload),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct UrlEncodedFormExtractFut<T, const LIMIT: usize> {
    req: Option<HttpRequest>,
    fut: UrlEncodedFormBody<T, LIMIT>,
}

impl<T: DeserializeOwned, const LIMIT: usize> Future for UrlEncodedFormExtractFut<T, LIMIT> {
    type Output = Result<UrlEncodedForm<T, LIMIT>, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let res = ready!(Pin::new(&mut this.fut).poll(cx));

        let res = match res {
            Err(err) => {
                let req = this.req.take().unwrap();
                debug!(
                    "Failed to deserialize UrlEncodedForm<{}> from payload in handler: {}",
                    core::any::type_name::<T>(),
                    req.match_name().unwrap_or_else(|| req.path())
                );

                Err(err.into())
            }
            Ok(data) => Ok(UrlEncodedForm(data)),
        };

        Poll::Ready(res)
    }
}

/// Future that resolves to some `T` when parsed from a URL-encoded payload.
///
/// Can deserialize any type `T` that implements [`Deserialize`][serde::Deserialize].
///
/// Returns error if:
/// - `Content-Type` is not `application/x-www-form-urlencoded`.
/// - `Content-Length` is greater than `LIMIT`.
/// - The payload, when consumed, is not URL-encoded.
pub enum UrlEncodedFormBody<T, const LIMIT: usize> {
    Error(Option<UrlEncodedFormError>),
    Body {
        /// Length as reported by `Content-Length` header, if present.
        #[allow(dead_code)]
        length: Option<usize>,
        payload: Payload,
        buf: web::BytesMut,
        _res: PhantomData<T>,
    },
}

impl<T, const LIMIT: usize> Unpin for UrlEncodedFormBody<T, LIMIT> {}

impl<T: DeserializeOwned, const LIMIT: usize> UrlEncodedFormBody<T, LIMIT> {
    /// Create a new future to decode a URL-encoded request payload.
    pub fn new(req: &HttpRequest, payload: &mut Payload) -> Self {
        // check content-type
        let can_parse_form = if let Ok(Some(mime)) = req.mime_type() {
            mime == mime::APPLICATION_WWW_FORM_URLENCODED
        } else {
            false
        };

        if !can_parse_form {
            return UrlEncodedFormBody::Error(Some(UrlEncodedFormError::ContentType));
        }

        let length = req
            .headers()
            .get(&header::CONTENT_LENGTH)
            .and_then(|l| l.to_str().ok())
            .and_then(|s| s.parse::<usize>().ok());

        // Notice the content-length is not checked against config limit here.
        // As the internal usage always call UrlEncodedBody::limit after UrlEncodedBody::new.
        // And limit check to return an error variant of UrlEncodedBody happens there.

        let payload = payload.take();

        if let Some(len) = length {
            if len > LIMIT {
                return UrlEncodedFormBody::Error(Some(UrlEncodedFormError::Overflow {
                    size: len,
                    limit: LIMIT,
                }));
            }
        }

        UrlEncodedFormBody::Body {
            length,
            payload,
            buf: web::BytesMut::with_capacity(8192),
            _res: PhantomData,
        }
    }
}

impl<T: DeserializeOwned, const LIMIT: usize> Future for UrlEncodedFormBody<T, LIMIT> {
    type Output = Result<T, UrlEncodedFormError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match this {
            UrlEncodedFormBody::Body { buf, payload, .. } => loop {
                let res = ready!(Pin::new(&mut *payload).poll_next(cx));

                match res {
                    Some(chunk) => {
                        let chunk =
                            chunk.map_err(|err| UrlEncodedFormError::Payload { source: err })?;

                        let buf_len = buf.len() + chunk.len();
                        if buf_len > LIMIT {
                            return Poll::Ready(Err(UrlEncodedFormError::Overflow {
                                size: buf_len,
                                limit: LIMIT,
                            }));
                        } else {
                            buf.extend_from_slice(&chunk);
                        }
                    }

                    None => {
                        let de = serde_html_form::Deserializer::from_bytes(buf);

                        let form = serde_path_to_error::deserialize(de).map_err(|err| {
                            UrlEncodedFormError::Deserialize {
                                source: UrlEncodedFormDeserializeError {
                                    path: err.path().clone(),
                                    source: err.into_inner(),
                                },
                            }
                        })?;

                        return Poll::Ready(Ok(form));
                    }
                }
            },

            UrlEncodedFormBody::Error(err) => Poll::Ready(Err(err.take().unwrap())),
        }
    }
}

/// Errors that can occur while extracting URL-encoded forms.
#[derive(Debug, Display, Error)]
#[non_exhaustive]
pub enum UrlEncodedFormError {
    /// Payload size is larger than allowed.
    #[display(
        "URL encoded payload is larger ({} bytes) than allowed (limit: {} bytes).",
        size,
        limit
    )]
    Overflow { size: usize, limit: usize },

    /// Content type error.
    #[display("Content type error.")]
    ContentType,

    /// Deserialization error.
    #[display("Deserialization error")]
    Deserialize {
        /// Deserialization error.
        source: UrlEncodedFormDeserializeError,
    },

    /// Payload error.
    #[display("Error that occur during reading payload")]
    Payload { source: PayloadError },
}

impl ResponseError for UrlEncodedFormError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::Overflow { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::ContentType => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            Self::Payload { source: err } => err.status_code(),
            Self::Deserialize { .. } => StatusCode::UNPROCESSABLE_ENTITY,
        }
    }
}

/// Errors that can occur while deserializing URL-encoded forms query strings.
#[derive(Debug, Error)]
pub struct UrlEncodedFormDeserializeError {
    /// Path where deserialization error occurred.
    path: serde_path_to_error::Path,

    /// Deserialization error.
    source: serde_html_form::de::Error,
}

impl UrlEncodedFormDeserializeError {
    /// Returns the path at which the deserialization error occurred.
    pub fn path(&self) -> impl fmt::Display + '_ {
        &self.path
    }

    /// Returns the source error.
    pub fn source(&self) -> &serde_html_form::de::Error {
        &self.source
    }
}

impl fmt::Display for UrlEncodedFormDeserializeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("URL-encoded form deserialization failed")?;

        if self.path.iter().len() > 0 {
            write!(f, " at path: {}", &self.path)?;
        }

        Ok(())
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

    fn err_eq(err: UrlEncodedFormError, other: UrlEncodedFormError) -> bool {
        match err {
            UrlEncodedFormError::Overflow { .. } => {
                matches!(other, UrlEncodedFormError::Overflow { .. })
            }

            UrlEncodedFormError::ContentType => matches!(other, UrlEncodedFormError::ContentType),

            _ => false,
        }
    }

    #[actix_web::test]
    async fn test_extract() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::form_url_encoded())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("9"),
            ))
            .set_payload(Bytes::from_static(b"name=test"))
            .to_http_parts();

        let s =
            UrlEncodedForm::<MyObject, DEFAULT_URL_ENCODED_FORM_LIMIT>::from_request(&req, &mut pl)
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
            .insert_header(header::ContentType::form_url_encoded())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("9"),
            ))
            .set_payload(Bytes::from_static(b"name=test"))
            .to_http_parts();

        let s = UrlEncodedForm::<MyObject, 8>::from_request(&req, &mut pl).await;
        let err = format!("{}", s.unwrap_err());
        assert_eq!(
            err,
            "URL encoded payload is larger (9 bytes) than allowed (limit: 8 bytes).",
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::form_url_encoded())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("9"),
            ))
            .set_payload(Bytes::from_static(b"name=test"))
            .to_http_parts();
        let s = UrlEncodedForm::<MyObject, 8>::from_request(&req, &mut pl).await;
        let err = format!("{}", s.unwrap_err());
        assert!(
            err.contains("payload is larger") && err.contains("than allowed"),
            "unexpected error string: {err:?}"
        );
    }

    #[actix_web::test]
    async fn test_form_body() {
        let (req, mut pl) = TestRequest::default().to_http_parts();
        let form =
            UrlEncodedFormBody::<MyObject, DEFAULT_URL_ENCODED_FORM_LIMIT>::new(&req, &mut pl)
                .await;
        assert!(err_eq(form.unwrap_err(), UrlEncodedFormError::ContentType));

        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("application/text"),
            ))
            .to_http_parts();
        let form =
            UrlEncodedFormBody::<MyObject, DEFAULT_URL_ENCODED_FORM_LIMIT>::new(&req, &mut pl)
                .await;
        assert!(err_eq(form.unwrap_err(), UrlEncodedFormError::ContentType));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::form_url_encoded())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("10000"),
            ))
            .to_http_parts();

        let form = UrlEncodedFormBody::<MyObject, 100>::new(&req, &mut pl).await;
        assert!(err_eq(
            form.unwrap_err(),
            UrlEncodedFormError::Overflow {
                size: 10000,
                limit: 100
            }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::form_url_encoded())
            .set_payload(Bytes::from_static(&[0u8; 1000]))
            .to_http_parts();

        let form = UrlEncodedFormBody::<MyObject, 100>::new(&req, &mut pl).await;

        assert!(err_eq(
            form.unwrap_err(),
            UrlEncodedFormError::Overflow {
                size: 1000,
                limit: 100
            }
        ));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::form_url_encoded())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("9"),
            ))
            .set_payload(Bytes::from_static(b"name=test"))
            .to_http_parts();

        let form =
            UrlEncodedFormBody::<MyObject, DEFAULT_URL_ENCODED_FORM_LIMIT>::new(&req, &mut pl)
                .await;
        assert_eq!(
            form.ok().unwrap(),
            MyObject {
                name: "test".to_owned()
            }
        );
    }

    #[actix_web::test]
    async fn test_with_form_and_bad_content_type() {
        let (req, mut pl) = TestRequest::default()
            .insert_header((
                header::CONTENT_TYPE,
                header::HeaderValue::from_static("text/plain"),
            ))
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("9"),
            ))
            .set_payload(Bytes::from_static(b"name=test"))
            .to_http_parts();

        let s = UrlEncodedForm::<MyObject, 4096>::from_request(&req, &mut pl).await;
        assert!(s.is_err())
    }

    #[actix_web::test]
    async fn test_with_config_in_data_wrapper() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::form_url_encoded())
            .insert_header((header::CONTENT_LENGTH, 9))
            .set_payload(Bytes::from_static(b"name=test"))
            .to_http_parts();

        let s = UrlEncodedForm::<MyObject, 8>::from_request(&req, &mut pl).await;
        assert!(s.is_err());

        let err_str = s.unwrap_err().to_string();
        assert_eq!(
            err_str,
            "URL encoded payload is larger (9 bytes) than allowed (limit: 8 bytes).",
        );
    }
}
