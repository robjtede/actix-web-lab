//! JSON extraction with deser.
//!
//! Requires the `deser` feature. See [`DeserJson`] for usage and error context.

pub use crate::{
    deser_json::{DeserJson, DeserJsonPayloadError},
    json_serde::DEFAULT_JSON_LIMIT,
};
