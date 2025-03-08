//! Experimental extractors.

/// An alias for [`actix_web::web::Data<T>`] with a more descriptive name.
pub type SharedData<T> = actix_web::web::Data<T>;

pub use crate::{
    body_limit::{BodyLimit, DEFAULT_BODY_LIMIT},
    bytes::{Bytes, DEFAULT_BYTES_LIMIT},
    host::Host,
    json::{DEFAULT_JSON_LIMIT, Json, JsonDeserializeError, JsonPayloadError},
    lazy_data::LazyData,
    local_data::LocalData,
    path::Path,
    query::{Query, QueryDeserializeError},
    request_signature::{RequestSignature, RequestSignatureError, RequestSignatureScheme},
    swap_data::SwapData,
    url_encoded_form::{
        DEFAULT_URL_ENCODED_FORM_LIMIT, UrlEncodedForm, UrlEncodedFormDeserializeError,
    },
    x_forwarded_prefix::ReconstructedPath,
};
