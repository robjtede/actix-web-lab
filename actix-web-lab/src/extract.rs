//! Experimental extractors.

/// An alias for [`actix_web::web::Data<T>`] with a more descriptive name.
pub type SharedData<T> = actix_web::web::Data<T>;

pub use crate::{
    body_limit::BodyLimit,
    bytes::{Bytes, DEFAULT_BYTES_LIMIT},
    json::{Json, DEFAULT_JSON_LIMIT},
    lazy_data::LazyData,
    local_data::LocalData,
    path::Path,
    query::Query,
    request_signature::{RequestSignature, RequestSignatureError, RequestSignatureScheme},
    swap_data::SwapData,
};
