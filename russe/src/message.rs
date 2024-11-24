use bytestring::ByteString;

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// millis
    pub(crate) retry: Option<u64>,

    /// named event
    pub(crate) event: Option<ByteString>,

    /// is always string ?
    pub(crate) data: Option<ByteString>,

    /// is always numeric ?
    pub(crate) id: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
