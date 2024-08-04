//! Demonstrates use of the client IP header extractor.

use actix_client_ip_cloudflare::{fetch_trusted_cf_ips, CfConnectingIp, TrustedClientIp};
use actix_web::{get, web::Header, App, HttpServer, Responder};

#[get("/raw-header")]
async fn header(Header(client_ip): Header<CfConnectingIp>) -> impl Responder {
    match client_ip {
        CfConnectingIp::Trusted(_ip) => unreachable!(),
        CfConnectingIp::Untrusted(ip) => format!("Possibly fake client IP: {ip}"),
    }
}

#[get("/client-ip")]
async fn trusted_client_ip(client_ip: TrustedClientIp) -> impl Responder {
    format!("Trusted client IP: {client_ip}")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let cloudflare_ips = fetch_trusted_cf_ips()
        .await
        .unwrap()
        .add_ip_range("127.0.0.1/24".parse().unwrap());

    HttpServer::new(move || {
        App::new()
            .app_data(cloudflare_ips.clone())
            .service(header)
            .service(trusted_client_ip)
    })
    .bind(("127.0.0.1", 8080))?
    .workers(2)
    .run()
    .await
}
