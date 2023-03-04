//! Extractor for client IP addresses when proxied through Cloudflare.

// #![forbid(unsafe_code)] // urgh why cidr-utils
#![deny(rust_2018_idioms, nonstandard_style)]
#![warn(future_incompatible)]
// #![warn(missing_docs)]
#![cfg_attr(docsrs, feature(doc_auto_cfg))]

mod extract;
mod fetch_cf_ips;
mod header_v4;
// mod header_v6;

pub use self::extract::TrustedClientIp;
#[cfg(feature = "fetch-ips")]
pub use self::fetch_cf_ips::fetch_trusted_cf_ips;
pub use self::fetch_cf_ips::{TrustedIps, CF_URL_IPS};
pub use self::header_v4::CfConnectingIp;
