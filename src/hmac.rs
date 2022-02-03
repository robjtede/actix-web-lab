use std::fmt;
use std::marker::PhantomData;

use actix_web::{dev, FromRequest, HttpRequest};
use digest::CtOutput;
use futures_core::future::LocalBoxFuture;
use hmac::digest::Digest;
use hmac::Mac as _;

use crate::body_extractor_fold::body_extractor_fold;

type HmacSha256 = hmac::Hmac<sha2::Sha256>;

/// Wraps an extractor and calculates a body checksum hash alongside.
///
/// If your extractor would usually be `T` and you want to create a hash of type `D` then you need
/// to use `Hmac<T, D>`. It is assumed that the `T` extractor will consume the payload.
/// Any hasher that implements [`Digest`] can be used.
///
/// # Errors
/// This extractor produces no errors of its own and all errors from the underlying extractor are
/// propagated correctly. For example, if the payload limits are exceeded.
///
/// # Example
/// ```
/// use actix_web::{App, Responder, web};
/// use actix_web_lab::extract::{Hmac, HmacConfig};
/// use sha2::Sha256;
///
/// # type T = u64;
/// async fn hmac_payload(form: Hmac<web::Json<T>, Sha256>) -> impl Responder {
///     web::Bytes::from(form.hash())
/// }
///
/// let key = vec![0x01, 0x12, 0x34, 0x56];
///
/// App::new()
///     .app_data(HmacConfig::new(&key))
/// # ;
/// ```
#[derive()]
pub struct Hmac<T, D: Digest> {
    body: T,
    hash: CtOutput<HmacSha256>,
    _phantom: PhantomData<(D,)>,
}

pub struct HmacConfig {
    key: Vec<u8>,
}

impl HmacConfig {
    pub fn new(key: &[u8]) -> Self {
        Self {
            key: key.to_owned(),
        }
    }
}

impl fmt::Debug for HmacConfig {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HmacConfig")
            .field("key", &"[redacted]")
            .finish()
    }
}

impl<T, D: Digest> Hmac<T, D> {
    /// Returns hash slice.
    pub fn hash(&self) -> Vec<u8> {
        let out = self.hash.clone().into_bytes();
        out.as_slice().to_vec()
    }

    /// Returns hash output size.
    pub fn hash_size(&self) -> usize {
        self.hash().len()
    }

    /// Returns tuple containing body type and owned hash.
    pub fn into_parts(self) -> (T, Vec<u8>) {
        let hash = self.hash();
        (self.body, hash)
    }
}

impl<T, D> FromRequest for Hmac<T, D>
where
    D: Digest,
    T: FromRequest + 'static,
{
    type Error = T::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        let config = req.app_data::<HmacConfig>().unwrap();

        body_extractor_fold(
            req,
            payload,
            HmacSha256::new_from_slice(&config.key).unwrap(),
            |hasher, _req, chunk| hasher.update(&chunk),
            |body, hasher| Self {
                body,
                hash: hasher.finalize(),
                _phantom: PhantomData,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use actix_web::{
        http::StatusCode,
        test,
        web::{self, Bytes},
        App, Resource,
    };
    use hex_literal::hex;
    use sha2::Sha256;

    use super::*;
    use crate::extract::Json;

    #[actix_web::test]
    async fn correctly_hashes_payload() {
        let app = test::init_service(
            App::new()
                .service(
                    Resource::new("/key-blank")
                        .app_data(HmacConfig::new(&[]))
                        .route(web::get().to(|body: Hmac<Bytes, Sha256>| async move {
                            Bytes::from(body.hash())
                        })),
                )
                .service(
                    Resource::new("/key-pi")
                        .app_data(HmacConfig::new(&hex!("31 41 59 26 53 58 97 93")))
                        .route(web::get().to(|body: Hmac<Bytes, Sha256>| async move {
                            Bytes::from(body.hash())
                        })),
                ),
        )
        .await;

        let req = test::TestRequest::with_uri("/key-blank").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!("b613679a 0814d9ec 772f95d7 78c35fc5 ff1697c4 93715653 c6c71214 4292c5ad")
        );

        let req = test::TestRequest::with_uri("/key-blank")
            .set_payload("abc")
            .to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!("fd7adb15 2c05ef80 dccf50a1 fa4c05d5 a3ec6da9 5575fc31 2ae7c5d0 91836351")
        );

        let req = test::TestRequest::with_uri("/key-pi").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!("bbb4789d d01448ce 3c87ebec a78d45a1 6b6072db 2b639648 2783a284 f2ce5713")
        );

        let req = test::TestRequest::with_uri("/key-pi")
            .set_payload("abc")
            .to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!("67029104 cd676bae 23d74ac6 bdce84fc 80764b5e 5327a624 515fc5a5 2c240f8e")
        );
    }

    #[actix_web::test]
    async fn respects_inner_extractor_errors() {
        let app = test::init_service(
            App::new().app_data(HmacConfig::new(&[])).route(
                "/",
                web::get()
                    .to(|body: Hmac<Json<u64, 4>, Sha256>| async move { Bytes::from(body.hash()) }),
            ),
        )
        .await;

        let req = test::TestRequest::default().set_json(1234).to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!("5a697c67 68fcb4f3 63874b4d 73c517a6 e7f8932d 23c31b6e a52bebd2 c3f4aa05")
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
