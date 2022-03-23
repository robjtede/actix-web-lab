//! Experimental extractors.

/// An alias for [`actix_web::web::Data<T>`] with a more descriptive name.
pub type SharedData<T> = actix_web::web::Data<T>;

pub use crate::body_hash::BodyHash;
pub use crate::body_hmac::{BodyHmac, HmacConfig};
pub use crate::json::{Json, DEFAULT_JSON_LIMIT};
pub use crate::lazy_data::LazyData;
pub use crate::local_data::LocalData;
pub use crate::path::Path;
pub use crate::query::Query;
#[doc(hidden)]
pub use crate::request_hash::{RequestHash, RequestHasher};
#[doc(hidden)]
pub use crate::request_signature::{
    RequestSignature, RequestSignatureError, RequestSignatureScheme,
};
pub use crate::swap_data::SwapData;
