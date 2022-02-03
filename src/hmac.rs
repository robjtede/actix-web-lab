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
/// use actix_web::{Responder, web};
/// use actix_web_lab::extract::BodyHash;
/// use sha2::Sha256;
///
/// # type T = u64;
/// async fn hash_payload(form: Hmac<web::Json<T>, Sha256>) -> impl Responder {
///     web::Bytes::copy_from_slice(form.hash())
/// }
///
/// App::new()
///     .app_data(HmacConfig)
/// ```
#[derive()]
pub struct Hmac<T, D: Digest> {
    body: T,
    hash: CtOutput<HmacSha256>,
    _phantom: PhantomData<(D,)>,
}

struct HmacConfig {
    key: Vec<u8>,
}

impl HmacConfig {
    fn default() -> Self {
        Self { key: vec![] }
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

    // /// Returns hash output size.
    // pub fn hash_size(&self) -> usize {
    //     self.hash.len()
    // }

    // /// Returns tuple containing body type and owned hash.
    // pub fn into_parts(self) -> (T, Vec<u8>) {
    //     let hash = self.hash().to_vec();
    //     (self.body, hash)
    // }
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
        let app = test::init_service(App::new().app_data(HmacConfig::default()).route(
            "/",
            web::get().to(|body: Hmac<Bytes, Sha256>| async move { Bytes::from(body.hash()) }),
        ))
        .await;

        let req = test::TestRequest::default().to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("b613679a 0814d9ec 772f95d7 78c35fc5 ff1697c4 93715653 c6c71214 4292c5ad")
                .as_ref()
        );

        let req = test::TestRequest::default().set_payload("abc").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("fd7adb15 2c05ef80 dccf50a1 fa4c05d5 a3ec6da9 5575fc31 2ae7c5d0 91836351")
                .as_ref()
        );
    }

    // #[actix_web::test]
    // async fn respects_inner_extractor_errors() {
    //     let app = test::init_service(App::new().route(
    //         "/",
    //         web::get().to(|body: Hmac<Json<u64, 4>, Sha256>| async move {
    //             Bytes::copy_from_slice(body.hash())
    //         }),
    //     ))
    //     .await;

    //     let req = test::TestRequest::default().set_json(1234).to_request();
    //     let body = test::call_and_read_body(&app, req).await;
    //     assert_eq!(
    //         body,
    //         hex!("03ac6742 16f3e15c 761ee1a5 e255f067 953623c8 b388b445 9e13f978 d7c846f4")
    //             .as_ref()
    //     );

    //     // no body would expect a 400 content type error
    //     let req = test::TestRequest::default().to_request();
    //     let body = test::call_service(&app, req).await;
    //     assert_eq!(body.status(), StatusCode::BAD_REQUEST);

    //     // body too big would expect a 413 request payload too large
    //     let req = test::TestRequest::default().set_json(12345).to_request();
    //     let body = test::call_service(&app, req).await;
    //     assert_eq!(body.status(), StatusCode::PAYLOAD_TOO_LARGE);
    // }
}
