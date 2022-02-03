use actix_web::{dev, FromRequest, HttpRequest};
use digest::{generic_array::GenericArray, Digest};
use futures_core::future::LocalBoxFuture;

use crate::body_fold::body_fold;

/// Wraps an extractor and calculates a body checksum hash alongside.
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
/// use actix_web_lab::extract::BodyHash;
/// use sha2::Sha256;
///
/// # type T = u64;
/// async fn hash_payload(form: BodyHash<web::Json<T>, Sha256>) -> impl Responder {
///     web::Bytes::copy_from_slice(form.hash())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BodyHash<T, D: Digest> {
    body: T,
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

    /// Returns tuple containing body type and owned hash.
    pub fn into_parts(self) -> (T, Vec<u8>) {
        let hash = self.hash().to_vec();
        (self.body, hash)
    }
}

impl<T, D> FromRequest for BodyHash<T, D>
where
    D: Digest + 'static,
    T: FromRequest + 'static,
{
    type Error = T::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        body_fold(
            req,
            payload,
            D::new(),
            |hasher, _req, chunk| hasher.update(&chunk),
            |body, hasher| Self {
                body,
                hash: hasher.finalize(),
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
        App,
    };
    use hex_literal::hex;
    use sha2::Sha256;

    use super::*;
    use crate::extract::Json;

    #[actix_web::test]
    async fn correctly_hashes_payload() {
        let app = test::init_service(App::new().route(
            "/",
            web::get().to(|body: BodyHash<Bytes, Sha256>| async move {
                Bytes::copy_from_slice(body.hash())
            }),
        ))
        .await;

        let req = test::TestRequest::default().to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("e3b0c442 98fc1c14 9afbf4c8 996fb924 27ae41e4 649b934c a495991b 7852b855")
                .as_ref()
        );

        let req = test::TestRequest::default().set_payload("abc").to_request();
        let body = test::call_and_read_body(&app, req).await;
        assert_eq!(
            body,
            hex!("ba7816bf 8f01cfea 414140de 5dae2223 b00361a3 96177a9c b410ff61 f20015ad")
                .as_ref()
        );
    }

    #[actix_web::test]
    async fn respects_inner_extractor_errors() {
        let app = test::init_service(App::new().route(
            "/",
            web::get().to(|body: BodyHash<Json<u64, 4>, Sha256>| async move {
                Bytes::copy_from_slice(body.hash())
            }),
        ))
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
