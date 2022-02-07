use std::io;

use actix_http::header::HeaderValue;
use actix_web::{
    error,
    middleware::Logger,
    web::{self, Bytes},
    App, HttpRequest, HttpServer,
};
use actix_web_lab::extract::{BodyHmac, HmacConfig};
use digest::Mac as _;
use futures_core::Stream;
use futures_util::StreamExt as _;
use hmac::SimpleHmac;
use sha2::{Sha256, Sha512};

#[allow(non_upper_case_globals)]
const db: () = ();

async fn lookup_public_key_in_db<T>(_db: &(), val: T) -> T {
    val
}

/// Extracts user's public key from request and pretends it is the secret key.
fn cf_extract_key_sync(req: &HttpRequest) -> actix_web::Result<Vec<u8>> {
    // public key, not encryption key
    let hdr = req.headers().get("Api-Key");
    let pub_key = hdr
        .map(HeaderValue::as_bytes)
        .map(base64::decode)
        .transpose()
        .map_err(error::ErrorInternalServerError)?
        .ok_or_else(|| error::ErrorUnauthorized("key not provided"))?;

    Ok(pub_key)
}

/// Extracts user's public key from request and pretends to look up secret key in the DB.
async fn cf_extract_key(req: &HttpRequest) -> actix_web::Result<Vec<u8>> {
    // public key, not encryption key
    let hdr = req.headers().get("Api-Key");
    let pub_key = hdr
        .map(HeaderValue::as_bytes)
        .map(base64::decode)
        .transpose()
        .map_err(error::ErrorInternalServerError)?
        .ok_or_else(|| error::ErrorUnauthorized("key not provided"))?;

    // let db = req.app_data::<DbPool>().unwrap();
    let secret_key = lookup_public_key_in_db(&db, pub_key).await;

    Ok(secret_key)
}

/// Signature scheme of `body + nonce + path`.
async fn cf_signature_scheme(
    mut hasher: SimpleHmac<Sha512>,
    req: &HttpRequest,
    mut chunks: impl Stream<Item = web::Bytes> + Unpin,
) {
    while let Some(chunk) = chunks.next().await {
        hasher.update(&chunk)
    }

    // nonce optional
    if let Some(hdr) = req.headers().get("Nonce") {
        hasher.update(hdr.as_bytes());
    }

    hasher.update(req.path().as_bytes());
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    log::info!("staring server at http://localhost:8080");

    HttpServer::new(|| {
        // let hmac_config = HmacConfig::static_key(*b"");
        let hmac_config = HmacConfig::dynamic_key(cf_extract_key_sync);

        App::new()
            .app_data(hmac_config)
            .wrap(Logger::default().log_target("@"))
            .route(
                "/",
                web::post().to(|body: BodyHmac<Bytes, Sha256>| async move {
                    Bytes::copy_from_slice(body.hash())
                }),
            )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
