use std::fmt;

use actix_http::BoxedPayloadStream;
use actix_web::{Error, FromRequest, HttpRequest, dev, web::Bytes};
use derive_more::Display;
use futures_core::future::LocalBoxFuture;
use futures_util::{FutureExt as _, StreamExt as _, TryFutureExt as _};
use local_channel::mpsc;
use tokio::try_join;
use tracing::trace;

/// Define a scheme for deriving and verifying some kind of signature from request parts.
///
/// There are 4 phases to calculating a signature while a request is being received:
/// 1. [Initialize](Self::init): Construct the signature scheme type and perform any pre-body
///    calculation steps with request head parts.
/// 1. [Consume body](Self::consume_chunk): For each body chunk received, fold it to the signature
///    calculation.
/// 1. [Finalize](Self::finalize): Perform post-body calculation steps and finalize signature type.
/// 1. [Verify](Self::verify): Check the _true signature_ against a _candidate signature_; for
///    example, a header added by the client. This phase is optional.
///
/// # Bring Your Own Crypto
///
/// It is up to the implementor to ensure that best security practices are being followed when
/// implementing this trait, and in particular the `verify` method. There is no inherent preference
/// for certain crypto ecosystems though many of the examples shown here will use types from
/// [RustCrypto](https://github.com/RustCrypto).
///
/// # `RequestSignature` Extractor
///
/// Types that implement this trait can be used with the [`RequestSignature`] extractor to
/// declaratively derive the request signature alongside the desired body extractor.
///
/// # Examples
///
/// This trait can be used to define:
/// - API authentication schemes that requires a signature to be attached to the request, either
///   with static keys or dynamic, per-user keys that are looked asynchronously from a database.
/// - Request hashes derived from specific parts for cache lookups.
///
/// This example implementation does a simple HMAC calculation on the body using a static key.
/// It does not implement verification.
/// ```
/// use actix_web::{Error, HttpRequest, web::Bytes};
/// use actix_web_lab::extract::RequestSignatureScheme;
/// use hmac::{Mac, SimpleHmac, digest::CtOutput};
/// use sha2::Sha256;
///
/// struct AbcApi {
///     /// Running state.
///     hmac: SimpleHmac<Sha256>,
/// }
///
/// impl RequestSignatureScheme for AbcApi {
///     /// The constant-time verifiable output of the HMAC type.
///     type Signature = CtOutput<SimpleHmac<Sha256>>;
///     type Error = Error;
///
///     async fn init(req: &HttpRequest) -> Result<Self, Self::Error> {
///         // acquire HMAC signing key
///         let key = req.app_data::<[u8; 32]>().unwrap();
///
///         // construct HMAC signer
///         let hmac = SimpleHmac::new_from_slice(&key[..]).unwrap();
///         Ok(AbcApi { hmac })
///     }
///
///     async fn consume_chunk(
///         &mut self,
///         _req: &HttpRequest,
///         chunk: Bytes,
///     ) -> Result<(), Self::Error> {
///         // digest body chunk
///         self.hmac.update(&chunk);
///         Ok(())
///     }
///
///     async fn finalize(self, _req: &HttpRequest) -> Result<Self::Signature, Self::Error> {
///         // construct signature type
///         Ok(self.hmac.finalize())
///     }
/// }
/// ```
pub trait RequestSignatureScheme: Sized {
    /// The signature type returned from [`finalize`](Self::finalize) and checked in
    /// [`verify`](Self::verify).
    ///
    /// Ideally, this type has constant-time equality capabilities.
    type Signature;

    /// Error type used by all trait methods to signal missing precondition, processing errors, or
    /// verification failures.
    ///
    /// Must be convertible to an error response; i.e., implements [`ResponseError`].
    ///
    /// [`ResponseError`]: https://docs.rs/actix-web/4/actix_web/trait.ResponseError.html
    type Error: Into<Error>;

    /// Initialize signature scheme for incoming request.
    ///
    /// Possible steps that should be included in `init` implementations:
    /// - initialization of signature scheme type
    /// - key lookup / initialization
    /// - pre-body digest updates
    fn init(req: &HttpRequest) -> impl Future<Output = Result<Self, Self::Error>>;

    /// Fold received body chunk into signature.
    ///
    /// If processing the request body one chunk at a time is not equivalent to processing it all at
    /// once, then the chunks will need to be added to a buffer.
    fn consume_chunk(
        &mut self,
        req: &HttpRequest,
        chunk: Bytes,
    ) -> impl Future<Output = Result<(), Self::Error>>;

    /// Finalize and output `Signature` type.
    ///
    /// Possible steps that should be included in `finalize` implementations:
    /// - post-body digest updates
    /// - signature finalization
    fn finalize(
        self,
        req: &HttpRequest,
    ) -> impl Future<Output = Result<Self::Signature, Self::Error>>;

    /// Verify _true signature_ against _candidate signature_.
    ///
    /// The _true signature_ is what has been calculated during request processing by the other
    /// methods in this trait. The _candidate signature_ might be a signature provided by the client
    /// in order to prove ownership of a key or some other known signature to validate against.
    ///
    /// Implementations should return `signature` if it is valid and return an error if it is not.
    /// The default implementation does no checks and just returns `signature` as-is.
    ///
    /// # Security
    /// To avoid timing attacks, equality checks should be constant-time; check the docs of your
    /// chosen crypto library.
    #[allow(unused_variables)]
    #[inline]
    fn verify(
        signature: Self::Signature,
        req: &HttpRequest,
    ) -> Result<Self::Signature, Self::Error> {
        Ok(signature)
    }
}

/// Wraps an extractor and calculates a request signature hash alongside.
///
/// Warning: Currently, this will always take the body meaning that if a body extractor is used,
/// this needs to wrap it or else it will not work.
#[allow(missing_debug_implementations)]
#[derive(Clone)]
pub struct RequestSignature<T, S: RequestSignatureScheme> {
    extractor: T,
    signature: S::Signature,
}

impl<T, S: RequestSignatureScheme> RequestSignature<T, S> {
    /// Returns tuple containing body type, and owned hash.
    pub fn into_parts(self) -> (T, S::Signature) {
        (self.extractor, self.signature)
    }
}

/// Errors that can occur when extracting and processing request signatures.
#[derive(Display)]
#[non_exhaustive]
pub enum RequestSignatureError<T, S>
where
    T: FromRequest,
    T::Error: fmt::Debug + fmt::Display,
    S: RequestSignatureScheme,
    S::Error: fmt::Debug + fmt::Display,
{
    /// Inner extractor error.
    #[display("Inner extractor error: {_0}")]
    Extractor(T::Error),

    /// Signature calculation error.
    #[display("Signature calculation error: {_0}")]
    Signature(S::Error),
}

impl<T, S> fmt::Debug for RequestSignatureError<T, S>
where
    T: FromRequest,
    T::Error: fmt::Debug + fmt::Display,
    S: RequestSignatureScheme,
    S::Error: fmt::Debug + fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Extractor(err) => f
                .debug_tuple("RequestSignatureError::Extractor")
                .field(err)
                .finish(),

            Self::Signature(err) => f
                .debug_tuple("RequestSignatureError::Signature")
                .field(err)
                .finish(),
        }
    }
}

impl<T, S> From<RequestSignatureError<T, S>> for actix_web::Error
where
    T: FromRequest,
    T::Error: fmt::Debug + fmt::Display,
    S: RequestSignatureScheme,
    S::Error: fmt::Debug + fmt::Display,
{
    fn from(err: RequestSignatureError<T, S>) -> Self {
        match err {
            RequestSignatureError::Extractor(err) => err.into(),
            RequestSignatureError::Signature(err) => err.into(),
        }
    }
}

impl<T, S> FromRequest for RequestSignature<T, S>
where
    T: FromRequest + 'static,
    T::Error: fmt::Debug + fmt::Display,
    S: RequestSignatureScheme + 'static,
    S::Error: fmt::Debug + fmt::Display,
{
    type Error = RequestSignatureError<T, S>;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        let req = req.clone();
        let payload = payload.take();

        Box::pin(async move {
            let (tx, mut rx) = mpsc::channel();

            // wrap payload in stream that reads chunks and clones them (cheaply) back here
            let proxy_stream: BoxedPayloadStream = Box::pin(payload.inspect(move |res| {
                if let Ok(chunk) = res {
                    trace!("yielding {} byte chunk", chunk.len());
                    tx.send(chunk.clone()).unwrap();
                }
            }));

            trace!("creating proxy payload");
            let mut proxy_payload = dev::Payload::from(proxy_stream);
            let body_fut =
                T::from_request(&req, &mut proxy_payload).map_err(RequestSignatureError::Extractor);

            trace!("initializing signature scheme");
            let mut sig_scheme = S::init(&req)
                .await
                .map_err(RequestSignatureError::Signature)?;

            // run update function as chunks are yielded from channel
            let hash_fut = actix_web::rt::spawn({
                let req = req.clone();

                async move {
                    while let Some(chunk) = rx.recv().await {
                        trace!("digesting chunk");
                        sig_scheme.consume_chunk(&req, chunk).await?;
                    }

                    trace!("finalizing signature");
                    sig_scheme.finalize(&req).await
                }
            })
            .map(Result::unwrap)
            .map_err(RequestSignatureError::Signature);

            trace!("driving both futures");
            let (body, signature) = try_join!(body_fut, hash_fut)?;

            trace!("verifying signature");
            let signature = S::verify(signature, &req).map_err(RequestSignatureError::Signature)?;

            let out = Self {
                extractor: body,
                signature,
            };

            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use actix_web::{
        App,
        http::StatusCode,
        test,
        web::{self, Bytes},
    };
    use digest::{CtOutput, Digest as _};
    use hex_literal::hex;
    use sha2::Sha256;

    use super::*;
    use crate::extract::Json;

    #[derive(Debug, Default)]
    struct JustHash(sha2::Sha256);

    impl RequestSignatureScheme for JustHash {
        type Signature = CtOutput<sha2::Sha256>;
        type Error = Infallible;

        async fn init(head: &HttpRequest) -> Result<Self, Self::Error> {
            let mut hasher = Sha256::new();

            if let Some(path) = head.uri().path_and_query() {
                hasher.update(path.as_str().as_bytes())
            }

            Ok(Self(hasher))
        }

        async fn consume_chunk(
            &mut self,
            _req: &HttpRequest,
            chunk: Bytes,
        ) -> Result<(), Self::Error> {
            self.0.update(&chunk);
            Ok(())
        }

        async fn finalize(self, _req: &HttpRequest) -> Result<Self::Signature, Self::Error> {
            let hash = self.0.finalize();
            Ok(CtOutput::new(hash))
        }
    }

    #[actix_web::test]
    async fn correctly_hashes_payload() {
        let app = test::init_service(App::new().route(
            "/service/path",
            web::get().to(|body: RequestSignature<Bytes, JustHash>| async move {
                let (_, sig) = body.into_parts();
                sig.into_bytes().to_vec()
            }),
        ))
        .await;

        let req = test::TestRequest::with_uri("/service/path").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("a5441a3d ec265f82 3758d164 1188ab1d d1093972 45012a45 fa66df70 32d02177")
                .as_ref()
        );

        let req = test::TestRequest::with_uri("/service/path")
            .set_payload("abc")
            .to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("555290a8 9e75260d fb0afead 2d5d3d70 f058c85d 1ff98bf3 06807301 7ce4c847")
                .as_ref()
        );
    }

    #[actix_web::test]
    async fn respects_inner_extractor_errors() {
        let app = test::init_service(App::new().route(
            "/",
            web::get().to(
                |body: RequestSignature<Json<u64, 4>, JustHash>| async move {
                    let (_, sig) = body.into_parts();
                    sig.into_bytes().to_vec()
                },
            ),
        ))
        .await;

        let req = test::TestRequest::default().set_json(1234).to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("4f373f6c cadfaba3 1a32cf52 04cf3db9 367609ee 6a7d7113 8e4f28ef 7c1a87a9")
                .as_ref()
        );

        // no content-type header would expect a 406 not acceptable error
        let req = test::TestRequest::default().to_request();
        let body = test::call_service(&app, req).await;
        assert_eq!(body.status(), StatusCode::NOT_ACCEPTABLE);

        // body too big would expect a 413 request payload too large
        let req = test::TestRequest::default().set_json(12345).to_request();
        let body = test::call_service(&app, req).await;
        assert_eq!(body.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
