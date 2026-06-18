//! Type-length-value helpers for PROXY protocol v2 headers.

use std::{borrow::Cow, str};

const PP2_TYPE_ALPN: u8 = 0x01; //           done
const PP2_TYPE_AUTHORITY: u8 = 0x02; //      done
const PP2_TYPE_CRC32C: u8 = 0x03; //         done
const PP2_TYPE_NOOP: u8 = 0x04; //           done
const PP2_TYPE_UNIQUE_ID: u8 = 0x05; //      done
const PP2_TYPE_SSL: u8 = 0x20;
const PP2_SUBTYPE_SSL_VERSION: u8 = 0x21;
const PP2_SUBTYPE_SSL_CN: u8 = 0x22;
const PP2_SUBTYPE_SSL_CIPHER: u8 = 0x23;
const PP2_SUBTYPE_SSL_SIG_ALG: u8 = 0x24;
const PP2_SUBTYPE_SSL_KEY_ALG: u8 = 0x25;
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

bitflags::bitflags! {
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SslClientFlags: u8 {
        const PP2_CLIENT_SSL = 0x01;
        const PP2_CLIENT_CERT_CONN = 0x02;
        const PP2_CLIENT_CERT_SESS = 0x04;
    }
}

/// TLS (SSL).
///
/// Very broken atm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ssl {
    /// The client field is made of a bit field indicating which element is present.
    ///
    /// Note, that each of these elements may lead to extra data being appended to
    /// this TLV using a second level of TLV encapsulation. It is thus possible to
    /// find multiple TLV values after this field. The total length of the pp2_tlv_ssl
    /// TLV will reflect this.
    client: SslClientFlags,

    /// The verify field will be zero if the client presented a certificate
    /// and it was successfully verified, and non-zero otherwise.
    verify: bool,

    /// Sub-TLVs.
    tlvs: Vec<SslTlv>,
}

impl Tlv for Ssl {
    const TYPE: u8 = PP2_TYPE_SSL;

    fn try_from_value(_value: &[u8]) -> Option<Self> {
        /// The PP2_CLIENT_SSL flag indicates that the client connected over SSL/TLS. When
        /// this field is present, the US-ASCII string representation of the TLS version is
        /// appended at the end of the field in the TLV format using the type
        /// PP2_SUBTYPE_SSL_VERSION.
        const PP2_CLIENT_SSL: u8 = 0x01;

        /// PP2_CLIENT_CERT_CONN indicates that the client provided a certificate over the
        /// current connection.
        const PP2_CLIENT_CERT_CONN: u8 = 0x02;

        /// PP2_CLIENT_CERT_SESS indicates that the client provided a
        /// certificate at least once over the TLS session this connection belongs to.
        const PP2_CLIENT_CERT_SESS: u8 = 0x04;

        // TODO: finish parsing

        None
    }

    fn value_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&[])
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SslTlv {}

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
