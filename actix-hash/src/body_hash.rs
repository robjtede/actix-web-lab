use actix_web::{dev, web::Bytes, FromRequest, HttpRequest};
use digest::{generic_array::GenericArray, Digest};
use futures_core::future::LocalBoxFuture;

use crate::body_extractor_fold::body_extractor_fold;

/// Parts of the resulting body hash extractor.
pub struct BodyHashParts<T> {
    /// Extracted body item.
    pub body: T,

    /// Bytes of the body that were extracted.
    pub body_bytes: Bytes,

    /// Bytes of the calculated hash.
    pub hash_bytes: Vec<u8>,
}

/// Wraps an extractor and calculates a body checksum hash alongside.
///
/// If your extractor would usually be `T` and you want to create a hash of type `D` then you need
/// to use `BodyHash<T, D>`. It is assumed that the `T` extractor will consume the payload.
///
/// Any hasher that implements [`Digest`] can be used. Type aliases for common hashing algorithms
/// are available at the crate root.
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
///     if !form.verify_slice(b"correct-signature") {
///         // return unauthorized error
///     }
///
///     "Ok"
/// }
/// ```
#[derive(Debug, Clone)]
pub struct BodyHash<T, D: Digest> {
    body: T,
    bytes: Bytes,
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
            body: self.body,
            body_bytes: self.bytes,
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
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut dev::Payload) -> Self::Future {
        body_extractor_fold(
            req,
            payload,
            D::new(),
            |hasher, _req, chunk| hasher.update(&chunk),
            |body, bytes, hasher| Self {
                body,
                bytes,
                hash: hasher.finalize(),
            },
        )
    }
}
