use std::sync::Arc;

use actix_http::BoxedPayloadStream;
use actix_web::{dev, web::Bytes, FromRequest, HttpRequest};
use digest::{generic_array::GenericArray, Digest};
use futures_core::{future::LocalBoxFuture, Future};
use futures_util::{FutureExt as _, StreamExt as _};
use local_channel::mpsc::{self, Receiver};
use tokio::try_join;

type HashFn<D> = Arc<dyn Fn(D, HttpRequest, Receiver<Bytes>) -> LocalBoxFuture<'static, D>>;

/// TODO
pub struct RequestHasher<D>
where
    D: Digest + 'static,
{
    hash_fn: HashFn<D>,
}

impl<D> RequestHasher<D>
where
    D: Digest + 'static,
{
    /// TODO
    pub fn from_fn<F, Fut>(hash_fn: F) -> Self
    where
        F: Fn(D, HttpRequest, Receiver<Bytes>) -> Fut + 'static,
        Fut: Future<Output = D> + 'static,
    {
        Self {
            hash_fn: Arc::new(move |arg1, arg2, arg3| Box::pin((hash_fn)(arg1, arg2, arg3))),
        }
    }

    /// TODO
    pub fn digest_body() -> Self {
        Self {
            hash_fn: Arc::new(|mut hasher, _req, mut pl_stream| {
                Box::pin(async move {
                    while let Some(chunk) = pl_stream.next().await {
                        hasher.update(&chunk);
                    }

                    hasher
                })
            }),
        }
    }
}

/// Wraps an extractor and calculates a request checksum hash alongside.
///
/// If your extractor would usually be `T` and you want to create a hash of type `D` then you need
/// to use `BodyHash<T, D>`. It is assumed that the `T` extractor will consume the payload.
/// Any hasher that implements [`Digest`] can be used.
///
/// # Errors
/// This extractor produces no errors of its own and all errors from the underlying extractor are
/// propagated correctly; for example, if the payload limits are exceeded.
///
/// # Example
/// ```
/// use actix_web::{Responder, web};
/// use actix_web_lab::extract::RequestHash;
/// use sha2::Sha256;
///
/// # type T = u64;
/// async fn hash_payload(form: RequestHash<web::Json<T>, Sha256>) -> impl Responder {
///     if !form.verify_slice(b"correct-signature") {
///         // return unauthorized error
///     }
///
///     "Ok"
/// }
/// ```
#[derive(Debug, Clone)]
pub struct RequestHash<B, D: Digest> {
    body: B,
    hash: GenericArray<u8, D::OutputSize>,
}

impl<T, D: Digest> RequestHash<T, D> {
    /// Returns hash slice.
    pub fn hash(&self) -> &[u8] {
        self.hash.as_slice()
    }

    /// Returns hash output size.
    pub fn hash_size(&self) -> usize {
        self.hash.len()
    }

    /// Verifies HMAC hash against provides `tag` using constant-time equality.
    pub fn verify_slice(&self, tag: &[u8]) -> bool {
        use subtle::ConstantTimeEq as _;
        self.hash.ct_eq(tag).into()
    }

    /// Returns tuple containing body type, raw body bytes, and owned hash.
    pub fn into_parts(self) -> (T, Vec<u8>) {
        let hash = self.hash().to_vec();
        (self.body, hash)
    }
}

impl<T, D> FromRequest for RequestHash<T, D>
where
    T: FromRequest + 'static,
    D: Digest + 'static,
{
    type Error = T::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        let req = req.clone();
        let payload = payload.take();

        Box::pin(async move {
            let hasher = D::new();
            let hash_fn = Arc::clone(&req.app_data::<RequestHasher<D>>().unwrap().hash_fn);

            let (tx, rx) = mpsc::channel();

            // wrap payload in stream that reads chunks and clones them (cheaply) back here
            let proxy_stream: BoxedPayloadStream = Box::pin(payload.inspect(move |res| {
                if let Ok(chunk) = res {
                    log::trace!("yielding {} byte chunk", chunk.len());
                    tx.send(chunk.clone()).unwrap();
                }
            }));

            log::trace!("creating proxy payload");
            let mut proxy_payload = dev::Payload::from(proxy_stream);
            let body_fut = T::from_request(&req, &mut proxy_payload);

            // run update function as chunks are yielded from channel
            let hash_fut = ((hash_fn)(hasher, req, rx)).map(Ok);

            log::trace!("driving both futures");
            let (body, hash) = try_join!(body_fut, hash_fut)?;

            let out = Self {
                body,
                hash: hash.finalize(),
            };

            Ok(out)
        })
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        http::StatusCode,
        test,
        web::{self, Bytes},
        App,
    };
    use hex_literal::hex;
    use sha2::Sha256;

    use super::*;
    use crate::extract::Json;

    #[actix_web::test]
    async fn correctly_hashes_payload() {
        let app = test::init_service(
            App::new()
                .app_data(RequestHasher::<Sha256>::digest_body())
                .route(
                    "/service/path",
                    web::get().to(|body: RequestHash<Bytes, Sha256>| async move {
                        Bytes::copy_from_slice(body.hash())
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::with_uri("/service/path").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("e3b0c442 98fc1c14 9afbf4c8 996fb924 27ae41e4 649b934c a495991b 7852b855")
                .as_ref()
        );

        let req = test::TestRequest::with_uri("/service/path")
            .set_payload("abc")
            .to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad")
                .as_ref()
        );
    }

    #[actix_web::test]
    async fn respects_inner_extractor_errors() {
        let app = test::init_service(
            App::new()
                .app_data(RequestHasher::<Sha256>::digest_body())
                .route(
                    "/",
                    web::get().to(|body: RequestHash<Json<u64, 4>, Sha256>| async move {
                        Bytes::copy_from_slice(body.hash())
                    }),
                ),
        )
        .await;

        let req = test::TestRequest::default().set_json(1234).to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("03ac6742 16f3e15c 761ee1a5 e255f067 953623c8 b388b445 9e13f978 d7c846f4")
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
