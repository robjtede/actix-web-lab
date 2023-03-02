//! Extractor for client IP addresses when proxied through Cloudflare.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms, nonstandard_style)]
#![warn(future_incompatible)]
// #![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

use std::net::IpAddr;

use actix_utils::future::{err, ok, Ready};
use actix_web::{dev, http::header::HeaderName, FromRequest, HttpRequest};
use cidr_utils::{cidr::IpCidr, utils::IpCidrCombiner};
use serde::Deserialize;

/// URL for Cloudflare's canonical list of IP ranges.
pub const CF_URL_IPS: &str = "https://api.cloudflare.com/client/v4/ips";

/// Cloudflare's `cf-connecting-ip` header name.
#[allow(clippy::declare_interior_mutable_const)]
pub const CF_CONNECTING_IP: HeaderName = HeaderName::from_static("cf-connecting-ip");

/// A trusted source for client's IP address when server is behind Cloudflare.
pub struct CfConnectingIp(pub IpAddr);

fn bad_req() -> actix_web::error::Error {
    actix_web::error::ErrorBadRequest("err; todo")
}

impl FromRequest for CfConnectingIp {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _pl: &mut dev::Payload) -> Self::Future {
        let cf_ips = match req.app_data::<CloudflareIps>() {
            Some(ips) => ips,
            None => return err(bad_req()),
        };

        let hdr_val = match req.headers().get(CF_CONNECTING_IP) {
            Some(hdr_val) => hdr_val,
            None => return err(bad_req()),
        };

        let hdr_str = match hdr_val.to_str() {
            Ok(hdr_str) => hdr_str,
            Err(_) => return err(bad_req()),
        };

        let client_ip = match hdr_str.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => return err(bad_req()),
        };

        let conn_info = req.connection_info();

        let peer_ip_str = match conn_info.peer_addr() {
            Some(ip_str) => ip_str,
            None => return err(bad_req()),
        };

        let peer_ip = match peer_ip_str.parse::<IpAddr>() {
            Ok(ip) => ip,
            Err(_) => return err(bad_req()),
        };

        if cf_ips.contains(peer_ip) {
            ok(Self(client_ip))
        } else {
            err(bad_req())
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CfIpsResult {
    ipv4_cidrs: Vec<cidr_utils::cidr::Ipv4Cidr>,
    ipv6_cidrs: Vec<cidr_utils::cidr::Ipv6Cidr>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum CfIpsResponse {
    Success { result: CfIpsResult },
    Failure { success: bool },
}

/// IP CIDR ranges owned by Cloudflare
#[derive(Debug)]
pub struct CloudflareIps {
    cidr_ranges: IpCidrCombiner,
}

impl CloudflareIps {
    pub fn try_from_response(res: CfIpsResponse) -> Result<Self, Err> {
        let ips = match res {
            CfIpsResponse::Success { result } => result,
            CfIpsResponse::Failure { .. } => {
                tracing::error!("parsing response returned success: false");
                return Err(Err::Fetch);
            }
        };

        let mut cidr_ranges = IpCidrCombiner::new();

        for cidr in ips.ipv4_cidrs {
            cidr_ranges.push(IpCidr::V4(cidr));
        }

        for cidr in ips.ipv6_cidrs {
            cidr_ranges.push(IpCidr::V6(cidr));
        }

        Ok(Self { cidr_ranges })
    }

    /// Returns true if `ip` is controlled by Cloudflare.
    pub fn contains(&self, ip: IpAddr) -> bool {
        self.cidr_ranges.contains(ip)
    }
}

#[derive(Debug)]
pub enum Err {
    Fetch,
}

impl_more::impl_display_enum!(Err, Fetch => "failed to fetch");

impl std::error::Error for Err {}

#[cfg(feature = "fetch-ips")]
pub async fn fetch_ips() -> Result<CloudflareIps, Err> {
    let client = awc::Client::new();

    tracing::debug!("fetching cloudflare ips");
    let mut res = client.get(CF_URL_IPS).send().await.map_err(|err| {
        tracing::error!("{err}");
        Err::Fetch
    })?;

    tracing::debug!("parsing response");
    let res = res.json::<CfIpsResponse>().await.map_err(|err| {
        tracing::error!("{err}");
        Err::Fetch
    })?;

    CloudflareIps::try_from_response(res)
}
