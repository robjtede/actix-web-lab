use std::borrow::Cow;

use super::Tlv;

const PP2_TYPE_ALPN: u8 = 0x01;

/// Application-Layer Protocol Negotiation (ALPN).
///
/// It is a byte sequence defining the upper layer protocol in use over the connection. The most
/// common use case will be to pass the exact copy of the ALPN extension of the Transport Layer
/// Security (TLS) protocol as defined by RFC 7301.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alpn {
    alpn: Vec<u8>,
}

impl Alpn {
    /// Constructs a new ALPN TLV.
    ///
    /// # Panics
    /// Panics if `alpn` is empty (i.e., has length of 0).
    pub fn new(alpn: impl Into<Vec<u8>>) -> Self {
        let alpn = alpn.into();

        assert!(!alpn.is_empty(), "ALPN TLV value cannot be empty");

        Self { alpn }
    }

    /// Returns the ALPN protocol bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.alpn
    }
}

impl Tlv for Alpn {
    const TYPE: u8 = PP2_TYPE_ALPN;

    fn try_from_value(value: &[u8]) -> Option<Self> {
        Some(Self {
            alpn: value.to_owned(),
        })
    }

    fn value_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.alpn)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alpn_tlv_round_trip() {
        let alpn = Alpn::new("h2");

        assert_eq!(alpn.as_bytes(), b"h2");
        assert_eq!(alpn.value_bytes(), b"h2".as_slice());
        assert_eq!(Alpn::try_from_value(b"h2"), Some(alpn.clone()));
        assert_eq!(Alpn::try_from_parts(0x01, b"h2"), Some(alpn));
        assert_eq!(Alpn::try_from_parts(0x02, b"h2"), None);
    }

    #[test]
    #[should_panic = "ALPN TLV value cannot be empty"]
    fn alpn_tlv_rejects_empty_constructor_value() {
        Alpn::new(Vec::new());
    }
}
