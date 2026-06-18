//! Demonstrates obtaining the trusted set of CloudFlare IPs.

use std::net::IpAddr;

use actix_client_ip_cloudflare::fetch_trusted_cf_ips;

#[actix_web::main]
async fn main() {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let ips = fetch_trusted_cf_ips().await.unwrap();

    for ip in [
        IpAddr::from([103, 21, 243, 0]),
        IpAddr::from([103, 21, 244, 0]),
        IpAddr::from([103, 21, 245, 0]),
        IpAddr::from([103, 21, 246, 0]),
        IpAddr::from([103, 21, 247, 0]),
        IpAddr::from([103, 21, 248, 0]),
    ] {
        assert!(ips.contains(ip));
    }
}
