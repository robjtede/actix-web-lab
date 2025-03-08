//! Body limit extractor.
//!
//! See [`BodyLimit`] docs.

use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::{
    FromRequest, HttpMessage as _, HttpRequest, ResponseError,
    dev::{self, Payload},
};
use derive_more::Display;
use futures_core::Stream as _;

use crate::header::ContentLength;

/// Default body size limit of 2MiB.
pub const DEFAULT_BODY_LIMIT: usize = 2_097_152;

/// Extractor wrapper that limits size of payload used.
///
/// # Examples
/// ```no_run
/// use actix_web::{Responder, get, web::Bytes};
/// use actix_web_lab::extract::BodyLimit;
///
/// const BODY_LIMIT: usize = 1_048_576; // 1MB
///
/// #[get("/")]
/// async fn handler(body: BodyLimit<Bytes, BODY_LIMIT>) -> impl Responder {
///     let body = body.into_inner();
///     assert!(body.len() < BODY_LIMIT);
///     body
/// }
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct BodyLimit<T, const LIMIT: usize = DEFAULT_BODY_LIMIT> {
    inner: T,
}

mod waiting_on_derive_more_to_start_using_syn_2_due_to_proc_macro_panic {
    use super::*;

    impl<T: std::fmt::Display, const LIMIT: usize> std::fmt::Display for BodyLimit<T, LIMIT> {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            std::fmt::Display::fmt(&self.inner, f)
        }
    }

    impl<T, const LIMIT: usize> AsRef<T> for BodyLimit<T, LIMIT> {
        fn as_ref(&self) -> &T {
            &self.inner
        }
    }

    impl<T, const LIMIT: usize> From<T> for BodyLimit<T, LIMIT> {
        fn from(inner: T) -> Self {
            Self { inner }
        }
    }
}

impl<T, const LIMIT: usize> BodyLimit<T, LIMIT> {
    /// Returns inner extracted type.
    pub fn into_inner(self) -> T {
        self.inner
    }
}

impl<T, const LIMIT: usize> FromRequest for BodyLimit<T, LIMIT>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
    type Error = BodyLimitError<T>;
    type Future = BodyLimitFut<T, LIMIT>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        // fast check of Content-Length header
        match req.get_header::<ContentLength>() {
            // CL header indicated that payload would be too large
            Some(len) if len > LIMIT => return BodyLimitFut::new_error(BodyLimitError::Overflow),
            _ => {}
        }

        let counter = crate::util::fork_request_payload(payload);

        BodyLimitFut {
            inner: Inner::Body {
                fut: Box::pin(T::from_request(req, payload)),
                counter_pl: counter,
                size: 0,
            },
        }
    }
}

#[allow(missing_debug_implementations)]
pub struct BodyLimitFut<T, const LIMIT: usize>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
    inner: Inner<T, LIMIT>,
}

impl<T, const LIMIT: usize> BodyLimitFut<T, LIMIT>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
    fn new_error(err: BodyLimitError<T>) -> Self {
        Self {
            inner: Inner::Error { err: Some(err) },
        }
    }
}

enum Inner<T, const LIMIT: usize>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
    Error {
        err: Option<BodyLimitError<T>>,
    },

    Body {
        /// Wrapped extractor future.
        fut: Pin<Box<T::Future>>,

        /// Forked request payload.
        counter_pl: dev::Payload,

        /// Running payload size count.
        size: usize,
    },
}

impl<T, const LIMIT: usize> Unpin for Inner<T, LIMIT>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
}

impl<T, const LIMIT: usize> Future for BodyLimitFut<T, LIMIT>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
    type Output = Result<BodyLimit<T, LIMIT>, BodyLimitError<T>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = &mut self.get_mut().inner;

        match this {
            Inner::Error { err } => Poll::Ready(Err(err.take().unwrap())),

            Inner::Body {
                fut,
                counter_pl,
                size,
            } => {
                // poll inner extractor first which also polls original payload stream
                let res = ready!(fut.as_mut().poll(cx).map_err(BodyLimitError::Extractor)?);

                // catch up with payload length counter checks
                while let Poll::Ready(Some(Ok(chunk))) = Pin::new(&mut *counter_pl).poll_next(cx) {
                    // update running size
                    *size += chunk.len();

                    if *size > LIMIT {
                        return Poll::Ready(Err(BodyLimitError::Overflow));
                    }
                }

                let ret = BodyLimit { inner: res };

                Poll::Ready(Ok(ret))
            }
        }
    }
}

#[derive(Display)]
pub enum BodyLimitError<T>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
    #[display("Wrapped extractor error: {_0}")]
    Extractor(T::Error),

    #[display("Body was too large")]
    Overflow,
}

impl<T> fmt::Debug for BodyLimitError<T>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extractor(err) => f
                .debug_tuple("BodyLimitError::Extractor")
                .field(err)
                .finish(),

            Self::Overflow => write!(f, "BodyLimitError::Overflow"),
        }
    }
}

impl<T> ResponseError for BodyLimitError<T>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
{
}

#[cfg(test)]
mod tests {
    use actix_web::{http::header, test::TestRequest};
    use bytes::Bytes;

    use super::*;

    static_assertions::assert_impl_all!(BodyLimitFut<(), 100>: Unpin);
    static_assertions::assert_impl_all!(BodyLimitFut<Bytes, 100>: Unpin);

    #[actix_web::test]
    async fn within_limit() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::plaintext())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("9"),
            ))
            .set_payload(Bytes::from_static(b"123456789"))
            .to_http_parts();

        let body = BodyLimit::<Bytes, 10>::from_request(&req, &mut pl).await;
        assert_eq!(
            body.ok().unwrap().into_inner(),
            Bytes::from_static(b"123456789")
        );
    }

    #[actix_web::test]
    async fn exceeds_limit() {
        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::plaintext())
            .insert_header((
                header::CONTENT_LENGTH,
                header::HeaderValue::from_static("10"),
            ))
            .set_payload(Bytes::from_static(b"0123456789"))
            .to_http_parts();

        let body = BodyLimit::<Bytes, 4>::from_request(&req, &mut pl).await;
        assert!(matches!(body.unwrap_err(), BodyLimitError::Overflow));

        let (req, mut pl) = TestRequest::default()
            .insert_header(header::ContentType::plaintext())
            .insert_header((
                header::TRANSFER_ENCODING,
                header::HeaderValue::from_static("chunked"),
            ))
            .set_payload(Bytes::from_static(b"10\r\n0123456789\r\n0"))
            .to_http_parts();

        let body = BodyLimit::<Bytes, 4>::from_request(&req, &mut pl).await;
        assert!(matches!(body.unwrap_err(), BodyLimitError::Overflow));
    }
}
