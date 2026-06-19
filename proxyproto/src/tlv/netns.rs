use std::{borrow::Cow, str};

use super::Tlv;

const PP2_TYPE_NETNS: u8 = 0x30;

/// Network namespace name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetNamespace {
    namespace: String,
}

impl NetNamespace {
    /// Constructs a new network namespace TLV from a UTF-8 string.
    ///
    /// # Panics
    /// Panics if `namespace` is an empty string.
    pub fn new(namespace: impl Into<String>) -> Self {
        let namespace = namespace.into();

        assert!(
            !namespace.is_empty(),
            "NetNamespace TLV value cannot be empty"
        );

        Self { namespace }
    }

    /// Returns the network namespace.
    pub fn as_str(&self) -> &str {
        &self.namespace
    }
}

impl Tlv for NetNamespace {
    const TYPE: u8 = PP2_TYPE_NETNS;

    fn try_from_value(value: &[u8]) -> Option<Self> {
        Some(Self {
            namespace: str::from_utf8(value).ok()?.to_owned(),
        })
    }

    fn value_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.namespace.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn netns_tlv_round_trip() {
        let netns = NetNamespace::new("blue");

        assert_eq!(netns.as_str(), "blue");
        assert_eq!(netns.value_bytes(), b"blue".as_slice());
        assert_eq!(NetNamespace::try_from_value(b"blue"), Some(netns.clone()));
        assert_eq!(NetNamespace::try_from_parts(0x30, b"blue"), Some(netns));
        assert_eq!(NetNamespace::try_from_parts(0x05, b"blue"), None);
    }

    #[test]
    fn netns_tlv_rejects_invalid_utf8_when_decoding() {
        assert_eq!(NetNamespace::try_from_value(&[0xff]), None);
    }

    #[test]
    #[should_panic = "NetNamespace TLV value cannot be empty"]
    fn netns_tlv_rejects_empty_constructor_value() {
        NetNamespace::new("");
    }
}
