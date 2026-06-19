//! HTTP/1 backend that accepts connections carrying PROXY protocol headers.
//!
//! From the workspace root, start the backend:
//!
//! ```console
//! cargo run -p actix-proxy-protocol --example haproxy-v2-backend
//! ```
//!
//! Start HAProxy in another terminal:
//!
//! ```console
//! docker run --rm --name actix-proxy-protocol-haproxy \
//!   -p 18080:8080 \
//!   -p 18081:8081 \
//!   -v "$PWD/actix-proxy-protocol/examples/haproxy-v2.cfg:/usr/local/etc/haproxy/haproxy.cfg:ro" \
//!   haproxy:2.9-alpine
//! ```
//!
//! Send requests through the PROXY v1 and v2 frontends:
//!
//! ```console
//! curl --haproxy-protocol --haproxy-clientip 203.0.113.42 \
//!   --include http://127.0.0.1:18080/
//! curl --include http://127.0.0.1:18081/
//! ```

use std::{
    convert::Infallible,
    io,
    net::{IpAddr, Ipv4Addr, SocketAddr},
};

use actix_http::{
    HttpService, Protocol, Request, Response, StatusCode,
    body::BoxBody,
    error::DispatchError,
    header::{HeaderName, HeaderValue},
};
use actix_proxy_protocol::{Acceptor, Header, ProxyStream, Version};
use actix_rt::net::TcpStream;
use actix_server::Server;
use actix_service::{ServiceFactoryExt as _, fn_service};

const BACKEND_ADDR: SocketAddr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), 19_090);

#[derive(Debug, Clone)]
struct ProxyConnectionInfo {
    transport_peer_addr: Option<SocketAddr>,
    proxy_client_addr: Option<SocketAddr>,
    proxy_version: Option<Version>,
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    tracing_subscriber::fmt::fmt().without_time().init();

    tracing::info!("HTTP backend listening on {BACKEND_ADDR}");

    Server::build()
        .bind("proxy-protocol-http", BACKEND_ADDR, || {
            let accept_proxy_header = Acceptor::new().map_err(io::Error::other);

            let prepare_http = fn_service(|stream: ProxyStream<TcpStream>| async move {
                let proxy_client_addr = stream.header().and_then(Header::source_addr);
                let transport_peer_addr = stream.get_ref().peer_addr().ok();
                let actix_peer_addr = proxy_client_addr.or(transport_peer_addr);

                tracing::info!(
                    ?transport_peer_addr,
                    ?proxy_client_addr,
                    version = ?stream.header().map(Header::version),
                    "accepted connection",
                );

                Ok::<_, io::Error>((stream, Protocol::Http1, actix_peer_addr))
            });

            let http = HttpService::build()
                .on_connect_ext(|stream: &ProxyStream<TcpStream>, extensions| {
                    extensions.insert(ProxyConnectionInfo {
                        transport_peer_addr: stream.get_ref().peer_addr().ok(),
                        proxy_client_addr: stream.header().and_then(Header::source_addr),
                        proxy_version: stream.header().map(Header::version),
                    });
                })
                .finish(handle_request)
                .map_err(dispatch_io_error);

            accept_proxy_header.and_then(prepare_http).and_then(http)
        })?
        .workers(1)
        .run()
        .await
}

async fn handle_request(req: Request) -> Result<Response<BoxBody>, Infallible> {
    let mut res = Response::build(StatusCode::OK);
    let proxy_info = req.conn_data::<ProxyConnectionInfo>();

    if let Some(addr) = req.peer_addr() {
        insert_addr_header(&mut res, "x-actix-peer-addr", addr);
    }

    if let Some(info) = proxy_info {
        if let Some(addr) = info.transport_peer_addr {
            insert_addr_header(&mut res, "x-transport-peer-addr", addr);
        }

        if let Some(addr) = info.proxy_client_addr {
            insert_addr_header(&mut res, "x-proxy-client-addr", addr);
        }

        if let Some(version) = info.proxy_version {
            res.insert_header((
                HeaderName::from_static("x-proxy-version"),
                HeaderValue::from_str(&format!("{version:?}")).unwrap(),
            ));
        }
    }

    Ok(res
        .body(format!(
            "actix_peer={:?}\ntransport_peer={:?}\nproxy_client={:?}\nproxy_version={:?}\n",
            req.peer_addr(),
            proxy_info.and_then(|info| info.transport_peer_addr),
            proxy_info.and_then(|info| info.proxy_client_addr),
            proxy_info.and_then(|info| info.proxy_version),
        ))
        .map_into_boxed_body())
}

fn insert_addr_header(res: &mut actix_http::ResponseBuilder, name: &'static str, addr: SocketAddr) {
    res.insert_header((
        HeaderName::from_static(name),
        HeaderValue::from_str(&addr.to_string()).unwrap(),
    ));
}

fn dispatch_io_error(err: DispatchError) -> io::Error {
    io::Error::other(err.to_string())
}
