//! PROXY protocol v1 header support.

use std::{
    fmt, io,
    net::{IpAddr, SocketAddr},
    str,
};

use arrayvec::ArrayVec;
use nom::{Err as NomErr, IResult, Needed, error::ErrorKind};
use tokio::io::{AsyncWrite, AsyncWriteExt as _};

pub use crate::{Acceptor, AcceptorService, HeaderPolicy, ProxyProtocolError, ProxyStream};
use crate::{AddressFamily, ParseError};

/// PROXY protocol v1 signature.
pub const SIGNATURE: &str = "PROXY";
/// Maximum serialized PROXY protocol v1 header length.
pub const MAX_HEADER_SIZE: usize = 107;

/// PROXY protocol v1 header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    addresses: Option<SocketAddresses>,
}

/// Socket addresses from a PROXY protocol v1 TCP header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddresses {
    address_family: AddressFamily,
    source: SocketAddr,
    destination: SocketAddr,
}

impl SocketAddresses {
    /// Returns the header address family.
    pub const fn address_family(&self) -> AddressFamily {
        self.address_family
    }

    /// Returns the source socket address.
    pub const fn source(&self) -> SocketAddr {
        self.source
    }

    /// Returns the destination socket address.
    pub const fn destination(&self) -> SocketAddr {
        self.destination
    }
}

impl Header {
    /// Constructs a new PROXY protocol v1 header.
    ///
    /// # Panics
    /// Panics if `af` is not [`AddressFamily::Inet`] or [`AddressFamily::Inet6`], or when the
    /// supplied socket addresses do not match the address family.
    pub fn new(af: AddressFamily, src: SocketAddr, dst: SocketAddr) -> Self {
        assert!(
            matches!(af, AddressFamily::Inet | AddressFamily::Inet6),
            "{af:?} is not supported in PROXY v1"
        );
        assert!(
            matches!(
                (af, src, dst),
                (AddressFamily::Inet, SocketAddr::V4(_), SocketAddr::V4(_))
                    | (AddressFamily::Inet6, SocketAddr::V6(_), SocketAddr::V6(_))
            ),
            "socket addresses must match the PROXY v1 address family"
        );

        Self {
            addresses: Some(SocketAddresses {
                address_family: af,
                source: src,
                destination: dst,
            }),
        }
    }

    /// Constructs a new IPv4 PROXY protocol v1 header.
    pub fn new_inet(src: SocketAddr, dst: SocketAddr) -> Self {
        Self::new(AddressFamily::Inet, src, dst)
    }

    /// Constructs a new IPv6 PROXY protocol v1 header.
    pub fn new_inet6(src: SocketAddr, dst: SocketAddr) -> Self {
        Self::new(AddressFamily::Inet6, src, dst)
    }

    /// Constructs an `UNKNOWN` PROXY protocol v1 header.
    pub const fn unknown() -> Self {
        Self { addresses: None }
    }

    /// Returns socket address metadata when this is not an `UNKNOWN` header.
    pub const fn addresses(&self) -> Option<SocketAddresses> {
        self.addresses
    }

    /// Returns the source socket address when available.
    pub fn source_addr(&self) -> Option<SocketAddr> {
        self.addresses.map(|addresses| addresses.source)
    }

    /// Returns the destination socket address when available.
    pub fn destination_addr(&self) -> Option<SocketAddr> {
        self.addresses.map(|addresses| addresses.destination)
    }

    /// Writes this header to an I/O writer.
    pub fn write_to(&self, wrt: &mut impl io::Write) -> io::Result<()> {
        write!(wrt, "{self}")
    }

    /// Writes this header to a Tokio async writer.
    pub async fn write_to_tokio(&self, wrt: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
        let mut buf = ArrayVec::<_, MAX_HEADER_SIZE>::new();
        self.write_to(&mut buf)?;
        wrt.write_all(&buf).await
    }

    /// Attempts to parse a PROXY protocol v1 header from bytes.
    pub fn try_from_bytes(slice: &[u8]) -> IResult<&[u8], Self> {
        let Some(line_end) = slice.windows(2).position(|window| window == b"\r\n") else {
            return Err(NomErr::Incomplete(Needed::Unknown));
        };

        if line_end + 2 > MAX_HEADER_SIZE {
            return Err(nom_error(slice, ErrorKind::TooLarge));
        }

        let line = &slice[..line_end];
        let rest = &slice[line_end + 2..];

        let header = Self::parse_line(line).map_err(|_| nom_error(slice, ErrorKind::Verify))?;

        Ok((rest, header))
    }

    pub(crate) fn parse_line(line: &[u8]) -> Result<Self, ParseError> {
        let line =
            str::from_utf8(line).map_err(|_| ParseError::invalid("v1 header is not UTF-8"))?;

        let mut parts = line.split_ascii_whitespace();

        if parts.next() != Some(SIGNATURE) {
            return Err(ParseError::invalid("missing PROXY v1 signature"));
        }

        match parts.next() {
            Some("UNKNOWN") => Ok(Self::unknown()),
            Some("TCP4") => parse_tcp_header(AddressFamily::Inet, parts),
            Some("TCP6") => parse_tcp_header(AddressFamily::Inet6, parts),
            _ => Err(ParseError::invalid("invalid PROXY v1 address family")),
        }
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Some(addresses) = self.addresses else {
            return f.write_str("PROXY UNKNOWN\r\n");
        };

        write!(
            f,
            "{proto_sig} {af} {src_ip} {dst_ip} {src_port} {dst_port}\r\n",
            proto_sig = SIGNATURE,
            af = addresses.address_family.v1_str(),
            src_ip = addresses.source.ip(),
            dst_ip = addresses.destination.ip(),
            src_port = itoa::Buffer::new().format(addresses.source.port()),
            dst_port = itoa::Buffer::new().format(addresses.destination.port()),
        )
    }
}

fn parse_tcp_header<'a>(
    address_family: AddressFamily,
    mut parts: impl Iterator<Item = &'a str>,
) -> Result<Header, ParseError> {
    let src_ip = parts
        .next()
        .ok_or_else(|| ParseError::invalid("missing source IP address"))?
        .parse::<IpAddr>()
        .map_err(|_| ParseError::invalid("invalid source IP address"))?;

    let dst_ip = parts
        .next()
        .ok_or_else(|| ParseError::invalid("missing destination IP address"))?
        .parse::<IpAddr>()
        .map_err(|_| ParseError::invalid("invalid destination IP address"))?;

    let src_port = parts
        .next()
        .ok_or_else(|| ParseError::invalid("missing source port"))?
        .parse::<u16>()
        .map_err(|_| ParseError::invalid("invalid source port"))?;

    let dst_port = parts
        .next()
        .ok_or_else(|| ParseError::invalid("missing destination port"))?
        .parse::<u16>()
        .map_err(|_| ParseError::invalid("invalid destination port"))?;

    if parts.next().is_some() {
        return Err(ParseError::invalid("too many PROXY v1 header fields"));
    }

    let (source, destination) = match (address_family, src_ip, dst_ip) {
        (AddressFamily::Inet, IpAddr::V4(src_ip), IpAddr::V4(dst_ip)) => (
            SocketAddr::from((src_ip, src_port)),
            SocketAddr::from((dst_ip, dst_port)),
        ),
        (AddressFamily::Inet6, IpAddr::V6(src_ip), IpAddr::V6(dst_ip)) => (
            SocketAddr::from((src_ip, src_port)),
            SocketAddr::from((dst_ip, dst_port)),
        ),
        _ => {
            return Err(ParseError::invalid(
                "address family does not match IP version",
            ));
        }
    };

    Ok(Header::new(address_family, source, destination))
}

fn nom_error(input: &[u8], kind: ErrorKind) -> NomErr<nom::error::Error<&[u8]>> {
    NomErr::Error(nom::error::Error::new(input, kind))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_v1_ipv4() {
        let (rest, header) =
            Header::try_from_bytes(b"PROXY TCP4 192.0.2.1 198.51.100.2 12345 443\r\nGET /")
                .unwrap();

        assert_eq!(rest, b"GET /");
        assert_eq!(
            header.source_addr().unwrap(),
            SocketAddr::from(([192, 0, 2, 1], 12345))
        );
        assert_eq!(
            header.destination_addr().unwrap(),
            SocketAddr::from(([198, 51, 100, 2], 443))
        );
    }

    #[test]
    fn parse_v1_ipv6() {
        let (rest, header) =
            Header::try_from_bytes(b"PROXY TCP6 2001:db8::1 2001:db8::2 12345 443\r\nremaining")
                .unwrap();

        assert_eq!(rest, b"remaining");
        assert_eq!(
            header.to_string(),
            "PROXY TCP6 2001:db8::1 2001:db8::2 12345 443\r\n"
        );
    }

    #[test]
    fn parse_v1_unknown() {
        let (rest, header) = Header::try_from_bytes(b"PROXY UNKNOWN\r\nhello").unwrap();

        assert_eq!(rest, b"hello");
        assert_eq!(header.source_addr(), None);
        assert_eq!(header.to_string(), "PROXY UNKNOWN\r\n");
    }
}
