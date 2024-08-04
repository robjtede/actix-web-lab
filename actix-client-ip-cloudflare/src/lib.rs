//! Extractor for trustworthy client IP addresses when proxied through Cloudflare.
//!
//! When traffic to your web server is proxied through Cloudflare, it is tagged with headers
//! containing the original client's IP address. See the [Cloudflare documentation] for info on
//! configuring your proxy settings. However, these headers can be spoofed by clients if your origin
//! server is exposed to the internet.
//!
//! The goal of this crate is to provide a simple extractor for clients' IP addresses whilst
//! ensuring their integrity. To achieve this, it is necessary to build a list of trusted peers
//! which are guaranteed to provide (e.g., Cloudflare) or pass-though (e.g., local load-balancers)
//! the headers accurately.
//!
//! Cloudflare's trustworthy IP ranges sometimes change, so this crate also provides a utility for
//! obtaining them from Cloudflare's API ([`fetch_trusted_cf_ips()`]). If your origin server's
//! direct peer _is_ Cloudflare, this list will be sufficient to establish the trust chain. However,
//! if your setup includes load balancers or reverse proxies then you'll need to add their IP ranges
//! to the trusted set for the [`TrustedClientIp`] extractor to work as expected.
//!
//! # Typical Usage
//!
//! 1. Add an instance of [`TrustedIps`] to your app data. It is recommended to construct your
//!    trusted IP set using [`fetch_trusted_cf_ips()`] and add any further trusted ranges to that.
//! 1. Use the [`TrustedClientIp`] extractor in your handlers.
//!
//! # Example
//!
//! ```no_run
//! # async {
//! # use actix_web::{App, get, HttpServer};
//! use actix_client_ip_cloudflare::{fetch_trusted_cf_ips, TrustedClientIp};
//!
//! let cloudflare_ips = fetch_trusted_cf_ips()
//!     # // rustfmt ignore
//!     .await
//!     .unwrap()
//!     .add_loopback_ips();
//!
//! HttpServer::new(move || {
//!     App::new()
//!         # // rustfmt ignore
//!         .app_data(cloudflare_ips.clone())
//!         .service(handler)
//! });
//!
//! #[get("/")]
//! async fn handler(client_ip: TrustedClientIp) -> String {
//!     client_ip.to_string()
//! }
//! # };
//! ```
//!
//! # Crate Features
//!
//! `fetch-ips` (default): Enables functionality to (asynchronously) fetch Cloudflare's trusted IP list from
//! their API. This feature includes `rustls` but if you prefer OpenSSL you can use it by disabling
//! default crate features and enabling `fetch-ips-openssl` instead.
//!
//! [Cloudflare documentation]: https://developers.cloudflare.com/fundamentals/reference/http-request-headers

#![forbid(unsafe_code)]
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
