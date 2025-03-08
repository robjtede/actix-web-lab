use std::net::IpAddr;

use actix_utils::future::{Ready, err, ok};
use actix_web::{
    FromRequest, HttpRequest,
    dev::{self, PeerAddr},
    http::header::Header as _,
};

use crate::{CfConnectingIp, CfConnectingIpv6, fetch_cf_ips::TrustedIps};

fn bad_req(err: impl Into<String>) -> actix_web::error::Error {
    actix_web::error::ErrorBadRequest(format!("TrustedClientIp error: {}", err.into()))
}

/// Extractor for a client IP that has passed through Cloudflare and is verified as not spoofed.
///
/// For this extractor to work, there must be an instance of [`TrustedIps`] in your app data.
#[derive(Debug, Clone)]
pub struct TrustedClientIp(pub IpAddr);

impl_more::forward_display!(TrustedClientIp);

impl FromRequest for TrustedClientIp {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _pl: &mut dev::Payload) -> Self::Future {
        let client_ip_hdr = match CfConnectingIp::parse(req) {
            Ok(ip) => Ok(ip.ip()),
            Err(_) => Err(()),
        };

        let client_ipv6_hdr = match CfConnectingIpv6::parse(req) {
            Ok(ip) => Ok(ip.ip()),
            Err(_) => Err(()),
        };

        let client_ip = match client_ip_hdr.or(client_ipv6_hdr) {
            Ok(ip) => ip,
            Err(_) => return err(bad_req("cf-connecting-ip header not present")),
        };

        let trusted_ips = match req.app_data::<TrustedIps>() {
            Some(ips) => ips,
            None => return err(bad_req("trusted IPs not in app data")),
        };

        let peer_ip = PeerAddr::extract(req).into_inner().unwrap().0.ip();

        if trusted_ips.contains(peer_ip) {
            ok(Self(client_ip))
        } else {
            err(bad_req("cf-connecting-ip read from untrusted peer"))
        }
    }
}

#[cfg(test)]
mod tests {
    use actix_web::test::TestRequest;

    use super::*;

    fn sample_trusted_ips() -> TrustedIps {
        TrustedIps {
            cidr_ranges: Vec::from([
                "103.21.244.0/22".parse().unwrap(),
                "198.41.128.0/17".parse().unwrap(),
            ]),
        }
    }

    #[test]
    fn missing_app_data() {
        let req = TestRequest::default()
            .insert_header(("CF-Connecting-IP", "4.5.6.7"))
            .to_http_request();

        TrustedClientIp::extract(&req).into_inner().unwrap_err();
    }

    #[test]
    fn from_untrusted_peer() {
        let trusted_ips = sample_trusted_ips();

        let req = TestRequest::default()
            .insert_header(("CF-Connecting-IP", "4.5.6.7"))
            .peer_addr("10.0.1.1:27432".parse().unwrap())
            .app_data(trusted_ips)
            .to_http_request();

        TrustedClientIp::extract(&req).into_inner().unwrap_err();
    }

    #[test]
    fn from_trusted_peer() {
        let trusted_ips = sample_trusted_ips();

        let req = TestRequest::default()
            .insert_header(("CF-Connecting-IP", "4.5.6.7"))
            .peer_addr("103.21.244.0:27432".parse().unwrap())
            .app_data(trusted_ips)
            .to_http_request();

        TrustedClientIp::extract(&req).into_inner().unwrap();
    }

    #[test]
    fn from_additional_trusted_peer() {
        let trusted_ips = sample_trusted_ips().add_ip_range("10.0.1.0/24".parse().unwrap());

        let req = TestRequest::default()
            .insert_header(("CF-Connecting-IP", "4.5.6.7"))
            .peer_addr("10.0.1.1:27432".parse().unwrap())
            .app_data(trusted_ips)
            .to_http_request();

        TrustedClientIp::extract(&req).into_inner().unwrap();
    }
}
