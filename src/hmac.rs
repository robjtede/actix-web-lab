use std::fmt;

use actix_web::{dev, FromRequest, HttpRequest};
use digest::core_api::BlockSizeUser;
use digest::{generic_array::GenericArray, FixedOutput as _};
use futures_core::future::LocalBoxFuture;
use hmac::digest::Digest;
use hmac::{Mac as _, SimpleHmac};

use crate::body_extractor_fold::body_extractor_fold;

// /// The "static" in the type name refers to the fact that the key is expected to be used for every
// /// request. That's to say, this is not appropriate for APIs where its users are using signing keys
// /// specific to their account.
// ///

/// Wraps an extractor and calculates a body HMAC alongside.
///
/// If your extractor would usually be `T` and you want to create a hash of type `D` then you need
/// to use `Hmac<T, D>`. It is assumed that the `T` extractor will consume the payload.
/// Any hasher that implements [`Digest`] can be used.
///
/// Provide secret key with [`HmacConfig`] in `app_data`.
///
/// # Errors
/// This extractor produces no errors of its own and all errors from the underlying extractor are
/// propagated correctly. For example, if the payload limits are exceeded.
///
/// # Example
/// ```
/// use actix_web::{App, Responder, web};
/// use actix_web_lab::extract::{BodyHmac, HmacConfig};
/// use sha2::Sha256;
///
/// # type T = u64;
/// async fn hmac_payload(form: BodyHmac<web::Json<T>, Sha256>) -> impl Responder {
///     web::Bytes::copy_from_slice(form.hash())
/// }
///
/// let key = vec![0x01, 0x12, 0x34, 0x56];
///
/// App::new()
///     .app_data(HmacConfig::static_key(key))
/// # ;
/// ```
#[derive(Debug, Clone)]
pub struct BodyHmac<T, D>
where
    D: Digest + BlockSizeUser,
{
    body: T,
    hash: GenericArray<u8, D::OutputSize>,
}

impl<T, D: Digest> BodyHmac<T, D>
where
    D: Digest + BlockSizeUser,
{
    /// Returns hash slice.
    pub fn hash(&self) -> &[u8] {
        self.hash.as_slice()
    }

    /// Returns hash output size.
    pub fn hash_size(&self) -> usize {
        self.hash().len()
    }

    /// Returns tuple containing body type and owned hash.
    pub fn into_parts(self) -> (T, Vec<u8>) {
        let hash = self.hash().to_vec();
        (self.body, hash)
    }
}

impl<T, D> FromRequest for BodyHmac<T, D>
where
    T: FromRequest + 'static,
    D: Digest + BlockSizeUser + 'static,
{
    type Error = T::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        let config = req.app_data::<HmacConfig>().unwrap();
        let key = (config.key_fn)(req);

        body_extractor_fold(
            req,
            payload,
            SimpleHmac::<D>::new_from_slice(&key).unwrap(),
            |hasher, _req, chunk| hasher.update(&chunk),
            |body, hasher| Self {
                body,
                hash: hasher.finalize_fixed(),
            },
        )
    }
}

// #[derive(Zeroize, ZeroizeOnDrop)]
pub struct HmacConfig {
    // pub key: Vec<u8>,
    key_fn: Box<dyn Fn(&HttpRequest) -> Vec<u8>>,
}

impl HmacConfig {
    pub fn static_key(key: impl Into<Vec<u8>>) -> Self {
        let key = key.into();

        Self {
            key_fn: Box::new(move |_| key.clone()),
        }
    }

    pub fn dynamic_key(key_fn: impl Fn(&HttpRequest) -> Vec<u8> + 'static) -> Self {
        Self {
            key_fn: Box::new(key_fn),
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

#[cfg(test)]
mod tests {
    use actix_web::{
        http::StatusCode,
        test,
        web::{self, Bytes},
        App, Resource,
    };
    use hex_literal::hex;
    use sha2::{Sha256, Sha512};

    use super::*;
    use crate::extract::Json;

    #[actix_web::test]
    async fn correctly_hashes_payload_static_key() {
        let app = test::init_service(
            App::new()
                .service(
                    Resource::new("/key-blank")
                        .app_data(HmacConfig::static_key([]))
                        .route(web::get().to(|body: BodyHmac<Bytes, Sha256>| async move {
                            Bytes::copy_from_slice(body.hash())
                        })),
                )
                .service(
                    Resource::new("/key-pi")
                        .app_data(HmacConfig::static_key(hex!("31 41 59 26 53 58 97 93")))
                        .route(web::get().to(|body: BodyHmac<Bytes, Sha256>| async move {
                            Bytes::copy_from_slice(body.hash())
                        })),
                )
                .service(
                    Resource::new("/sha512")
                        .app_data(HmacConfig::static_key([]))
                        .route(web::get().to(|body: BodyHmac<Bytes, Sha512>| async move {
                            Bytes::copy_from_slice(body.hash())
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

        let req = test::TestRequest::with_uri("/sha512").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!(
                "b936cee8 6c9f87aa 5d3c6f2e 84cb5a42 39a5fe50 480a6ec6 6b70ab5b 1f4ac673
                 0c6c5154 21b327ec 1d69402e 53dfb49a d7381eb0 67b338fd 7b0cb222 47225d47"
            )
        );
    }

    #[actix_web::test]
    async fn correctly_hashes_payload_dynamic_key() {
        fn base64_api_key_header(req: &HttpRequest) -> Vec<u8> {
            let key_b64 = req.headers().get("api-key").unwrap().as_bytes();
            base64::decode(key_b64).unwrap()
        }

        let app = test::init_service(
            App::new()
                .service(
                    Resource::new("/sha256")
                        .app_data(HmacConfig::dynamic_key(base64_api_key_header))
                        .route(web::get().to(|body: BodyHmac<Bytes, Sha256>| async move {
                            Bytes::copy_from_slice(body.hash())
                        })),
                )
                .service(
                    Resource::new("/sha512")
                        .app_data(HmacConfig::dynamic_key(base64_api_key_header))
                        .route(web::get().to(|body: BodyHmac<Bytes, Sha512>| async move {
                            Bytes::copy_from_slice(body.hash())
                        })),
                ),
        )
        .await;

        let req = test::TestRequest::with_uri("/sha256")
            .insert_header(("api-key", ""))
            .set_payload("abc")
            .to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!("fd7adb15 2c05ef80 dccf50a1 fa4c05d5 a3ec6da9 5575fc31 2ae7c5d0 91836351")
        );

        let req = test::TestRequest::with_uri("/sha256")
            .insert_header(("api-key", "MTIzNA=="))
            .set_payload("abc")
            .to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!("2a76332d a8983432 bc65ee84 0c4c9493 26b4a010 d2175a71 5105e90e 59c206e8")
        );

        let req = test::TestRequest::with_uri("/sha512")
            .insert_header(("api-key", "MTIzNA=="))
            .set_payload("abc")
            .to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            &body[..],
            hex!(
                "6f91e2c8 377a9f67 25ceb5b1 17c816a7 dc5e0e2e 17630972 b7f05d41 f0a56b3f
                 d4a2a5d7 465d740f 0eed1ed9 0af5b917 47bf3c0c 77585536 5c778c34 ea7cd805"
            )
        );
    }

    #[actix_web::test]
    async fn respects_inner_extractor_errors() {
        let app = test::init_service(App::new().app_data(HmacConfig::static_key([])).route(
            "/",
            web::get().to(|body: BodyHmac<Json<u64, 4>, Sha256>| async move {
                Bytes::copy_from_slice(body.hash())
            }),
        ))
        .await;

        let req = test::TestRequest::default()
            .set_json(1234usize)
            .to_request();
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
        let req = test::TestRequest::default()
            .set_json(12345usize)
            .to_request();
        let body = test::call_service(&app, req).await;
        assert_eq!(body.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }
}
