//! WIP
//! API is not in a good state yet.

use std::io;

use actix_web::{
    middleware::Logger,
    web::{self, Bytes},
    App, HttpRequest, HttpServer,
};
use actix_web_lab::extract::{RequestHash, RequestHasher};
use digest::Digest;
use local_channel::mpsc::Receiver;
use sha2::Sha256;
use tracing::info;

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    info!("staring server at http://localhost:8080");

    HttpServer::new(|| {
        App::new()
            .app_data(RequestHasher::from_fn(cf_signature_scheme))
            .wrap(Logger::default().log_target("@"))
            .route(
                "/",
                web::post().to(|body: RequestHash<String, Sha256>| async move {
                    base64::encode(body.hash())
                }),
            )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

/// Signature scheme of `body + nonce + path`.
async fn cf_signature_scheme(
    mut hasher: Sha256,
    req: HttpRequest,
    mut chunks: Receiver<Bytes>,
) -> Sha256 {
    while let Some(chunk) = chunks.recv().await {
        hasher.update(&chunk)
    }

    // nonce optional
    if let Some(hdr) = req.headers().get("Nonce") {
        hasher.update(hdr.as_bytes());
    }

    hasher.update(req.path().as_bytes());
    hasher
}
