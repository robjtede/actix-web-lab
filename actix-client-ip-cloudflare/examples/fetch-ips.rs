//! Demonstrates obtaining the trusted set of CloudFlare IPs.

mod util;

use actix_client_ip_cloudflare::fetch_trusted_cf_ips;

#[actix_web::main]
async fn main() {
    util::init_standard_logger();
    util::init_rustls_provider();

    let ips = fetch_trusted_cf_ips().await.unwrap();

    for ip_network in ips {
        println!("{ip_network}");
    }
}
