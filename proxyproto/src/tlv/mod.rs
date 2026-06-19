//! Type-length-value support for PROXY protocol v2 headers.

mod alpn;
mod authority;
mod crc32c;
mod netns;
mod noop;
mod ssl;
mod unique_id;

use std::borrow::Cow;

pub use self::{
    alpn::Alpn,
    authority::Authority,
    crc32c::Crc32c,
    netns::NetNamespace,
    noop::Noop,
    ssl::{Ssl, SslClientFlags, SslSubTlv},
    unique_id::UniqueId,
};

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
