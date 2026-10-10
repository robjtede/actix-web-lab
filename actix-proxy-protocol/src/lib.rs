//! Actix networking integration for the [PROXY protocol].
//!
//! Protocol parsing, serialization, and TLV types are provided by [`proxyproto`] and re-exported
//! here for compatibility.
//!
//! [proxy protocol]: https://www.haproxy.org/download/1.8/doc/proxy-protocol.txt
//! [`proxyproto`]: https://crates.io/crates/proxyproto

#![doc(html_logo_url = "https://actix.rs/img/logo.png")]
#![doc(html_favicon_url = "https://actix.rs/favicon.ico")]

mod service;

pub use proxyproto::{
    AddressFamily, Command, Header, ParseError, TransportProtocol, Version, tlv, v2,
};

pub use self::service::{Acceptor, AcceptorService, HeaderPolicy, ProxyProtocolError, ProxyStream};

/// PROXY protocol v1 header support.
pub mod v1 {
    pub use proxyproto::v1::*;

    pub use crate::{Acceptor, AcceptorService, HeaderPolicy, ProxyProtocolError, ProxyStream};
}
