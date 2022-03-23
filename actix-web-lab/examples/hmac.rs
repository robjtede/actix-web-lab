use std::io;

use actix_http::header::HeaderValue;
use actix_web::{
    error,
    middleware::Logger,
    web::{self, Bytes},
    App, Error, HttpRequest, HttpServer,
};
use actix_web_lab::extract::{RequestSignature, RequestSignatureScheme};
use async_trait::async_trait;
use digest::{CtOutput, Digest, Mac};
use hmac::SimpleHmac;
use sha2::{Sha256, Sha512};
use tracing::info;

#[allow(non_upper_case_globals)]
const db: () = ();

async fn lookup_public_key_in_db<T>(_db: &(), val: T) -> T {
    val
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

#[derive(Debug, Default)]
struct ExampleApi {
    /// Key derived from fetching user's API private key from database.
    key: Vec<u8>,

    /// Payload hash state.
    hasher: Sha256,
}

#[async_trait(?Send)]
impl RequestSignatureScheme for ExampleApi {
    type Output = SimpleHmac<Sha512>;
    type Error = Error;

    async fn init(req: &HttpRequest) -> Result<Self, Self::Error> {
        let key = cf_extract_key(req).await?;

        let mut hasher = Sha256::new();

        // optional nonce
        if let Some(nonce) = req.headers().get("nonce") {
            Digest::update(&mut hasher, nonce.as_bytes());
        }

        // path is not optional but easier to write like this
        if let Some(path) = req.uri().path_and_query() {
            Digest::update(&mut hasher, path.as_str().as_bytes())
        }

        Ok(Self { key, hasher })
    }

    async fn digest_chunk(&mut self, _req: &HttpRequest, chunk: Bytes) -> Result<(), Self::Error> {
        Digest::update(&mut self.hasher, &chunk);
        Ok(())
    }

    async fn finalize(
        &mut self,
        _req: &HttpRequest,
    ) -> Result<CtOutput<Self::Output>, Self::Error> {
        println!("using key: {:X?}", &self.key);

        let mut hmac = <SimpleHmac<Sha512>>::new_from_slice(&self.key).unwrap();

        let payload_hash = self.hasher.finalize_reset();
        println!("payload hash: {payload_hash:X?}");
        Mac::update(&mut hmac, &payload_hash);

        Ok(hmac.finalize())
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    info!("staring server at http://localhost:8080");

    HttpServer::new(|| {
        App::new().wrap(Logger::default().log_target("@")).route(
            "/",
            web::post().to(|body: RequestSignature<Bytes, ExampleApi>| async move {
                // if !body.verify_slice(b"correct-signature") {
                //     return "HMAC signature not correct";
                // }

                // "OK"

                let (body, sig) = body.into_parts();
                format!("{body:?}\n\n{sig:x?}")
            }),
        )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
