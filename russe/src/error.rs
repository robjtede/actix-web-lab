use core::str;
use std::io;

/// SSE encoding or decoding error.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// An event ID or event name contains a forbidden character.
    InvalidFieldValue,

    /// Stream contained invalid UTF-8.
    InvalidUtf8 {
        /// Source erorr.
        source: str::Utf8Error,
    },

    /// I/O error.
    Io {
        /// Source error.
        source: io::Error,
    },
}

impl_more::impl_display_enum! {
    Error:
    InvalidFieldValue => "SSE field value contains a forbidden character",
    InvalidUtf8 { .. } => "Stream contained invalid UTF-8",
    Io { .. } => "I/O error",
}

impl_more::impl_error_enum! {
    Error:
    InvalidUtf8 { source } => source,
    Io { source } => source,
}

impl_more::impl_enum_from!(str::Utf8Error => Error::InvalidUtf8 { source });
impl_more::impl_enum_from!(io::Error => Error::Io { source });
