//! Minimal TCP proxy that can add or consume PROXY protocol headers.
//!
//! Start any TCP/HTTP service on `127.0.0.1:8080`, then run this example:
//!
//! - `127.0.0.1:8081` forwards with a PROXY protocol v1 header.
//! - `127.0.0.1:8082` forwards with a PROXY protocol v2 header.
//! - `127.0.0.1:8083` consumes an incoming PROXY protocol header before forwarding.

use std::{
    io,
    net::{Ipv6Addr, SocketAddr},
};

use actix_proxy_protocol::{Header, ProxyStream, tlv, v1, v2};
use actix_rt::net::TcpStream;
use actix_server::Server;
use actix_service::{ServiceFactoryExt as _, fn_service};
use tokio::io::{AsyncWriteExt as _, copy_bidirectional};

fn upstream_addr() -> SocketAddr {
    SocketAddr::from(([127, 0, 0, 1], 8080))
}

async fn wrap_with_proxy_protocol_v1(mut downstream: TcpStream) -> io::Result<()> {
    let upstream_addr = upstream_addr();
    let mut upstream = TcpStream::connect(upstream_addr).await?;
    let source_addr = downstream.peer_addr()?;

    tracing::info!("forwarding {source_addr} to {upstream_addr} with PROXY v1");

    v1::Header::new_inet(source_addr, upstream_addr)
        .write_to_tokio(&mut upstream)
        .await?;

    copy_bidirectional(&mut downstream, &mut upstream).await?;

    Ok(())
}

async fn wrap_with_proxy_protocol_v2(mut downstream: TcpStream) -> io::Result<()> {
    let upstream_addr = upstream_addr();
    let mut upstream = TcpStream::connect(upstream_addr).await?;
    let source_addr = downstream.peer_addr()?;

    tracing::info!("forwarding {source_addr} to {upstream_addr} with PROXY v2");

    let mut header = if source_addr.is_ipv4() {
        v2::Header::new_tcp_ipv4_proxy(source_addr, upstream_addr)
    } else {
        v2::Header::new_tcp_ipv6_proxy(source_addr, SocketAddr::from((Ipv6Addr::LOCALHOST, 8080)))
    };

    header.add_typed_tlv(tlv::UniqueId::new(format!("conn-{source_addr}")));
    header.add_typed_tlv(tlv::Authority::new("localhost"));
    header.add_typed_tlv(tlv::Alpn::new("http/1.1"));
    header.add_crc32c_checksum();
    header.write_to_tokio(&mut upstream).await?;

    copy_bidirectional(&mut downstream, &mut upstream).await?;

    Ok(())
}

async fn unwrap_proxy_protocol(downstream: TcpStream) -> io::Result<()> {
    let mut downstream = ProxyStream::accept(downstream)
        .await
        .map_err(io::Error::other)?;
    let upstream_addr = upstream_addr();
    let mut upstream = TcpStream::connect(upstream_addr).await?;

    match downstream.header() {
        Some(Header::V1(header)) => {
            tracing::info!(
                "accepted PROXY v1 connection from {:?}",
                header.source_addr()
            );
        }
        Some(Header::V2(header)) => {
            tracing::info!(
                "accepted PROXY v2 connection from {:?}; crc32c valid: {:?}",
                header.source_addr(),
                header.validate_crc32c_tlv()
            );
        }
        None => unreachable!("required acceptor always returns a header"),
    }

    copy_bidirectional(&mut downstream, &mut upstream).await?;
    upstream.shutdown().await?;

    Ok(())
}

fn start_server() -> io::Result<Server> {
    tracing::info!("proxying to {}", upstream_addr());

    Ok(Server::build()
        .bind("proxy-protocol-v1", ("127.0.0.1", 8081), move || {
            fn_service(wrap_with_proxy_protocol_v1)
                .map_err(|err| tracing::error!("service error: {err:?}"))
        })?
        .bind("proxy-protocol-v2", ("127.0.0.1", 8082), move || {
            fn_service(wrap_with_proxy_protocol_v2)
                .map_err(|err| tracing::error!("service error: {err:?}"))
        })?
        .bind("proxy-protocol-unwrap", ("127.0.0.1", 8083), move || {
            fn_service(unwrap_proxy_protocol)
                .map_err(|err| tracing::error!("service error: {err:?}"))
        })?
        .workers(2)
        .run())
}

#[tokio::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt::fmt().without_time().init();

    start_server()?.await
}
