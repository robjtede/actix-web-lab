use std::net::{IpAddr, Ipv6Addr};

use actix_web::{
    http::header::{self, HeaderName},
    HttpMessage, HttpRequest,
};

/// Cloudflare's `cf-connecting-ipv6` header name.
#[allow(clippy::declare_interior_mutable_const)]
pub const CF_CONNECTING_IPV6: HeaderName = HeaderName::from_static("cf-connecting-ipv6");

/// A source for client's IP address when server is behind Cloudflare.
pub enum CfConnectingIpv6 {
    Trusted(IpAddr),
    Untrusted(IpAddr),
}

// impl Header for CfConnectingIp {
//     fn name() -> HeaderName {
//         CF_CONNECTING_IPV6
//     }

//     fn parse<M: HttpMessage>(msg: &M) -> Result<Self, actix_web::error::ParseError> {
//         header::from_one_raw_str(msg.headers().get(Self::name()))
//     }
// }

impl CfConnectingIpv6 {
    // fn from_req(req: &HttpRequest) -> Option<Ipv6Addr> {
    //     let val = req.headers().get(CF_CONNECTING_IPV6)?;
    //     let ip_str = val.to_str().ok()?;
    //     ip_str.parse().ok()
    // }
}
