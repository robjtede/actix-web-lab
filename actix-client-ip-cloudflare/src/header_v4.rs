use std::{convert::Infallible, net::IpAddr};

use actix_web::{
    HttpMessage, error,
    http::header::{self, Header, HeaderName, HeaderValue, TryIntoHeaderValue},
};

/// Cloudflare's `cf-connecting-ip` header name.
#[allow(clippy::declare_interior_mutable_const)]
pub const CF_CONNECTING_IP: HeaderName = HeaderName::from_static("cf-connecting-ip");

/// Header containing client's IPv4 address when server is behind Cloudflare.
#[derive(Debug, Clone)]
pub enum CfConnectingIp {
    /// Extracted client IPv4 address that has been forwarded by a trustworthy peer.
    Trusted(IpAddr),

    /// Extracted client IPv4 address that has no trust guarantee.
    Untrusted(IpAddr),
}

impl CfConnectingIp {
    /// Returns client IPv4 address, whether trusted or not.
    pub fn ip(&self) -> IpAddr {
        match self {
            Self::Trusted(ip) => *ip,
            Self::Untrusted(ip) => *ip,
        }
    }
}

impl_more::impl_display_enum! {
    CfConnectingIp:
    Trusted(ip) => "{ip}",
    Untrusted(ip) => "{ip}",
}

impl TryIntoHeaderValue for CfConnectingIp {
    type Error = Infallible;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        Ok(self.ip().to_string().parse().unwrap())
    }
}

impl Header for CfConnectingIp {
    fn name() -> HeaderName {
        CF_CONNECTING_IP
    }

    fn parse<M: HttpMessage>(msg: &M) -> Result<Self, error::ParseError> {
        let ip = header::from_one_raw_str(msg.headers().get(Self::name()))?;
        Ok(Self::Untrusted(ip))
    }
}
