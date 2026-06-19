//! PROXY protocol v1 header support.

use std::{
    fmt, io,
    net::{IpAddr, SocketAddr},
    str,
};

#[cfg(feature = "tokio")]
use arrayvec::ArrayVec;
#[cfg(feature = "tokio")]
use tokio::io::{AsyncWrite, AsyncWriteExt as _};
use winnow::{
    ascii::{crlf, dec_uint},
    combinator::{alt, preceded, terminated},
    prelude::*,
    stream::Partial,
    token::{take_till, take_until, take_while},
};

use crate::AddressFamily;

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
    #[cfg(feature = "tokio")]
    pub async fn write_to_tokio(&self, wrt: &mut (impl AsyncWrite + Unpin)) -> io::Result<()> {
        let mut buf = ArrayVec::<_, MAX_HEADER_SIZE>::new();
        self.write_to(&mut buf)?;
        wrt.write_all(&buf).await
    }

    /// Attempts to parse a PROXY protocol v1 header from bytes.
    pub fn try_from_bytes(slice: &[u8]) -> ModalResult<(&[u8], Self)> {
        let mut input = Partial::new(slice);
        let header = parse_header(&mut input)?;

        Ok((input.into_inner(), header))
    }
}

fn parse_header(input: &mut Partial<&[u8]>) -> ModalResult<Header> {
    terminated(parse_line, crlf)
        .with_taken()
        .verify(|(_, bytes): &(Header, &[u8])| bytes.len() <= MAX_HEADER_SIZE)
        .map(|(header, _)| header)
        .parse_next(input)
}

fn parse_line(input: &mut Partial<&[u8]>) -> ModalResult<Header> {
    preceded(
        (ascii_whitespace0, SIGNATURE, ascii_whitespace1),
        alt((
            ("UNKNOWN", take_until(0.., "\r\n")).value(Header::unknown()),
            terminated(parse_tcp_header, ascii_whitespace0),
        )),
    )
    .parse_next(input)
}

fn parse_tcp_header(input: &mut Partial<&[u8]>) -> ModalResult<Header> {
    (
        alt((
            "TCP4".value(AddressFamily::Inet),
            "TCP6".value(AddressFamily::Inet6),
        )),
        preceded(ascii_whitespace1, ip_addr),
        preceded(ascii_whitespace1, ip_addr),
        preceded(ascii_whitespace1, dec_uint::<_, u16, _>),
        preceded(ascii_whitespace1, dec_uint::<_, u16, _>),
    )
        .verify_map(|(address_family, src_ip, dst_ip, src_port, dst_port)| {
            let (source, destination) = match (address_family, src_ip, dst_ip) {
                (AddressFamily::Inet, IpAddr::V4(src_ip), IpAddr::V4(dst_ip)) => (
                    SocketAddr::from((src_ip, src_port)),
                    SocketAddr::from((dst_ip, dst_port)),
                ),
                (AddressFamily::Inet6, IpAddr::V6(src_ip), IpAddr::V6(dst_ip)) => (
                    SocketAddr::from((src_ip, src_port)),
                    SocketAddr::from((dst_ip, dst_port)),
                ),
                _ => return None,
            };

            Some(Header::new(address_family, source, destination))
        })
        .parse_next(input)
}

fn ip_addr(input: &mut Partial<&[u8]>) -> ModalResult<IpAddr> {
    take_till(1.., is_field_whitespace)
        .verify_map(|bytes| str::from_utf8(bytes).ok()?.parse().ok())
        .parse_next(input)
}

fn ascii_whitespace0<'a>(input: &mut Partial<&'a [u8]>) -> ModalResult<&'a [u8]> {
    take_while(0.., is_field_whitespace).parse_next(input)
}

fn ascii_whitespace1<'a>(input: &mut Partial<&'a [u8]>) -> ModalResult<&'a [u8]> {
    take_while(1.., is_field_whitespace).parse_next(input)
}

const fn is_field_whitespace(byte: u8) -> bool {
    byte.is_ascii_whitespace() && !matches!(byte, b'\r' | b'\n')
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

#[cfg(test)]
mod tests {
    use winnow::error::ErrMode;

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

    #[test]
    fn parse_v1_streaming_mode() {
        assert!(matches!(
            parse_header(&mut Partial::new(&b"PROXY UNKNOWN\r"[..])),
            Err(ErrMode::Incomplete(_))
        ));

        let mut input = Partial::new(&b"PROXY UNKNOWN\r\npayload"[..]);
        let header = parse_header(&mut input).unwrap();

        assert_eq!(header, Header::unknown());
        assert_eq!(input.into_inner(), b"payload");
    }

    #[test]
    fn parse_v1_returns_plain_remaining_bytes() {
        let (remaining, header) =
            Header::try_from_bytes(b"PROXY UNKNOWN\r\nHTTP/1.1 payload").unwrap();
        let remaining: &[u8] = remaining;

        assert_eq!(header, Header::unknown());
        assert_eq!(remaining, b"HTTP/1.1 payload");
    }

    #[test]
    fn parse_v1_rejects_oversized_header() {
        let mut bytes = b"PROXY UNKNOWN ".to_vec();
        bytes.resize(MAX_HEADER_SIZE - 1, b'x');
        bytes.extend_from_slice(b"\r\n");

        assert!(Header::try_from_bytes(&bytes).is_err());
    }

    #[test]
    fn parse_v1_rejects_source_port_out_of_u16_bounds() {
        assert!(
            Header::try_from_bytes(b"PROXY TCP4 192.0.2.1 198.51.100.2 65536 443\r\n").is_err()
        );
    }

    #[test]
    fn parse_v1_rejects_destination_port_out_of_u16_bounds() {
        assert!(
            Header::try_from_bytes(b"PROXY TCP4 192.0.2.1 198.51.100.2 12345 65536\r\n").is_err()
        );
    }
}
