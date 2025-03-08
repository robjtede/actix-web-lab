//! Bytes extractor with const-generic payload size limit.
//!
//! See docs for [`Bytes`].

use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::{FromRequest, HttpMessage, HttpRequest, ResponseError, dev, http::StatusCode, web};
use derive_more::{Display, Error};
use futures_core::Stream as _;
use tracing::debug;

/// Default bytes payload size limit of 4MiB.
pub const DEFAULT_BYTES_LIMIT: usize = 4_194_304;

/// Bytes extractor with const-generic payload size limit.
///
/// # Extractor
/// Extracts raw bytes from a request body, even if it.
///
/// Use the `LIMIT` const generic parameter to control the payload size limit. The default limit
/// that is exported (`DEFAULT_LIMIT`) is 4MiB.
///
/// # Differences from `actix_web::web::Bytes`
/// - Does not read `PayloadConfig` from app data.
/// - Supports const-generic size limits.
/// - Will not automatically decompress request bodies.
///
/// # Examples
/// ```
/// use actix_web::{App, post};
/// use actix_web_lab::extract::{Bytes, DEFAULT_BYTES_LIMIT};
///
/// /// Deserialize `Info` from request's body.
/// #[post("/")]
/// async fn index(info: Bytes) -> String {
///     format!("Payload up to 4MiB: {info:?}!")
/// }
///
/// const LIMIT_32_MB: usize = 33_554_432;
///
/// /// Deserialize payload with a higher 32MiB limit.
/// #[post("/big-payload")]
/// async fn big_payload(info: Bytes<LIMIT_32_MB>) -> String {
///     format!("Payload up to 32MiB: {info:?}!")
/// }
/// ```
#[derive(Debug)]
// #[derive(Debug, Deref, DerefMut, AsRef, AsMut)]
pub struct Bytes<const LIMIT: usize = DEFAULT_BYTES_LIMIT>(pub web::Bytes);

mod waiting_on_derive_more_to_start_using_syn_2_due_to_proc_macro_panic {
    use super::*;

    impl<const LIMIT: usize> std::ops::Deref for Bytes<LIMIT> {
        type Target = web::Bytes;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<const LIMIT: usize> std::ops::DerefMut for Bytes<LIMIT> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<const LIMIT: usize> AsRef<web::Bytes> for Bytes<LIMIT> {
        fn as_ref(&self) -> &web::Bytes {
            &self.0
        }
    }

    impl<const LIMIT: usize> AsMut<web::Bytes> for Bytes<LIMIT> {
        fn as_mut(&mut self) -> &mut web::Bytes {
            &mut self.0
        }
    }
}

impl<const LIMIT: usize> Bytes<LIMIT> {
    /// Unwraps into inner `Bytes`.
    pub fn into_inner(self) -> web::Bytes {
        self.0
    }
}

/// See [here](#extractor) for example of usage as an extractor.
impl<const LIMIT: usize> FromRequest for Bytes<LIMIT> {
    type Error = actix_web::Error;
    type Future = BytesExtractFut<LIMIT>;

    #[inline]
    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        BytesExtractFut {
            req: Some(req.clone()),
            fut: BytesBody::new(req, payload),
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct BytesExtractFut<const LIMIT: usize> {
    req: Option<HttpRequest>,
    fut: BytesBody<LIMIT>,
}

impl<const LIMIT: usize> Future for BytesExtractFut<LIMIT> {
    type Output = actix_web::Result<Bytes<LIMIT>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let res = ready!(Pin::new(&mut this.fut).poll(cx));

        let res = match res {
            Err(err) => {
                let req = this.req.take().unwrap();

                debug!(
                    "Failed to extract Bytes from payload in handler: {}",
                    req.match_name().unwrap_or_else(|| req.path())
                );

                Err(err.into())
            }
            Ok(data) => Ok(Bytes(data)),
        };

        Poll::Ready(res)
    }
}

/// Future that resolves to `Bytes` when the payload is been completely read.
///
/// Returns error if:
/// - `Content-Length` is greater than `LIMIT`.
pub enum BytesBody<const LIMIT: usize> {
    Error(Option<BytesPayloadError>),
    Body {
        /// Length as reported by `Content-Length` header, if present.
        #[allow(dead_code)]
        length: Option<usize>,
        payload: dev::Payload,
        buf: web::BytesMut,
    },
}

impl<const LIMIT: usize> Unpin for BytesBody<LIMIT> {}

impl<const LIMIT: usize> BytesBody<LIMIT> {
    /// Create a new future to decode a JSON request payload.
    pub fn new(req: &HttpRequest, payload: &mut dev::Payload) -> Self {
        let payload = payload.take();

        let length = req
            .get_header::<crate::header::ContentLength>()
            .map(|cl| cl.into_inner());

        // Notice the content-length is not checked against limit here as the internal usage always
        // call BytesBody::limit after BytesBody::new and limit check to return an error variant of
        // BytesBody happens there.

        if let Some(len) = length {
            if len > LIMIT {
                return BytesBody::Error(Some(BytesPayloadError::OverflowKnownLength {
                    length: len,
                    limit: LIMIT,
                }));
            }
        }

        BytesBody::Body {
            length,
            payload,
            buf: web::BytesMut::with_capacity(8192),
        }
    }
}

impl<const LIMIT: usize> Future for BytesBody<LIMIT> {
    type Output = Result<web::Bytes, BytesPayloadError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        match this {
            BytesBody::Body { buf, payload, .. } => loop {
                let res = ready!(Pin::new(&mut *payload).poll_next(cx));

                match res {
                    Some(chunk) => {
                        let chunk = chunk?;
                        let buf_len = buf.len() + chunk.len();
                        if buf_len > LIMIT {
                            return Poll::Ready(Err(BytesPayloadError::Overflow { limit: LIMIT }));
                        } else {
                            buf.extend_from_slice(&chunk);
                        }
                    }

                    None => return Poll::Ready(Ok(buf.split().freeze())),
                }
            },

            BytesBody::Error(err) => Poll::Ready(Err(err.take().unwrap())),
        }
    }
}

/// A set of errors that can occur during parsing json payloads
#[derive(Debug, Display, Error)]
#[non_exhaustive]
pub enum BytesPayloadError {
    /// Payload size is bigger than allowed & content length header set. (default: 4MiB)
    #[display("Payload ({length} bytes) is larger than allowed (limit: {limit} bytes).")]
    OverflowKnownLength { length: usize, limit: usize },

    /// Payload size is bigger than allowed but no content length header set. (default: 4MiB)
    #[display("Payload has exceeded limit ({limit} bytes).")]
    Overflow { limit: usize },

    /// Payload error.
    #[display("Error that occur during reading payload: {_0}")]
    Payload(actix_web::error::PayloadError),
}

impl From<actix_web::error::PayloadError> for BytesPayloadError {
    fn from(err: actix_web::error::PayloadError) -> Self {
        Self::Payload(err)
    }
}

impl ResponseError for BytesPayloadError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::OverflowKnownLength { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Overflow { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            Self::Payload(err) => err.status_code(),
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{http::header, test::TestRequest, web};

    use super::*;

    #[cfg(test)]
    impl PartialEq for BytesPayloadError {
        fn eq(&self, other: &Self) -> bool {
            match (self, other) {
                (
                    Self::OverflowKnownLength {
                        length: l_length,
                        limit: l_limit,
                    },
                    Self::OverflowKnownLength {
                        length: r_length,
                        limit: r_limit,
                    },
                ) => l_length == r_length && l_limit == r_limit,

                (Self::Overflow { limit: l_limit }, Self::Overflow { limit: r_limit }) => {
                    l_limit == r_limit
                }

                _ => false,
            }
        }
    }

    #[actix_web::test]
    async fn extract() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header(crate::header::ContentLength::from(3))
            .set_payload(web::Bytes::from_static(b"foo"))
            .to_http_parts();

        let s = Bytes::<DEFAULT_BYTES_LIMIT>::from_request(&req, &mut pl)
            .await
            .unwrap();
        assert_eq!(s.as_ref(), "foo");

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header(crate::header::ContentLength::from(16))
            .set_payload(web::Bytes::from_static(b"foo foo foo foo"))
            .to_http_parts();

        let s = Bytes::<10>::from_request(&req, &mut pl).await;
        let err_str = s.unwrap_err().to_string();
        assert_eq!(
            err_str,
            "Payload (16 bytes) is larger than allowed (limit: 10 bytes).",
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header(crate::header::ContentLength::from(16))
            .set_payload(web::Bytes::from_static(b"foo foo foo foo"))
            .to_http_parts();
        let s = Bytes::<10>::from_request(&req, &mut pl).await;
        let err = format!("{}", s.unwrap_err());
        assert!(
            err.contains("larger than allowed"),
            "unexpected error string: {err:?}",
        );
    }

    #[actix_web::test]
    async fn body() {
        let (req, mut pl) = TestRequest::default().to_http_parts();
        let _bytes = BytesBody::<DEFAULT_BYTES_LIMIT>::new(&req, &mut pl)
            .await
            .unwrap();

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType("application/text".parse().unwrap()))
            .to_http_parts();
        // content-type doesn't matter
        BytesBody::<DEFAULT_BYTES_LIMIT>::new(&req, &mut pl)
            .await
            .unwrap();

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header(crate::header::ContentLength::from(10000))
            .to_http_parts();

        let bytes = BytesBody::<100>::new(&req, &mut pl).await;
        assert_eq!(
            bytes.unwrap_err(),
            BytesPayloadError::OverflowKnownLength {
                length: 10000,
                limit: 100
            }
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .set_payload(web::Bytes::from_static(&[0u8; 1000]))
            .to_http_parts();

        let bytes = BytesBody::<100>::new(&req, &mut pl).await;

        assert_eq!(
            bytes.unwrap_err(),
            BytesPayloadError::Overflow { limit: 100 }
        );

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::json())
            .insert_header(crate::header::ContentLength::from(16))
            .set_payload(web::Bytes::from_static(b"foo foo foo foo"))
            .to_http_parts();

        let bytes = BytesBody::<DEFAULT_BYTES_LIMIT>::new(&req, &mut pl).await;
        assert_eq!(bytes.ok().unwrap(), "foo foo foo foo");
    }

    #[actix_web::test]
    async fn test_with_config_in_data_wrapper() {
        let (req, mut pl) = TestRequest::default()
            .app_data(web::Data::new(web::PayloadConfig::default().limit(8)))
            .insert_header(header::ContentType::json())
            .insert_header((header::CONTENT_LENGTH, 16))
            .set_payload(web::Bytes::from_static(b"{\"name\": \"test\"}"))
            .to_http_parts();

        let s = Bytes::<10>::from_request(&req, &mut pl).await;
        assert!(s.is_err());

        let err_str = s.unwrap_err().to_string();
        assert_eq!(
            err_str,
            "Payload (16 bytes) is larger than allowed (limit: 10 bytes).",
        );
    }
}
