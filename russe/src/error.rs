use core::{error::Error as StdError, str};
use std::io;

use truncate_safe::truncate;

/// SSE request, encoding, or decoding error.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// An event ID or event name contains a forbidden character.
    InvalidFieldValue {
        /// Field name: `id` or `event`.
        field: &'static str,

        /// Forbidden character.
        character: char,

        /// Zero-based byte offset of the character in the original value.
        offset: usize,

        /// Escaped excerpt around the character, limited to 64 bytes.
        ///
        /// An ellipsis marks each end where the value was truncated.
        value: String,
    },

    /// Stream contained invalid UTF-8.
    InvalidUtf8 {
        /// Source erorr.
        source: str::Utf8Error,
    },

    /// Initial HTTP request failed.
    Http {
        /// Boxed to support multiple HTTP clients.
        source: Box<dyn StdError + Send + Sync>,
    },

    /// I/O error.
    Io {
        /// Source error.
        source: io::Error,
    },
}

impl Error {
    pub(crate) fn invalid_field_value(field: &'static str, value: &str, offset: usize) -> Self {
        let character = char::from(value.as_bytes()[offset]);
        let start = offset - context_len(value[..offset].chars().rev());
        let end = offset + 1 + context_len(value[offset + 1..].chars());

        let mut excerpt = String::new();

        if start > 0 {
            excerpt.push('…');
        }

        excerpt.extend(value[start..end].chars().flat_map(char::escape_debug));

        if end < value.len() {
            excerpt.push('…');
        }

        Self::InvalidFieldValue {
            field,
            character,
            offset,
            value: excerpt,
        }
    }
}

fn context_len(chars: impl Iterator<Item = char>) -> usize {
    // Reserve six bytes for ellipses and two for the escaped forbidden character.
    const MAX_ESCAPED_LEN: usize = 28;

    let text: String = chars.take(MAX_ESCAPED_LEN).collect();
    let mut context = truncate(&text, MAX_ESCAPED_LEN);

    while context
        .chars()
        .flat_map(char::escape_debug)
        .map(char::len_utf8)
        .sum::<usize>()
        > MAX_ESCAPED_LEN
    {
        context = truncate(context, context.len() - 1);
    }

    context.len()
}

impl_more::impl_display_enum! {
    Error:
    InvalidFieldValue { field, character, offset, value } => "invalid SSE {field}: {character:?} at byte {offset}, near \"{value}\"",
    InvalidUtf8 { .. } => "Stream contained invalid UTF-8",
    Http { .. } => "HTTP request failed",
    Io { .. } => "I/O error",
}

impl_more::impl_error_enum! {
    Error:
    InvalidUtf8 { source } => source,
    Http { source } => source.as_ref(),
    Io { source } => source,
}

impl_more::impl_enum_from!(str::Utf8Error => Error::InvalidUtf8 { source });
impl_more::impl_enum_from!(io::Error => Error::Io { source });
