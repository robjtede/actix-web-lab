//! Extractor for client IP addresses when proxied through Cloudflare.

#![forbid(unsafe_code)]
#![deny(rust_2018_idioms, nonstandard_style)]
#![warn(future_incompatible)]
// #![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod extract;
mod fetch_cf_ips;
mod header_v4;
mod header_v6;

#[cfg(feature = "fetch-ips")]
pub use self::fetch_cf_ips::fetch_trusted_cf_ips;
pub use self::{
    extract::TrustedClientIp,
    fetch_cf_ips::{TrustedIps, CF_URL_IPS},
    header_v4::{CfConnectingIp, CF_CONNECTING_IP},
    header_v6::{CfConnectingIpv6, CF_CONNECTING_IPV6},
};
