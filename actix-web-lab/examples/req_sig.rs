//! Implements `RequestSignatureScheme` for a made-up API.

use std::io;

use actix_web::{
    App, Error, HttpRequest, HttpServer, error,
    http::header::HeaderValue,
    middleware::Logger,
    web::{self, Bytes},
};
use actix_web_lab::extract::{RequestSignature, RequestSignatureScheme};
use base64::prelude::*;
use digest::{CtOutput, Digest, Mac};
use generic_array::GenericArray;
use hmac::SimpleHmac;
use sha2::{Sha256, Sha512};
use tracing::info;

#[allow(non_upper_case_globals)]
const db: () = ();

async fn lookup_public_key_in_db<T>(_db: &(), val: T) -> T {
    val
}

/// Extracts user's public key from request and pretends to look up secret key in the DB.
async fn get_base64_api_key(req: &HttpRequest) -> actix_web::Result<Vec<u8>> {
    // public key, not encryption key
    let pub_key = req
        .headers()
        .get("Api-Key")
        .map(HeaderValue::as_bytes)
        .map(|bytes| BASE64_STANDARD.decode(bytes))
        .transpose()
        .map_err(|_| error::ErrorInternalServerError("invalid api key"))?
        .ok_or_else(|| error::ErrorUnauthorized("api key not provided"))?;

    // in a real app it would be something like:
    // let db = req.app_data::<Data<DbPool>>().unwrap();
    let secret_key = lookup_public_key_in_db(&db, pub_key).await;

    Ok(secret_key)
}

fn get_user_signature(req: &HttpRequest) -> actix_web::Result<Vec<u8>> {
    req.headers()
        .get("Signature")
        .map(HeaderValue::as_bytes)
        .map(|bytes| BASE64_STANDARD.decode(bytes))
        .transpose()
        .map_err(|_| error::ErrorInternalServerError("invalid signature"))?
        .ok_or_else(|| error::ErrorUnauthorized("signature not provided"))
}

#[derive(Debug, Default)]
struct ExampleApi {
    /// Key derived from fetching user's API private key from database.
    key: Vec<u8>,

    /// Payload hash state.
    hasher: Sha256,
}

impl RequestSignatureScheme for ExampleApi {
    type Signature = CtOutput<SimpleHmac<Sha512>>;
    type Error = Error;

    async fn init(req: &HttpRequest) -> Result<Self, Self::Error> {
        let key = get_base64_api_key(req).await?;

        let mut hasher = Sha256::new();

        // optional nonce
        if let Some(nonce) = req.headers().get("nonce") {
            Digest::update(&mut hasher, nonce.as_bytes());
        }

        if let Some(path) = req.uri().path_and_query() {
            Digest::update(&mut hasher, path.as_str().as_bytes())
        }

        Ok(Self { key, hasher })
    }

    async fn consume_chunk(&mut self, _req: &HttpRequest, chunk: Bytes) -> Result<(), Self::Error> {
        Digest::update(&mut self.hasher, &chunk);
        Ok(())
    }

    async fn finalize(self, _req: &HttpRequest) -> Result<Self::Signature, Self::Error> {
        println!("using key: {:X?}", &self.key);

        let mut hmac = <SimpleHmac<Sha512>>::new_from_slice(&self.key).unwrap();

        let payload_hash = self.hasher.finalize();
        println!("payload hash: {payload_hash:X?}");
        Mac::update(&mut hmac, &payload_hash);

        Ok(hmac.finalize())
    }

    fn verify(
        signature: Self::Signature,
        req: &HttpRequest,
    ) -> Result<Self::Signature, Self::Error> {
        let user_sig = get_user_signature(req)?;
        let user_sig = CtOutput::new(GenericArray::from_slice(&user_sig).to_owned());

        if signature == user_sig {
            Ok(signature)
        } else {
            Err(error::ErrorUnauthorized(
                "given signature does not match calculated signature",
            ))
        }
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
                let (body, sig) = body.into_parts();
                let sig = sig.into_bytes().to_vec();
                format!("{body:?}\n\n{sig:x?}")
            }),
        )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
