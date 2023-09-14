use std::{convert::Infallible, net::IpAddr};

use actix_web::{
    error,
    http::header::{self, Header, HeaderName, HeaderValue, TryIntoHeaderValue},
    HttpMessage,
};

/// Cloudflare's `cf-connecting-ipv6` header name.
#[allow(clippy::declare_interior_mutable_const)]
pub const CF_CONNECTING_IPV6: HeaderName = HeaderName::from_static("cf-connecting-ipv6");

/// Header containing client's IPv6 address when server is behind Cloudflare.
#[derive(Debug, Clone)]
pub enum CfConnectingIpv6 {
    /// Extracted client IPv6 address that has been forwarded by a trustworthy peer.
    Trusted(IpAddr),

    /// Extracted client IPv6 address that has no trust guarantee.
    Untrusted(IpAddr),
}

impl CfConnectingIpv6 {
    /// Returns client IPv6 address, whether trusted or not.
    pub fn ip(&self) -> IpAddr {
        match self {
            Self::Trusted(ip) => *ip,
            Self::Untrusted(ip) => *ip,
        }
    }

    /// Returns `true` if this header is `Trusted`.
    #[must_use]
    pub fn is_trusted(&self) -> bool {
        matches!(self, Self::Trusted(..))
    }
}

impl_more::impl_display_enum!(
    CfConnectingIpv6,
    Trusted(ip) => "{ip}",
    Untrusted(ip) => "{ip}",
);

impl TryIntoHeaderValue for CfConnectingIpv6 {
    type Error = Infallible;

    fn try_into_value(self) -> Result<HeaderValue, Self::Error> {
        Ok(self.ip().to_string().parse().unwrap())
    }
}

impl Header for CfConnectingIpv6 {
    fn name() -> HeaderName {
        CF_CONNECTING_IPV6
    }

    fn parse<M: HttpMessage>(msg: &M) -> Result<Self, error::ParseError> {
        let ip = header::from_one_raw_str(msg.headers().get(Self::name()))?;
        Ok(Self::Untrusted(ip))
    }
}
