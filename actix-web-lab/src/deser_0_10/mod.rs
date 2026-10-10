//! JSON extraction with deser 0.10.
//!
//! Requires the `deser-0_10` feature. See [`DeserJson`] for usage and error context.

mod json;

pub use self::json::{DeserJson, DeserJsonDeserializeError, DeserJsonPayloadError};
pub use crate::json::DEFAULT_JSON_LIMIT;
