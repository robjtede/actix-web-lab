use std::{borrow::Cow, convert::TryFrom};

use super::{PP2_TYPE_CRC32C, Tlv};

/// The value of the type PP2_TYPE_CRC32C is a 32-bit number storing the CRC32c
/// checksum of the PROXY protocol header.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Crc32c {
    pub(crate) checksum: u32,
}

impl Crc32c {
    /// Returns the checksum value.
    pub const fn checksum(&self) -> u32 {
        self.checksum
    }
}

impl Tlv for Crc32c {
    const TYPE: u8 = PP2_TYPE_CRC32C;

    fn try_from_value(value: &[u8]) -> Option<Self> {
        let checksum_bytes = <[u8; 4]>::try_from(value).ok()?;

        Some(Self {
            checksum: u32::from_be_bytes(checksum_bytes),
        })
    }

    fn value_bytes(&self) -> Cow<'_, [u8]> {
        Cow::Owned(self.checksum.to_be_bytes().to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32c_tlv_round_trip() {
        let crc = Crc32c::try_from_parts(0x03, &[0x08, 0x70, 0x17, 0x7b]).unwrap();

        assert_eq!(
            Crc32c::try_from_parts(0x04, &[0x08, 0x70, 0x17, 0x7b]),
            None
        );
        assert_eq!(crc.checksum(), 141563771);
        assert_eq!(crc.value_bytes(), [0x08, 0x70, 0x17, 0x7b].as_slice());
    }

    #[test]
    fn crc32c_tlv_rejects_invalid_length() {
        assert_eq!(Crc32c::try_from_value(&[0x08, 0x70, 0x17]), None);
        assert_eq!(
            Crc32c::try_from_value(&[0x08, 0x70, 0x17, 0x7b, 0x00]),
            None
        );
    }

    #[test]
    fn crc32c_tlv_default_serializes_as_zero_checksum() {
        let crc = Crc32c::default();

        assert_eq!(crc.checksum(), 0);
        assert_eq!(crc.value_bytes(), [0, 0, 0, 0].as_slice());
    }
}
