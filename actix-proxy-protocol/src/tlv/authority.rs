use std::{borrow::Cow, str};

use super::{PP2_TYPE_AUTHORITY, Tlv};

/// Contains the host name value passed by the client, as an UTF8-encoded string.
/// In case of TLS being used on the client connection, this is the exact copy of
/// the "server_name" extension as defined by RFC 3546, section 3.1, often
/// referred to as "SNI". There are probably other situations where an authority
/// can be mentioned on a connection without TLS being involved at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Authority {
    authority: String,
}

impl Authority {
    /// Constructs a new authority TLV from a UTF-8 string.
    ///
    /// # Panics
    /// Panics if `authority` is an empty string.
    pub fn new(authority: impl Into<String>) -> Self {
        let authority = authority.into();

        assert!(!authority.is_empty(), "Authority TLV value cannot be empty");

        Self { authority }
    }

    /// Returns the authority string.
    pub fn as_str(&self) -> &str {
        &self.authority
    }
}

impl Tlv for Authority {
    const TYPE: u8 = PP2_TYPE_AUTHORITY;

    fn try_from_value(value: &[u8]) -> Option<Self> {
        Some(Self {
            authority: str::from_utf8(value).ok()?.to_owned(),
        })
    }

    fn value_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(self.authority.as_bytes())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authority_tlv_round_trip() {
        let authority = Authority::new("example.com");

        assert_eq!(authority.as_str(), "example.com");
        assert_eq!(authority.value_bytes(), b"example.com".as_slice());
        assert_eq!(
            Authority::try_from_value(b"example.com"),
            Some(authority.clone())
        );
        assert_eq!(
            Authority::try_from_parts(0x02, b"example.com"),
            Some(authority)
        );
        assert_eq!(Authority::try_from_parts(0x01, b"example.com"), None);
    }

    #[test]
    fn authority_tlv_rejects_invalid_utf8_when_decoding() {
        assert_eq!(Authority::try_from_value(&[0xff]), None);
    }

    #[test]
    #[should_panic = "Authority TLV value cannot be empty"]
    fn authority_tlv_rejects_empty_constructor_value() {
        Authority::new("");
    }
}
