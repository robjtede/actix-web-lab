//! Type-length-value helpers for PROXY protocol v2 headers.

use std::{borrow::Cow, str};

const PP2_TYPE_ALPN: u8 = 0x01; //           done
const PP2_TYPE_AUTHORITY: u8 = 0x02; //      done
const PP2_TYPE_CRC32C: u8 = 0x03; //         done
const PP2_TYPE_NOOP: u8 = 0x04; //           done
const PP2_TYPE_UNIQUE_ID: u8 = 0x05; //      done
const PP2_TYPE_NETNS: u8 = 0x30;

/// PROXY protocol v2 type-length-value extension.
pub trait Tlv: Sized {
    /// Numeric TLV type identifier.
    const TYPE: u8;

    /// Attempts to decode a TLV value payload.
    fn try_from_value(value: &[u8]) -> Option<Self>;

    /// Serializes this TLV's value payload.
    fn value_bytes(&self) -> Cow<'_, [u8]>;

    /// Attempts to decode a TLV from its type identifier and value payload.
    fn try_from_parts(typ: u8, value: &[u8]) -> Option<Self> {
        if typ != Self::TYPE {
            return None;
        }

        Self::try_from_value(value)
    }
}

mod alpn;
pub use self::alpn::Alpn;

mod authority;
pub use self::authority::Authority;

mod crc32c;
pub use self::crc32c::Crc32c;

mod noop;
pub use self::noop::Noop;

mod unique_id;
pub use self::unique_id::UniqueId;

mod ssl;
pub use self::ssl::{Ssl, SslClientFlags, SslSubTlv};

mod netns;
pub use self::netns::NetNamespace;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tlv_as_crc32c() {
        // noop
        assert_eq!(Crc32c::try_from_parts(0x04, &[0x00]), None);

        assert_eq!(
            Crc32c::try_from_parts(0x03, &[0x08, 0x70, 0x17, 0x7b]),
            Some(Crc32c {
                checksum: 141563771
            })
        );
    }
}
