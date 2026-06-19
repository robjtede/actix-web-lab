//! PROXY protocol v2 header parsing and serialization.

use std::{
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
};

use smallvec::{SmallVec, ToSmallVec as _};
#[cfg(feature = "tokio")]
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

use crate::{
    AddressFamily, Command, ParseError, TransportProtocol, Version,
    tlv::{Crc32c, Tlv},
};

/// PROXY protocol v2 signature.
pub const SIGNATURE: [u8; 12] = [
    0x0D, 0x0A, 0x0D, 0x0A, 0x00, 0x0D, 0x0A, 0x51, 0x55, 0x49, 0x54, 0x0A,
];

/// UNIX socket address byte length used by PROXY protocol v2.
pub const UNIX_ADDR_LEN: usize = 108;

type RawTlvs = SmallVec<[(u8, SmallVec<[u8; 16]>); 4]>;

/// Address information carried by a PROXY protocol v2 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Addresses {
    /// No address information is carried.
    Unspecified,

    /// TCP or UDP IP socket addresses.
    Inet {
        /// Source socket address.
        source: SocketAddr,

        /// Destination socket address.
        destination: SocketAddr,
    },

    /// UNIX socket addresses.
    Unix {
        /// Source socket path bytes.
        source: Box<[u8; UNIX_ADDR_LEN]>,

        /// Destination socket path bytes.
        destination: Box<[u8; UNIX_ADDR_LEN]>,
    },
}

impl Addresses {
    fn address_family(&self) -> AddressFamily {
        match self {
            Self::Unspecified => AddressFamily::Unspecified,
            Self::Inet { source, .. } if source.is_ipv4() => AddressFamily::Inet,
            Self::Inet { .. } => AddressFamily::Inet6,
            Self::Unix { .. } => AddressFamily::Unix,
        }
    }

    fn encoded_len(&self) -> usize {
        match self {
            Self::Unspecified => 0,
            Self::Inet { source, .. } if source.is_ipv4() => 12,
            Self::Inet { .. } => 36,
            Self::Unix { .. } => UNIX_ADDR_LEN * 2,
        }
    }

    fn source_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::Inet { source, .. } => Some(*source),
            Self::Unspecified | Self::Unix { .. } => None,
        }
    }

    fn destination_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::Inet { destination, .. } => Some(*destination),
            Self::Unspecified | Self::Unix { .. } => None,
        }
    }
}

/// PROXY protocol v2 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    command: Command,
    transport_protocol: TransportProtocol,
    addresses: Addresses,
    tlvs: RawTlvs,
}

impl Header {
    /// Constructs a new PROXY protocol v2 header.
    ///
    /// # Panics
    /// Panics if `address_family` does not match `src` and `dst`.
    pub fn new(
        command: Command,
        transport_protocol: TransportProtocol,
        address_family: AddressFamily,
        src: impl Into<SocketAddr>,
        dst: impl Into<SocketAddr>,
    ) -> Self {
        let source = src.into();
        let destination = dst.into();

        assert!(
            matches!(
                (address_family, source, destination),
                (AddressFamily::Inet, SocketAddr::V4(_), SocketAddr::V4(_))
                    | (AddressFamily::Inet6, SocketAddr::V6(_), SocketAddr::V6(_))
            ),
            "socket addresses must match the PROXY v2 address family"
        );

        Self {
            command,
            transport_protocol,
            addresses: Addresses::Inet {
                source,
                destination,
            },
            tlvs: SmallVec::new(),
        }
    }

    /// Constructs a new header without address information.
    pub fn new_unspecified(command: Command) -> Self {
        Self {
            command,
            transport_protocol: TransportProtocol::Unspecified,
            addresses: Addresses::Unspecified,
            tlvs: SmallVec::new(),
        }
    }

    /// Constructs a new UNIX socket header.
    pub fn new_unix(
        command: Command,
        transport_protocol: TransportProtocol,
        source: [u8; UNIX_ADDR_LEN],
        destination: [u8; UNIX_ADDR_LEN],
    ) -> Self {
        Self {
            command,
            transport_protocol,
            addresses: Addresses::Unix {
                source: Box::new(source),
                destination: Box::new(destination),
            },
            tlvs: SmallVec::new(),
        }
    }

    /// Constructs a new TCP/IPv4 PROXY command header.
    pub fn new_tcp_ipv4_proxy(src: impl Into<SocketAddr>, dst: impl Into<SocketAddr>) -> Self {
        Self::new(
            Command::Proxy,
            TransportProtocol::Stream,
            AddressFamily::Inet,
            src,
            dst,
        )
    }

    /// Constructs a new TCP/IPv6 PROXY command header.
    pub fn new_tcp_ipv6_proxy(src: impl Into<SocketAddr>, dst: impl Into<SocketAddr>) -> Self {
        Self::new(
            Command::Proxy,
            TransportProtocol::Stream,
            AddressFamily::Inet6,
            src,
            dst,
        )
    }

    /// Returns the header command.
    pub const fn command(&self) -> Command {
        self.command
    }

    /// Returns the transport protocol.
    pub const fn transport_protocol(&self) -> TransportProtocol {
        self.transport_protocol
    }

    /// Returns the address family.
    pub fn address_family(&self) -> AddressFamily {
        self.addresses.address_family()
    }

    /// Returns the encoded address information.
    pub const fn addresses(&self) -> &Addresses {
        &self.addresses
    }

    /// Returns the source socket address for IP headers.
    pub fn source_addr(&self) -> Option<SocketAddr> {
        self.addresses.source_addr()
    }

    /// Returns the destination socket address for IP headers.
    pub fn destination_addr(&self) -> Option<SocketAddr> {
        self.addresses.destination_addr()
    }

    /// Returns raw TLVs as `(type, value)` pairs.
    pub fn tlvs(&self) -> impl Iterator<Item = (u8, &[u8])> {
        self.tlvs
            .iter()
            .map(|(typ, value)| (*typ, value.as_slice()))
    }

    /// Returns the first TLV that decodes as `T`.
    pub fn typed_tlv<T: Tlv>(&self) -> Option<T> {
        self.tlvs
            .iter()
            .find_map(|(typ, value)| T::try_from_parts(*typ, value))
    }

    /// Adds a raw TLV entry.
    pub fn add_tlv(&mut self, typ: u8, value: impl AsRef<[u8]>) {
        self.tlvs.push((typ, SmallVec::from_slice(value.as_ref())));
    }

    /// Adds a typed TLV entry.
    pub fn add_typed_tlv<T: Tlv>(&mut self, tlv: T) {
        self.add_tlv(T::TYPE, tlv.value_bytes());
    }

    fn v2_len(&self) -> u16 {
        (self.addresses.encoded_len()
            + self
                .tlvs
                .iter()
                .map(|(_, value)| 1 + 2 + value.len())
                .sum::<usize>())
        .try_into()
        .expect("PROXY v2 header length exceeds u16::MAX")
    }

    /// Writes this header to an I/O writer.
    pub fn write_to(&self, wrt: &mut impl io::Write) -> io::Result<()> {
        // PROXY v2 signature.
        wrt.write_all(&SIGNATURE)?;

        // Version and command.
        wrt.write_all(&[Version::V2.v2_hi() | self.command.v2_lo()])?;

        // Address family and transport protocol.
        wrt.write_all(&[self.address_family().v2_hi() | self.transport_protocol.v2_lo()])?;

        // Variable header length.
        wrt.write_all(&self.v2_len().to_be_bytes())?;

        match &self.addresses {
            Addresses::Unspecified => {}
            Addresses::Inet {
                source,
                destination,
            } => {
                // L3 IP addresses.
                write_ip_bytes_to(wrt, source.ip())?;
                write_ip_bytes_to(wrt, destination.ip())?;

                // L4 ports.
                wrt.write_all(&source.port().to_be_bytes())?;
                wrt.write_all(&destination.port().to_be_bytes())?;
            }
            Addresses::Unix {
                source,
                destination,
            } => {
                wrt.write_all(source.as_slice())?;
                wrt.write_all(destination.as_slice())?;
            }
        }

        // TLVs.
        for (typ, value) in &self.tlvs {
            wrt.write_all(&[*typ])?;
            wrt.write_all(&(value.len() as u16).to_be_bytes())?;
            wrt.write_all(value)?;
        }

        Ok(())
    }

    /// Writes this header to a Tokio async writer.
    #[cfg(feature = "tokio")]
    pub async fn write_to_tokio(&self, wrt: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
        let buf = self.to_vec();
        wrt.write_all(&buf).await
    }

    /// Serializes this header to bytes.
    pub fn to_vec(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(16 + self.v2_len() as usize);
        self.write_to(&mut buf).unwrap();
        buf
    }

    /// Attempts to parse a PROXY protocol v2 header from bytes.
    pub fn try_from_bytes(slice: &[u8]) -> Result<(&[u8], Self), ParseError> {
        parse(slice)
    }

    /// Returns true when this header contains a TLV with type `T`.
    pub fn has_tlv<T: Tlv>(&self) -> bool {
        self.tlvs.iter().any(|&(typ, _)| typ == T::TYPE)
    }

    /// Calculates and adds a crc32c TLV to the PROXY header.
    ///
    /// Uses method defined in spec.
    ///
    /// If this is not called last thing it will be wrong.
    pub fn add_crc32c_checksum(&mut self) {
        // Don't add a checksum if it is already set.
        if self.has_tlv::<Crc32c>() {
            return;
        }

        // When the checksum is supported by the sender after constructing the header
        // the sender MUST:
        // - initialize the checksum field to `0`s.
        // - calculate the CRC32c checksum of the PROXY header as described in RFC4960,
        //   Appendix B.
        // - put the resultant value into the checksum field, and leave the rest of
        //   the bits unchanged.

        // Add zeroed checksum field to TLVs.
        self.add_typed_tlv(Crc32c::default());

        // Write PROXY header to buffer.
        let mut buf = Vec::new();
        self.write_to(&mut buf).unwrap();

        // Calculate CRC on buffer and update CRC TLV.
        let crc_calc = crc32fast::hash(&buf);
        self.tlvs.last_mut().unwrap().1 = crc_calc.to_be_bytes().to_smallvec();
    }

    /// Validates the CRC32c TLV, returning `None` when no CRC32c TLV is present.
    pub fn validate_crc32c_tlv(&self) -> Option<bool> {
        // Extract CRC32c TLV or exit early if none is present.
        let crc_sent = self.typed_tlv::<Crc32c>()?;

        // If the checksum is provided as part of the PROXY header and the checksum
        // functionality is supported by the receiver, the receiver MUST:
        // - store the received CRC32c checksum value aside.
        // - replace the 32 bits of the checksum field in the received PROXY header with
        //   all `0`s and calculate a CRC32c checksum value of the whole PROXY header.
        // - verify that the calculated CRC32c checksum is the same as the received
        //   CRC32c checksum. If it is not, the receiver MUST treat the TCP connection
        //   providing the header as invalid.
        // The default procedure for handling an invalid TCP connection is to abort it.
        let mut this = self.clone();
        for (typ, value) in this.tlvs.iter_mut() {
            if Crc32c::try_from_parts(*typ, value).is_some() {
                value.fill(0);
            }
        }

        let mut buf = Vec::new();
        this.write_to(&mut buf).unwrap();
        let crc_calc = crc32fast::hash(&buf);

        Some(crc_sent.checksum == crc_calc)
    }
}

fn parse(slice: &[u8]) -> Result<(&[u8], Header), ParseError> {
    if slice.len() < 16 {
        return Err(parse_err("incomplete PROXY v2 fixed header"));
    }

    if slice[..12] != SIGNATURE {
        return Err(parse_err("missing PROXY v2 signature"));
    }

    let ver_cmd = slice[12];
    if ver_cmd >> 4 != 0x2 {
        return Err(parse_err("invalid PROXY v2 version"));
    }

    let command =
        Command::from_v2_lo(ver_cmd & 0x0f).ok_or_else(|| parse_err("invalid command"))?;

    let fam_proto = slice[13];
    let address_family = AddressFamily::from_v2_hi(fam_proto >> 4)
        .ok_or_else(|| parse_err("invalid address family"))?;
    let transport_protocol = TransportProtocol::from_v2_lo(fam_proto & 0x0f)
        .ok_or_else(|| parse_err("invalid transport protocol"))?;

    let len = u16::from_be_bytes([slice[14], slice[15]]) as usize;
    let end = 16 + len;
    if slice.len() < end {
        return Err(parse_err("incomplete PROXY v2 variable header"));
    }

    let payload = &slice[16..end];
    let rest = &slice[end..];
    let addr_len = address_len(address_family, transport_protocol)?;

    if payload.len() < addr_len {
        return Err(parse_err("PROXY v2 address block is shorter than required"));
    }

    let addresses = parse_addresses(address_family, transport_protocol, &payload[..addr_len])?;
    let tlvs = parse_tlvs(&payload[addr_len..])?;

    Ok((
        rest,
        Header {
            command,
            transport_protocol,
            addresses,
            tlvs,
        },
    ))
}

fn address_len(
    address_family: AddressFamily,
    transport_protocol: TransportProtocol,
) -> Result<usize, ParseError> {
    match (address_family, transport_protocol) {
        (AddressFamily::Unspecified, TransportProtocol::Unspecified) => Ok(0),
        (AddressFamily::Inet, TransportProtocol::Stream | TransportProtocol::Datagram) => Ok(12),
        (AddressFamily::Inet6, TransportProtocol::Stream | TransportProtocol::Datagram) => Ok(36),
        (AddressFamily::Unix, TransportProtocol::Stream | TransportProtocol::Datagram) => {
            Ok(UNIX_ADDR_LEN * 2)
        }
        _ => Err(parse_err(
            "invalid address family and transport protocol combination",
        )),
    }
}

fn parse_addresses(
    address_family: AddressFamily,
    transport_protocol: TransportProtocol,
    payload: &[u8],
) -> Result<Addresses, ParseError> {
    match (address_family, transport_protocol) {
        (AddressFamily::Unspecified, TransportProtocol::Unspecified) => Ok(Addresses::Unspecified),
        (AddressFamily::Inet, TransportProtocol::Stream | TransportProtocol::Datagram) => {
            Ok(Addresses::Inet {
                source: SocketAddr::from((
                    Ipv4Addr::new(payload[0], payload[1], payload[2], payload[3]),
                    u16::from_be_bytes([payload[8], payload[9]]),
                )),
                destination: SocketAddr::from((
                    Ipv4Addr::new(payload[4], payload[5], payload[6], payload[7]),
                    u16::from_be_bytes([payload[10], payload[11]]),
                )),
            })
        }
        (AddressFamily::Inet6, TransportProtocol::Stream | TransportProtocol::Datagram) => {
            let source_ip =
                Ipv6Addr::from(<[u8; 16]>::try_from(&payload[..16]).expect("16-byte IPv6 source"));
            let destination_ip = Ipv6Addr::from(
                <[u8; 16]>::try_from(&payload[16..32]).expect("16-byte IPv6 destination"),
            );

            Ok(Addresses::Inet {
                source: SocketAddr::from((
                    source_ip,
                    u16::from_be_bytes([payload[32], payload[33]]),
                )),
                destination: SocketAddr::from((
                    destination_ip,
                    u16::from_be_bytes([payload[34], payload[35]]),
                )),
            })
        }
        (AddressFamily::Unix, TransportProtocol::Stream | TransportProtocol::Datagram) => {
            let source = Box::new(
                <[u8; UNIX_ADDR_LEN]>::try_from(&payload[..UNIX_ADDR_LEN])
                    .expect("108-byte UNIX source"),
            );
            let destination = Box::new(
                <[u8; UNIX_ADDR_LEN]>::try_from(&payload[UNIX_ADDR_LEN..])
                    .expect("108-byte UNIX destination"),
            );

            Ok(Addresses::Unix {
                source,
                destination,
            })
        }
        _ => Err(parse_err(
            "invalid address family and transport protocol combination",
        )),
    }
}

fn parse_tlvs(mut payload: &[u8]) -> Result<RawTlvs, ParseError> {
    let mut tlvs = SmallVec::new();

    while !payload.is_empty() {
        if payload.len() < 3 {
            return Err(parse_err("incomplete PROXY v2 TLV header"));
        }

        let typ = payload[0];
        let len = u16::from_be_bytes([payload[1], payload[2]]) as usize;
        payload = &payload[3..];

        if payload.len() < len {
            return Err(parse_err("incomplete PROXY v2 TLV value"));
        }

        tlvs.push((typ, SmallVec::from_slice(&payload[..len])));
        payload = &payload[len..];
    }

    Ok(tlvs)
}

fn write_ip_bytes_to(wrt: &mut impl io::Write, ip: IpAddr) -> io::Result<()> {
    match ip {
        IpAddr::V4(ip) => wrt.write_all(&ip.octets()),
        IpAddr::V6(ip) => wrt.write_all(&ip.octets()),
    }
}

fn parse_err(message: &'static str) -> ParseError {
    ParseError::invalid(message)
}

#[cfg(test)]
mod tests {
    use std::net::Ipv6Addr;

    use const_str::hex;
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn write_v2_no_tlvs() {
        let mut exp = Vec::new();
        exp.extend_from_slice(&SIGNATURE);
        exp.extend_from_slice(&[0x21, 0x11]);
        exp.extend_from_slice(&[0x00, 0x0C]);
        exp.extend_from_slice(&[127, 0, 0, 1, 127, 0, 0, 2]);
        exp.extend_from_slice(&[0x04, 0xd2, 0x00, 80]);

        let header = Header::new(
            Command::Proxy,
            TransportProtocol::Stream,
            AddressFamily::Inet,
            SocketAddr::from(([127, 0, 0, 1], 1234)),
            SocketAddr::from(([127, 0, 0, 2], 80)),
        );

        assert_eq!(header.v2_len(), 12);
        assert_eq!(header.to_vec(), exp);
    }

    #[test]
    fn parse_v2_no_tlvs() {
        let mut bytes = Header::new_tcp_ipv4_proxy(
            SocketAddr::from(([127, 0, 0, 1], 1234)),
            SocketAddr::from(([127, 0, 0, 2], 80)),
        )
        .to_vec();
        bytes.extend_from_slice(b"GET /");

        let (rest, header) = Header::try_from_bytes(&bytes).unwrap();

        assert_eq!(rest, b"GET /");
        assert_eq!(header.command(), Command::Proxy);
        assert_eq!(header.transport_protocol(), TransportProtocol::Stream);
        assert_eq!(header.address_family(), AddressFamily::Inet);
        assert_eq!(
            header.source_addr().unwrap(),
            SocketAddr::from(([127, 0, 0, 1], 1234))
        );
        assert_eq!(
            header.destination_addr().unwrap(),
            SocketAddr::from(([127, 0, 0, 2], 80))
        );
    }

    #[test]
    fn write_v2_ipv6_tlv_noop() {
        let mut exp = Vec::new();
        exp.extend_from_slice(&SIGNATURE);
        exp.extend_from_slice(&[0x20, 0x21]);
        exp.extend_from_slice(&[0x00, 0x28]);
        exp.extend_from_slice(&hex!("00000000000000000000000000000001"));
        exp.extend_from_slice(&hex!("000102030405060708090A0B0C0D0E0F"));
        exp.extend_from_slice(&[0x00, 80, 0xff, 0xff]);
        exp.extend_from_slice(&[0x04, 0x00, 0x01, 0x00]);

        let mut header = Header::new(
            Command::Local,
            TransportProtocol::Stream,
            AddressFamily::Inet6,
            SocketAddr::from((Ipv6Addr::LOCALHOST, 80)),
            SocketAddr::from((
                Ipv6Addr::from([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15]),
                65535,
            )),
        );

        header.add_tlv(0x04, [0]);

        assert_eq!(header.v2_len(), 36 + 4);
        assert_eq!(header.to_vec(), exp);
    }

    #[test]
    fn parse_v2_unspecified_with_tlv() {
        let mut header = Header::new_unspecified(Command::Local);
        header.add_tlv(0x05, b"abc123");

        let (_rest, parsed) = Header::try_from_bytes(&header.to_vec()).unwrap();

        assert_eq!(parsed.address_family(), AddressFamily::Unspecified);
        assert_eq!(parsed.source_addr(), None);
        assert_eq!(
            parsed.tlvs().collect::<Vec<_>>(),
            vec![(0x05, b"abc123".as_slice())]
        );
    }

    #[test]
    fn write_v2_tlv_crc32c() {
        let mut exp = Vec::new();
        exp.extend_from_slice(&SIGNATURE);
        exp.extend_from_slice(&[0x21, 0x11]);
        exp.extend_from_slice(&[0x00, 0x13]);
        exp.extend_from_slice(&[127, 0, 0, 1, 127, 0, 0, 1]);
        exp.extend_from_slice(&[0x00, 80, 0x00, 80]);
        exp.extend_from_slice(&[0x03, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);

        // Correct checksum calculated manually.
        assert_eq!(
            crc32fast::hash(&exp),
            u32::from_be_bytes([0x08, 0x70, 0x17, 0x7b]),
        );

        // Re-assign actual checksum to last 4 bytes of expected byte array.
        exp[31..35].copy_from_slice(&[0x08, 0x70, 0x17, 0x7b]);

        let mut header = Header::new(
            Command::Proxy,
            TransportProtocol::Stream,
            AddressFamily::Inet,
            SocketAddr::from(([127, 0, 0, 1], 80)),
            SocketAddr::from(([127, 0, 0, 1], 80)),
        );

        assert!(
            header.validate_crc32c_tlv().is_none(),
            "header doesn't have CRC TLV added yet"
        );

        // Add CRC32c TLV to header.
        header.add_crc32c_checksum();

        assert_eq!(header.v2_len(), 12 + 7);
        assert_eq!(header.to_vec(), exp);

        // Struct can self-validate checksum.
        assert_eq!(header.validate_crc32c_tlv().unwrap(), true);

        // Mangle CRC32c TLV and assert that validate now fails.
        *header.tlvs.last_mut().unwrap().1.last_mut().unwrap() = 0x00;
        assert_eq!(header.validate_crc32c_tlv().unwrap(), false);
    }
}
