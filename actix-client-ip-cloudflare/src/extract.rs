use std::net::IpAddr;

use actix_utils::future::{err, ok, Ready};
use actix_web::{
    dev::{self, PeerAddr},
    http::header::Header as _,
    FromRequest, HttpRequest,
};

use crate::{fetch_cf_ips::TrustedIps, CfConnectingIp};

fn bad_req(err: impl Into<String>) -> actix_web::error::Error {
    let err = err.into();
    actix_web::error::ErrorBadRequest(format!("TrustedClientIp error: {err}"))
}

#[derive(Debug, Clone)]
pub struct TrustedClientIp(pub IpAddr);

impl_more::forward_display!(TrustedClientIp);

impl FromRequest for TrustedClientIp {
    type Error = actix_web::Error;
    type Future = Ready<Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _pl: &mut dev::Payload) -> Self::Future {
        let client_ip_hdr = match CfConnectingIp::parse(req) {
            Ok(ip) => ip,
            Err(_) => return err(bad_req("cf-connecting-ip header not present")),
        }
        .ip();

        let trusted_ips = match req.app_data::<TrustedIps>() {
            Some(ips) => ips,
            None => return err(bad_req("trusted ips not in app data")),
        };

        let peer_ip = PeerAddr::extract(req).into_inner().unwrap().0.ip();

        if trusted_ips.contains(peer_ip) {
            ok(Self(client_ip_hdr))
        } else {
            err(bad_req("cf-connecting-ip read from untrusted peer"))
        }
    }
}
