use std::fmt;

use actix_http::BoxedPayloadStream;
use actix_web::{dev, web::Bytes, Error, FromRequest, HttpRequest, ResponseError};
use async_trait::async_trait;
use digest::{CtOutput, OutputSizeUser};
use futures_core::future::LocalBoxFuture;
use futures_util::{FutureExt as _, StreamExt as _, TryFutureExt as _};
use generic_array::GenericArray;
use local_channel::mpsc;
use tokio::try_join;
use tracing::trace;

/// todo
#[async_trait(?Send)]
pub trait RequestSignatureScheme: Sized {
    /// todo
    type Output: OutputSizeUser;

    /// todo
    type Error: Into<Error>;

    /// todo
    ///
    /// - initialize signature scheme struct
    /// - key derivation / hashing
    /// - pre-body hash updates
    async fn init(req: &HttpRequest) -> Result<Self, Self::Error>;

    /// todo
    async fn digest_chunk(&mut self, req: &HttpRequest, chunk: Bytes) -> Result<(), Self::Error>;

    /// todo
    ///
    /// - post-body hash updates
    /// - finalization
    /// - hash output
    async fn finalize(self, req: &HttpRequest) -> Result<CtOutput<Self::Output>, Self::Error>;

    /// todo
    ///
    /// - return signature if valid
    /// - return error if not
    /// - by default the signature not checked and returned as-is
    #[allow(unused_variables)]
    fn verify(
        signature: CtOutput<Self::Output>,
        req: &HttpRequest,
    ) -> Result<CtOutput<Self::Output>, Self::Error> {
        Ok(signature)
    }
}

/// Wraps an extractor and calculates a request signature hash alongside.
///
/// Warning: Currently, this will always take the body meaning that if a body extractor is used,
/// this needs to wrap it or else it will not work.
#[derive(Clone)]
pub struct RequestSignature<T, S: RequestSignatureScheme> {
    extractor: T,
    signature: CtOutput<S::Output>,
}

impl<T, S: RequestSignatureScheme> RequestSignature<T, S> {
    /// Verifies HMAC hash against provides `tag` using constant-time equality.
    pub fn verify_slice(&self, tag: &[u8]) -> bool {
        use subtle::ConstantTimeEq as _;
        self.signature
            .ct_eq(&CtOutput::new(GenericArray::from_slice(tag).to_owned()))
            .into()
    }

    /// Returns tuple containing body type, and owned hash.
    pub fn into_parts(self) -> (T, Vec<u8>) {
        (self.extractor, self.signature.into_bytes().to_vec())
    }
}

/// todo
pub enum RequestSignatureError<T: FromRequest, S: RequestSignatureScheme> {
    /// todo
    Extractor(T::Error),

    /// todo
    Signature(S::Error),
}

impl<T: FromRequest, S: RequestSignatureScheme> fmt::Debug for RequestSignatureError<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RequestSignatureError")
    }
}

impl<T: FromRequest, S: RequestSignatureScheme> fmt::Display for RequestSignatureError<T, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RequestSignatureError")
    }
}

impl<T: FromRequest, S: RequestSignatureScheme> ResponseError for RequestSignatureError<T, S> {}

impl<T, S> FromRequest for RequestSignature<T, S>
where
    T: FromRequest + 'static,
    S: RequestSignatureScheme + 'static,
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
                        sig_scheme.digest_chunk(&req, chunk).await?;
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
        http::StatusCode,
        test,
        web::{self, Bytes},
        App,
    };
    use digest::Digest as _;
    use hex_literal::hex;
    use sha2::Sha256;

    use super::*;
    use crate::extract::Json;

    #[derive(Debug, Default)]
    struct JustHash(sha2::Sha256);

    #[async_trait(?Send)]
    impl RequestSignatureScheme for JustHash {
        type Output = sha2::Sha256;
        type Error = Infallible;

        async fn init(head: &HttpRequest) -> Result<Self, Self::Error> {
            let mut hasher = Sha256::new();

            if let Some(path) = head.uri().path_and_query() {
                hasher.update(path.as_str().as_bytes())
            }

            Ok(Self(hasher))
        }

        async fn digest_chunk(
            &mut self,
            _req: &HttpRequest,
            chunk: Bytes,
        ) -> Result<(), Self::Error> {
            self.0.update(&chunk);
            Ok(())
        }

        async fn finalize(self, _req: &HttpRequest) -> Result<CtOutput<Self::Output>, Self::Error> {
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
                sig
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

    /// TODO: fix test
    #[ignore]
    #[actix_web::test]
    async fn respects_inner_extractor_errors() {
        let app = test::init_service(App::new().route(
            "/",
            web::get().to(
                |body: RequestSignature<Json<u64, 4>, JustHash>| async move {
                    let (_, sig) = body.into_parts();
                    sig
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

        // no body would expect a 400 content type error
        let req = test::TestRequest::default().to_request();
        let body = test::call_service(&app, req).await;
        assert_eq!(body.status(), StatusCode::BAD_REQUEST);

        // body too big would expect a 413 request payload too large
        let req = test::TestRequest::default().set_json(12345).to_request();
        let body = test::call_service(&app, req).await;
        assert_eq!(body.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
