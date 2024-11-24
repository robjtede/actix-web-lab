use std::time::Duration;

use bytestring::ByteString;

/// An SSE data message.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// Message data.
    pub(crate) data: ByteString,

    /// Name of event.
    pub(crate) event: Option<ByteString>,

    /// Recommended retry delay in milliseconds.
    pub(crate) retry: Option<Duration>,

    /// Event identifier.
    ///
    /// Used in Last-Event-ID header.
    // TODO: not always a number
    // see https://github.com/whatwg/html/issues/7363
    pub(crate) id: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    impl Message {
        pub(crate) fn data(data: impl Into<ByteString>) -> Self {
            Self {
                data: data.into(),
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
                data: ByteString::new(),
                id: None,
            }
        }
    }
}
