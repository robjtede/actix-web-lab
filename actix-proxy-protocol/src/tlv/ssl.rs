use std::{borrow::Cow, convert::TryFrom, str};

use super::Tlv;

const PP2_TYPE_SSL: u8 = 0x20;
const PP2_SUBTYPE_SSL_VERSION: u8 = 0x21;
const PP2_SUBTYPE_SSL_CN: u8 = 0x22;
const PP2_SUBTYPE_SSL_CIPHER: u8 = 0x23;
const PP2_SUBTYPE_SSL_SIG_ALG: u8 = 0x24;
const PP2_SUBTYPE_SSL_KEY_ALG: u8 = 0x25;

bitflags::bitflags! {
    /// SSL/TLS client flags carried by an SSL TLV.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct SslClientFlags: u8 {
        /// Client connected over SSL/TLS.
        const SSL = 0x01;

        /// Client provided a certificate on the current connection.
        const CERT_CONN = 0x02;

        /// Client provided a certificate at least once over this TLS session.
        const CERT_SESS = 0x04;
    }
}

/// TLS (SSL).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ssl {
    client: SslClientFlags,
    verify: u32,
    tlvs: Vec<SslSubTlv>,
}

impl Ssl {
    /// Constructs a new SSL TLV.
    pub fn new(client: SslClientFlags, verify: u32) -> Self {
        Self {
            client,
            verify,
            tlvs: Vec::new(),
        }
    }

    /// Returns the SSL client flags.
    pub const fn client(&self) -> SslClientFlags {
        self.client
    }

    /// Returns the certificate verification status.
    ///
    /// A value of zero means the client certificate was presented and verified successfully.
    pub const fn verify(&self) -> u32 {
        self.verify
    }

    /// Returns nested SSL TLVs.
    pub fn tlvs(&self) -> &[SslSubTlv] {
        &self.tlvs
    }

    /// Adds a nested SSL TLV.
    pub fn add_tlv(&mut self, tlv: SslSubTlv) {
        self.tlvs.push(tlv);
    }
}

impl Tlv for Ssl {
    const TYPE: u8 = PP2_TYPE_SSL;

    fn try_from_value(value: &[u8]) -> Option<Self> {
        if value.len() < 5 {
            return None;
        }

        let client = SslClientFlags::from_bits(value[0])?;
        let verify = u32::from_be_bytes(<[u8; 4]>::try_from(&value[1..5]).ok()?);
        let tlvs = parse_ssl_sub_tlvs(&value[5..])?;

        Some(Self {
            client,
            verify,
            tlvs,
        })
    }

    fn value_bytes(&self) -> Cow<'_, [u8]> {
        let mut bytes = Vec::with_capacity(5);

        bytes.push(self.client.bits());
        bytes.extend_from_slice(&self.verify.to_be_bytes());

        for tlv in &self.tlvs {
            bytes.push(tlv.typ());
            bytes.extend_from_slice(&(tlv.value().len() as u16).to_be_bytes());
            bytes.extend_from_slice(&tlv.value());
        }

        Cow::Owned(bytes)
    }
}

/// Nested SSL TLV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SslSubTlv {
    /// US-ASCII string representation of the TLS version.
    Version(String),

    /// Client certificate common name.
    CommonName(String),

    /// Negotiated cipher.
    Cipher(String),

    /// Signature algorithm.
    SignatureAlgorithm(String),

    /// Key algorithm.
    KeyAlgorithm(String),

    /// Unknown nested TLV.
    Other(u8, Vec<u8>),
}

impl SslSubTlv {
    /// Constructs a TLS version nested TLV.
    pub fn version(value: impl Into<String>) -> Self {
        Self::Version(value.into())
    }

    /// Constructs a client certificate common-name nested TLV.
    pub fn common_name(value: impl Into<String>) -> Self {
        Self::CommonName(value.into())
    }

    /// Constructs a cipher nested TLV.
    pub fn cipher(value: impl Into<String>) -> Self {
        Self::Cipher(value.into())
    }

    /// Constructs a signature algorithm nested TLV.
    pub fn signature_algorithm(value: impl Into<String>) -> Self {
        Self::SignatureAlgorithm(value.into())
    }

    /// Constructs a key algorithm nested TLV.
    pub fn key_algorithm(value: impl Into<String>) -> Self {
        Self::KeyAlgorithm(value.into())
    }

    fn typ(&self) -> u8 {
        match self {
            Self::Version(_) => PP2_SUBTYPE_SSL_VERSION,
            Self::CommonName(_) => PP2_SUBTYPE_SSL_CN,
            Self::Cipher(_) => PP2_SUBTYPE_SSL_CIPHER,
            Self::SignatureAlgorithm(_) => PP2_SUBTYPE_SSL_SIG_ALG,
            Self::KeyAlgorithm(_) => PP2_SUBTYPE_SSL_KEY_ALG,
            Self::Other(typ, _) => *typ,
        }
    }

    fn value(&self) -> Cow<'_, [u8]> {
        match self {
            Self::Version(value)
            | Self::CommonName(value)
            | Self::Cipher(value)
            | Self::SignatureAlgorithm(value)
            | Self::KeyAlgorithm(value) => Cow::Borrowed(value.as_bytes()),
            Self::Other(_, value) => Cow::Borrowed(value),
        }
    }
}

fn parse_ssl_sub_tlvs(mut value: &[u8]) -> Option<Vec<SslSubTlv>> {
    let mut tlvs = Vec::new();

    while !value.is_empty() {
        if value.len() < 3 {
            return None;
        }

        let typ = value[0];
        let len = u16::from_be_bytes([value[1], value[2]]) as usize;
        value = &value[3..];

        if value.len() < len {
            return None;
        }

        let tlv_value = &value[..len];
        let tlv = match typ {
            PP2_SUBTYPE_SSL_VERSION => {
                SslSubTlv::Version(str::from_utf8(tlv_value).ok()?.to_owned())
            }
            PP2_SUBTYPE_SSL_CN => SslSubTlv::CommonName(str::from_utf8(tlv_value).ok()?.to_owned()),
            PP2_SUBTYPE_SSL_CIPHER => SslSubTlv::Cipher(str::from_utf8(tlv_value).ok()?.to_owned()),
            PP2_SUBTYPE_SSL_SIG_ALG => {
                SslSubTlv::SignatureAlgorithm(str::from_utf8(tlv_value).ok()?.to_owned())
            }
            PP2_SUBTYPE_SSL_KEY_ALG => {
                SslSubTlv::KeyAlgorithm(str::from_utf8(tlv_value).ok()?.to_owned())
            }
            typ => SslSubTlv::Other(typ, tlv_value.to_owned()),
        };

        tlvs.push(tlv);
        value = &value[len..];
    }

    Some(tlvs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssl_tlv_round_trip() {
        let mut ssl = Ssl::new(SslClientFlags::SSL | SslClientFlags::CERT_CONN, 0);
        ssl.add_tlv(SslSubTlv::version("TLSv1.3"));
        ssl.add_tlv(SslSubTlv::common_name("client.example"));
        ssl.add_tlv(SslSubTlv::cipher("TLS_AES_128_GCM_SHA256"));
        ssl.add_tlv(SslSubTlv::signature_algorithm("rsa_pss_rsae_sha256"));
        ssl.add_tlv(SslSubTlv::key_algorithm("RSA"));
        ssl.add_tlv(SslSubTlv::Other(0xff, b"opaque".to_vec()));

        let bytes = ssl.value_bytes();
        let parsed = Ssl::try_from_value(&bytes).unwrap();

        assert_eq!(parsed, ssl);
        assert_eq!(
            parsed.client(),
            SslClientFlags::SSL | SslClientFlags::CERT_CONN
        );
        assert_eq!(parsed.verify(), 0);
        assert_eq!(parsed.tlvs().len(), 6);
        assert_eq!(Ssl::try_from_parts(0x20, &bytes), Some(parsed));
        assert_eq!(Ssl::try_from_parts(0x21, &bytes), None);
    }

    #[test]
    fn ssl_tlv_rejects_malformed_top_level_value() {
        assert_eq!(Ssl::try_from_value(&[SslClientFlags::SSL.bits()]), None);
        assert_eq!(Ssl::try_from_value(&[0xff, 0, 0, 0, 0]), None);
    }

    #[test]
    fn ssl_tlv_rejects_truncated_nested_tlv_header() {
        let value = [
            SslClientFlags::SSL.bits(),
            0,
            0,
            0,
            0,
            PP2_SUBTYPE_SSL_VERSION,
            0,
        ];

        assert_eq!(Ssl::try_from_value(&value), None);
    }

    #[test]
    fn ssl_tlv_rejects_truncated_nested_tlv_value() {
        let value = [
            SslClientFlags::SSL.bits(),
            0,
            0,
            0,
            0,
            PP2_SUBTYPE_SSL_VERSION,
            0,
            4,
            b'T',
        ];

        assert_eq!(Ssl::try_from_value(&value), None);
    }

    #[test]
    fn ssl_tlv_rejects_invalid_utf8_nested_value() {
        let value = [
            SslClientFlags::SSL.bits(),
            0,
            0,
            0,
            0,
            PP2_SUBTYPE_SSL_VERSION,
            0,
            1,
            0xff,
        ];

        assert_eq!(Ssl::try_from_value(&value), None);
    }
}
