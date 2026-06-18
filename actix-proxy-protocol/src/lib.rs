//! PROXY protocol utilities for Actix networking.

#![doc(html_logo_url = "https://actix.rs/img/logo.png")]
#![doc(html_favicon_url = "https://actix.rs/favicon.ico")]

use std::{fmt, io, net::SocketAddr};

mod service;
pub mod tlv;
pub mod v1;
pub mod v2;

pub use self::service::{Acceptor, AcceptorService, HeaderPolicy, ProxyProtocolError, ProxyStream};

/// PROXY Protocol Version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Version {
    /// Human-readable header format (Version 1)
    V1,

    /// Binary header format (Version 2)
    V2,
}

impl Version {
    const fn v2_hi(&self) -> u8 {
        (match self {
            Version::V1 => panic!("v1 not supported in PROXY v2"),
            Version::V2 => 0x2,
        }) << 4
    }
}

/// Command
///
/// other values are unassigned and must not be emitted by senders. Receivers
/// must drop connections presenting unexpected values here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// \x0 : LOCAL : the connection was established on purpose by the proxy
    /// without being relayed. The connection endpoints are the sender and the
    /// receiver. Such connections exist when the proxy sends health-checks to the
    /// server. The receiver must accept this connection as valid and must use the
    /// real connection endpoints and discard the protocol block including the
    /// family which is ignored.
    Local,

    /// \x1 : PROXY : the connection was established on behalf of another node,
    /// and reflects the original connection endpoints. The receiver must then use
    /// the information provided in the protocol block to get original the address.
    Proxy,
}

impl Command {
    pub(crate) const fn from_v2_lo(val: u8) -> Option<Self> {
        match val {
            0x0 => Some(Self::Local),
            0x1 => Some(Self::Proxy),
            _ => None,
        }
    }

    const fn v2_lo(&self) -> u8 {
        match self {
            Command::Local => 0x0,
            Command::Proxy => 0x1,
        }
    }
}

/// Address Family.
///
/// maps to the original socket family without necessarily
/// matching the values internally used by the system.
///
/// other values are unspecified and must not be emitted in version 2 of this
/// protocol and must be rejected as invalid by receivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressFamily {
    /// 0x0 : AF_UNSPEC : the connection is forwarded for an unknown, unspecified
    /// or unsupported protocol. The sender should use this family when sending
    /// LOCAL commands or when dealing with unsupported protocol families. The
    /// receiver is free to accept the connection anyway and use the real endpoint
    /// addresses or to reject it. The receiver should ignore address information.
    Unspecified,

    /// 0x1 : AF_INET : the forwarded connection uses the AF_INET address family
    /// (IPv4). The addresses are exactly 4 bytes each in network byte order,
    /// followed by transport protocol information (typically ports).
    Inet,

    /// 0x2 : AF_INET6 : the forwarded connection uses the AF_INET6 address family
    /// (IPv6). The addresses are exactly 16 bytes each in network byte order,
    /// followed by transport protocol information (typically ports).
    Inet6,

    /// 0x3 : AF_UNIX : the forwarded connection uses the AF_UNIX address family
    /// (UNIX). The addresses are exactly 108 bytes each.
    Unix,
}

impl AddressFamily {
    pub(crate) fn v1_str(&self) -> &'static str {
        match self {
            AddressFamily::Inet => "TCP4",
            AddressFamily::Inet6 => "TCP6",
            af => panic!("{:?} is not supported in PROXY v1", af),
        }
    }

    pub(crate) const fn from_v2_hi(val: u8) -> Option<Self> {
        match val {
            0x0 => Some(Self::Unspecified),
            0x1 => Some(Self::Inet),
            0x2 => Some(Self::Inet6),
            0x3 => Some(Self::Unix),
            _ => None,
        }
    }

    const fn v2_hi(&self) -> u8 {
        (match self {
            AddressFamily::Unspecified => 0x0,
            AddressFamily::Inet => 0x1,
            AddressFamily::Inet6 => 0x2,
            AddressFamily::Unix => 0x3,
        }) << 4
    }
}

/// Transport Protocol.
///
/// other values are unspecified and must not be emitted in version 2 of this
/// protocol and must be rejected as invalid by receivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportProtocol {
    /// 0x0 : UNSPEC : the connection is forwarded for an unknown, unspecified
    /// or unsupported protocol. The sender should use this family when sending
    /// LOCAL commands or when dealing with unsupported protocol families. The
    /// receiver is free to accept the connection anyway and use the real endpoint
    /// addresses or to reject it. The receiver should ignore address information.
    Unspecified,

    /// 0x1 : STREAM : the forwarded connection uses a SOCK_STREAM protocol (eg:
    /// TCP or UNIX_STREAM). When used with AF_INET/AF_INET6 (TCP), the addresses
    /// are followed by the source and destination ports represented on 2 bytes
    /// each in network byte order.
    Stream,

    /// 0x2 : DGRAM : the forwarded connection uses a SOCK_DGRAM protocol (eg:
    /// UDP or UNIX_DGRAM). When used with AF_INET/AF_INET6 (UDP), the addresses
    /// are followed by the source and destination ports represented on 2 bytes
    /// each in network byte order.
    Datagram,
}

impl TransportProtocol {
    pub(crate) const fn from_v2_lo(val: u8) -> Option<Self> {
        match val {
            0x0 => Some(Self::Unspecified),
            0x1 => Some(Self::Stream),
            0x2 => Some(Self::Datagram),
            _ => None,
        }
    }

    const fn v2_lo(&self) -> u8 {
        match self {
            TransportProtocol::Unspecified => 0x0,
            TransportProtocol::Stream => 0x1,
            TransportProtocol::Datagram => 0x2,
        }
    }
}

/// Parsed PROXY protocol header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Header {
    /// Version 1 header.
    V1(v1::Header),

    /// Version 2 header.
    V2(v2::Header),
}

impl Header {
    /// Returns the PROXY protocol version used by this header.
    pub const fn version(&self) -> Version {
        match self {
            Self::V1(_) => Version::V1,
            Self::V2(_) => Version::V2,
        }
    }

    /// Returns the source socket address when the header carries TCP/UDP IP addresses.
    pub fn source_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::V1(header) => header.source_addr(),
            Self::V2(header) => header.source_addr(),
        }
    }

    /// Returns the destination socket address when the header carries TCP/UDP IP addresses.
    pub fn destination_addr(&self) -> Option<SocketAddr> {
        match self {
            Self::V1(header) => header.destination_addr(),
            Self::V2(header) => header.destination_addr(),
        }
    }
}

/// PROXY protocol parse error.
#[derive(Debug)]
pub struct ParseError {
    kind: ParseErrorKind,
}

#[derive(Debug)]
enum ParseErrorKind {
    Invalid(&'static str),
    Io(io::Error),
}

impl ParseError {
    pub(crate) fn invalid(message: &'static str) -> Self {
        Self {
            kind: ParseErrorKind::Invalid(message),
        }
    }

    pub(crate) fn io(err: io::Error) -> Self {
        Self {
            kind: ParseErrorKind::Io(err),
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParseErrorKind::Invalid(message) => f.write_str(message),
            ParseErrorKind::Io(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match &self.kind {
            ParseErrorKind::Invalid(_) => None,
            ParseErrorKind::Io(err) => Some(err),
        }
    }
}

impl From<io::Error> for ParseError {
    fn from(err: io::Error) -> Self {
        Self::io(err)
    }
}
