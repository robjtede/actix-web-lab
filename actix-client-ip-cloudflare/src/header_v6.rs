use std::{convert::Infallible, net::IpAddr};

use actix_web::{
    error,
    http::header::{self, Header, HeaderName, HeaderValue, TryIntoHeaderValue},
    HttpMessage,
};

/// Cloudflare's `cf-connecting-ipv6` header name.
#[allow(clippy::declare_interior_mutable_const)]
pub const CF_CONNECTING_IPV6: HeaderName = HeaderName::from_static("cf-connecting-ipv6");

/// A source for client's IP address when server is behind Cloudflare.
#[derive(Debug, Clone)]
pub enum CfConnectingIpv6 {
    Trusted(IpAddr),
    Untrusted(IpAddr),
}

impl CfConnectingIpv6 {
    pub fn ip(&self) -> IpAddr {
        match self {
            Self::Trusted(ip) => *ip,
            Self::Untrusted(ip) => *ip,
        }
    }

    // pub(crate) fn into_trusted(self) -> Self {
    //     match self {
    //         Self::Trusted(ip) => Self::Trusted(ip),
    //         Self::Untrusted(ip) => Self::Trusted(ip),
    //     }
    // }
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
