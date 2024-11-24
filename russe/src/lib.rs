//! Server-Sent Events (SSE) decoder.

#![forbid(unsafe_code)]
#![allow(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use bytestring::ByteString;

mod decoder;
mod error;

pub use self::error::Error;

pub(crate) const NEWLINE: u8 = b'\n';
pub(crate) const SSE_DELIMITER: &[u8] = b"\n\n";

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// millis
    retry: Option<u64>,

    /// named event
    event: Option<ByteString>,

    /// is always string ?
    data: Option<ByteString>,

    /// is always numeric ?
    id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    Message(Message),
    Comment(ByteString),
    Retry(u64),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[macro_export]
    macro_rules! assert_none {
        ($exp:expr) => {{
            let exp = $exp;
            assert!(exp.is_none(), "Expected None; got: {exp:?}");
        }};
    }

    impl Message {
        pub(crate) fn data(data: impl Into<ByteString>) -> Self {
            Self {
                data: Some(data.into()),
                ..Default::default()
            }
        }
    }

    // simplifies some tests
    #[allow(clippy::derivable_impls)]
    impl Default for Message {
        fn default() -> Self {
            Self {
                retry: None,
                event: None,
                data: None,
                id: None,
            }
        }
    }
}
