use std::io;

use actix_web::{
    error,
    http::header::{HeaderName, HeaderValue},
    middleware::Logger,
    web::{self, Bytes},
    App, Error, HttpRequest, HttpServer,
};
use actix_web_lab::extract::{Json, RequestSignature, RequestSignatureScheme};
use async_trait::async_trait;
use ed25519_dalek::{Digest as _, PublicKey, Sha512, Signature};
use hex_literal::hex;
use once_cell::sync::Lazy;
use tracing::info;

const APP_PUBLIC_KEY_BYTES: &[u8] =
    &hex!("4555e5cfa7ff6bc45aa0f6e48bdd0c103b49fc03829365c4f147f42d47d6b348");

static APP_PUBLIC_KEY: Lazy<PublicKey> =
    Lazy::new(|| PublicKey::from_bytes(&*APP_PUBLIC_KEY_BYTES).unwrap());
static SIG_HDR_NAME: Lazy<HeaderName> =
    Lazy::new(|| HeaderName::from_static("X-Signature-Ed25519"));
static TS_HDR_NAME: Lazy<HeaderName> =
    Lazy::new(|| HeaderName::from_static("X-Signature-Timestamp"));

#[derive(Debug)]
struct DiscordWebhook {
    /// Signature taken from webhook request header.
    candidate_signature: Signature,

    /// Payload hash state.
    hasher: Sha512,
}

impl DiscordWebhook {
    fn get_timestamp(req: &HttpRequest) -> Result<&[u8], Error> {
        req.headers()
            .get(&*TS_HDR_NAME)
            .map(HeaderValue::as_bytes)
            .ok_or_else(|| error::ErrorUnauthorized("timestamp not provided"))
    }

    fn get_signature(req: &HttpRequest) -> Result<Signature, Error> {
        let sig: [u8; 64] = req
            .headers()
            .get(&*SIG_HDR_NAME)
            .map(HeaderValue::as_bytes)
            .map(hex::decode)
            .transpose()
            .map_err(|_| error::ErrorInternalServerError("invalid signature"))?
            .ok_or_else(|| error::ErrorUnauthorized("signature not provided"))?
            .try_into()
            .map_err(|_| error::ErrorInternalServerError("invalid signature"))?;

        Ok(Signature::from(sig))
    }
}

#[async_trait(?Send)]
impl RequestSignatureScheme for DiscordWebhook {
    /// For asymmetric signature schemes, we need to store the intermediate hash state
    type Signature = (Sha512, Signature);

    type Error = Error;

    async fn init(req: &HttpRequest) -> Result<Self, Self::Error> {
        let ts = Self::get_timestamp(req)?;
        let candidate_signature = Self::get_signature(req)?;

        let mut hasher = Sha512::new();
        hasher.update(&ts);

        Ok(Self {
            candidate_signature,
            hasher,
        })
    }

    async fn consume_chunk(&mut self, _req: &HttpRequest, chunk: Bytes) -> Result<(), Self::Error> {
        self.hasher.update(&chunk);
        Ok(())
    }

    async fn finalize(self, _req: &HttpRequest) -> Result<Self::Signature, Self::Error> {
        Ok((self.hasher, self.candidate_signature))
    }

    fn verify(
        (hasher, candidate_signature): Self::Signature,
        _req: &HttpRequest,
    ) -> Result<Self::Signature, Self::Error> {
        if APP_PUBLIC_KEY
            .verify_prehashed(hasher, None, &candidate_signature)
            .is_ok()
        {
            Ok((Sha512::new(), candidate_signature))
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
            web::post().to(
                |body: RequestSignature<Json<serde_json::Value>, DiscordWebhook>| async move {
                    let (form, _) = body.into_parts();
                    format!("{form:#?}")
                },
            ),
        )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
