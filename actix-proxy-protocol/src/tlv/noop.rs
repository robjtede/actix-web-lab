use std::borrow::Cow;

use super::{PP2_TYPE_NOOP, Tlv};

/// The TLV of this type should be ignored when parsed. The value is zero or more bytes. Can be used
/// for data padding or alignment.
///
/// Note that it can be used to align only by 3 or more bytes because any TLV on-the-wire can not be
/// smaller than that.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Noop {
    value: Vec<u8>,
}

impl Noop {
    /// Constructs a new NOOP TLV.
    pub fn new(value: impl Into<Vec<u8>>) -> Self {
        let value = value.into();

        Self { value }
    }
}

impl Tlv for Noop {
    const TYPE: u8 = PP2_TYPE_NOOP;

    fn try_from_value(value: &[u8]) -> Option<Self> {
        Some(Self {
            value: value.to_owned(),
        })
    }

    fn value_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Borrowed(&self.value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_tlv_round_trip() {
        let noop = Noop::new([0, 1, 2]);

        assert_eq!(noop.value_bytes(), [0, 1, 2].as_slice());
        assert_eq!(Noop::try_from_value(&[0, 1, 2]), Some(noop.clone()));
        assert_eq!(Noop::try_from_parts(0x04, &[0, 1, 2]), Some(noop));
        assert_eq!(Noop::try_from_parts(0x05, &[0, 1, 2]), None);
    }

    #[test]
    fn noop_tlv_parser_accepts_empty_value() {
        let noop = Noop::try_from_value(&[]).unwrap();

        assert_eq!(noop.value_bytes(), b"".as_slice());
    }

    #[test]
    fn noop_tlv_constructor_accepts_empty_value() {
        let noop = Noop::new(Vec::new());

        assert_eq!(noop.value_bytes(), b"".as_slice());
    }
}
