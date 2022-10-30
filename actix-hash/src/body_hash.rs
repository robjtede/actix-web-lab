use std::{
    future::Future,
    mem,
    pin::Pin,
    task::{ready, Context, Poll},
};

use actix_web::{dev, FromRequest, HttpRequest};
use actix_web_lab::util::fork_request_payload;
use digest::{generic_array::GenericArray, Digest};
use futures_core::Stream as _;
use pin_project_lite::pin_project;
use tracing::trace;

/// Parts of the resulting body hash extractor.
pub struct BodyHashParts<T> {
    /// Extracted item.
    pub inner: T,

    /// Bytes of the calculated hash.
    pub hash_bytes: Vec<u8>,
}

/// Wraps an extractor and calculates a body checksum hash alongside.
///
/// If your extractor would usually be `T` and you want to create a hash of type `D` then you need
/// to use `BodyHash<T, D>`. E.g., `BodyHash<String, Sha256>`.
///
/// Any hasher that implements [`Digest`] can be used. Type aliases for common hashing algorithms
/// are available at the crate root.
///
/// # Errors
/// This extractor produces no errors of its own and all errors from the underlying extractor are
/// propagated correctly; for example, if the payload limits are exceeded.
///
/// # When Used On The Wrong Extractor
/// Use on a non-body extractor is tolerated unless it is used after a different extractor that
/// _takes_ the payload. In this case, the resulting hash will be as if an empty input was given to
/// the hasher.
///
/// # Example
/// ```
/// use actix_web::{Responder, web};
/// use actix_hash::BodyHash;
/// use sha2::Sha256;
///
/// # type T = u64;
/// async fn hash_payload(form: BodyHash<web::Json<T>, Sha256>) -> impl Responder {
///     if !form.verify_slice(b"correct-signature") {
///         // return unauthorized error
///     }
///
///     "Ok"
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BodyHash<T, D: Digest> {
    inner: T,
    hash: GenericArray<u8, D::OutputSize>,
}

impl<T, D: Digest> BodyHash<T, D> {
    /// Returns hash slice.
    pub fn hash(&self) -> &[u8] {
        self.hash.as_slice()
    }

    /// Returns hash output size.
    pub fn hash_size(&self) -> usize {
        self.hash.len()
    }

    /// Verifies HMAC hash against provided `tag` using constant-time equality.
    pub fn verify_slice(&self, tag: &[u8]) -> bool {
        use subtle::ConstantTimeEq as _;
        self.hash.ct_eq(tag).into()
    }

    /// Returns body type parts, including extracted body type, raw body bytes, and hash bytes.
    pub fn into_parts(self) -> BodyHashParts<T> {
        let hash = self.hash().to_vec();

        BodyHashParts {
            inner: self.inner,
            hash_bytes: hash,
        }
    }
}

impl<T, D> FromRequest for BodyHash<T, D>
where
    T: FromRequest + 'static,
    D: Digest + 'static,
{
    type Error = T::Error;
    type Future = BodyHashFut<T, D>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        if matches!(payload, dev::Payload::None) {
            trace!("inner request payload is none");
            BodyHashFut::PayloadNone {
                inner_fut: T::from_request(req, payload),
                hash: D::new().finalize(),
            }
        } else {
            trace!("forking request payload");
            let forked_payload = fork_request_payload(payload);

            let inner_fut = T::from_request(req, payload);
            let hasher = D::new();

            BodyHashFut::Inner {
                inner_fut,
                hasher,
                forked_payload,
            }
        }
    }
}

pin_project! {
    #[project = BodyHashFutProj]
    pub enum BodyHashFut<T: FromRequest, D: Digest> {
        PayloadNone {
            #[pin]
            inner_fut: T::Future,
            hash: GenericArray<u8, D::OutputSize>,
        },

        Inner {
            #[pin]
            inner_fut: T::Future,
            hasher: D,
            forked_payload: dev::Payload,
        },

        InnerDone {
            inner: Option<T>,
            hasher: D,
            forked_payload: dev::Payload,
        }
    }
}

impl<T: FromRequest, D: Digest> Future for BodyHashFut<T, D> {
    type Output = Result<BodyHash<T, D>, T::Error>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.as_mut().project() {
            BodyHashFutProj::PayloadNone { inner_fut, hash } => {
                let inner = ready!(inner_fut.poll(cx))?;
                Poll::Ready(Ok(BodyHash {
                    inner,
                    hash: mem::take(hash),
                }))
            }

            BodyHashFutProj::Inner {
                inner_fut,
                hasher,
                mut forked_payload,
            } => {
                // poll original extractor
                match inner_fut.poll(cx)? {
                    Poll::Ready(inner) => {
                        trace!("inner extractor complete");

                        let next = BodyHashFut::InnerDone {
                            inner: Some(inner),
                            hasher: mem::replace(hasher, D::new()),
                            forked_payload: mem::replace(forked_payload, dev::Payload::None),
                        };
                        self.set(next);

                        // re-enter poll in done state
                        self.poll(cx)
                    }
                    Poll::Pending => {
                        // drain forked payload
                        loop {
                            match Pin::new(&mut forked_payload).poll_next(cx) {
                                // update hasher with chunks
                                Poll::Ready(Some(Ok(chunk))) => hasher.update(&chunk),

                                Poll::Ready(None) => {
                                    unreachable!(
                                        "not possible to poll end of payload before inner stream \
                                        completes"
                                    )
                                }

                                // Ignore Pending because its possible the inner extractor never
                                // polls the payload stream and ignore errors because they will be
                                // propagated by original payload polls.
                                Poll::Ready(Some(Err(_))) | Poll::Pending => break,
                            }
                        }

                        Poll::Pending
                    }
                }
            }

            BodyHashFutProj::InnerDone {
                inner,
                hasher,
                forked_payload,
            } => {
                let mut pl = Pin::new(forked_payload);

                // drain forked payload
                loop {
                    match pl.as_mut().poll_next(cx) {
                        // update hasher with chunks
                        Poll::Ready(Some(Ok(chunk))) => hasher.update(&chunk),

                        // when drain is complete, finalize hash and return parts
                        Poll::Ready(None) => {
                            trace!("payload hashing complete");

                            let hasher = mem::replace(hasher, D::new());
                            let hash = hasher.finalize();

                            return Poll::Ready(Ok(BodyHash {
                                inner: inner.take().unwrap(),
                                hash,
                            }));
                        }

                        // Ignore Pending because its possible the inner extractor never polls the
                        // payload stream and ignore errors because they will be propagated by
                        // original payload polls
                        Poll::Ready(Some(Err(_))) | Poll::Pending => return Poll::Pending,
                    }
                }
            }
        }
    }
}
