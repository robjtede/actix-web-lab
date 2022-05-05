//! For path segment extractor documentation, see [`Path`].

use std::{
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::{
    dev::{self, Payload},
    FromRequest, HttpRequest, ResponseError,
};
use derive_more::{AsRef, Display, From};
use futures_core::Stream;

/// Default body size limit of 2MiB.
pub const DEFAULT_BODY_LIMIT: usize = 2_097_152;

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, AsRef, Display, From)]
pub struct BodyLimit<T, const LIMIT: usize = DEFAULT_BODY_LIMIT> {
    inner: T,
}

impl<T, const LIMIT: usize> BodyLimit<T, LIMIT> {
    fn into_inner(self) -> T {
        self.inner
    }
}

impl<T, const LIMIT: usize> FromRequest for BodyLimit<T, LIMIT>
where
    T: FromRequest,
    T::Future: 'static,
    T::Error: 'static,
{
    type Error = BodyLimitError<T::Error>;
    type Future = BodyLimitFut<T, LIMIT>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let counter = crate::util::fork_request_payload(payload);

        BodyLimitFut {
            fut: Box::pin(T::from_request(req, payload)),
            pl: counter,
            size: 0,
        }
    }
}

pub struct BodyLimitFut<T: FromRequest, const LIMIT: usize> {
    /// Wrapped extractor future.
    fut: Pin<Box<T::Future>>,

    /// Forked request payload.
    pl: dev::Payload,

    /// Running payload size count.
    size: usize,
}

impl<T: FromRequest, const LIMIT: usize> Future for BodyLimitFut<T, LIMIT> {
    type Output = Result<BodyLimit<T, LIMIT>, BodyLimitError<T::Error>>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        while let Poll::Ready(Some(Ok(chunk))) = Pin::new(&mut self.pl).poll_next(cx) {
            // update running size
            self.size += chunk.len();

            if self.size > LIMIT {
                return Poll::Ready(Err(BodyLimitError::Overflow));
            }
        }

        match self.fut.as_mut().poll(cx) {
            Poll::Ready(res) => {
                let ret = match res {
                    Ok(item) => Ok(BodyLimit { inner: item }),
                    Err(err) => Err(BodyLimitError::Wrapped(err)),
                };

                while let Poll::Ready(Some(Ok(chunk))) = Pin::new(&mut self.pl).poll_next(cx) {
                    // update running size
                    self.size += chunk.len();

                    if self.size > LIMIT {
                        return Poll::Ready(Err(BodyLimitError::Overflow));
                    }
                }

                Poll::Ready(ret)
            }

            Poll::Pending => Poll::Pending,
        }
    }
}

#[derive(Display)]
#[display(fmt = "todo")]
pub enum BodyLimitError<E> {
    Wrapped(E),
    Overflow,
}

impl<E> fmt::Debug for BodyLimitError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Wrapped(_) => f.debug_tuple("Wrapped").finish(),
            Self::Overflow => write!(f, "Overflow"),
        }
    }
}

impl<E> ResponseError for BodyLimitError<E> {}

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
                header::HeaderValue::from_static("16"),
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
                header::HeaderValue::from_static("16"),
            ))
            .set_payload(Bytes::from_static(b"123456789"))
            .to_http_parts();

        let body = BodyLimit::<Bytes, 4>::from_request(&req, &mut pl).await;
        assert!(matches!(body.unwrap_err(), BodyLimitError::Overflow));
    }
}
