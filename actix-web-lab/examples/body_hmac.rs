//! Alternative approach to using `BodyHmac` type using more flexible `RequestSignature` type.

use std::io;

use actix_web::{
    App, Error, FromRequest, HttpRequest, HttpServer,
    middleware::Logger,
    web::{self, Bytes, Data},
};
use actix_web_lab::extract::{RequestSignature, RequestSignatureScheme};
use digest::{CtOutput, Mac};
use hmac::SimpleHmac;
use sha2::Sha256;
use tracing::info;

struct AbcSigningKey([u8; 32]);

/// Grabs variable signing key from app data.
async fn get_signing_key(req: &HttpRequest) -> actix_web::Result<[u8; 32]> {
    let key = Data::<AbcSigningKey>::extract(req).into_inner()?.0;
    Ok(key)
}

#[derive(Debug)]
struct AbcApi {
    /// Payload hash state.
    hmac: SimpleHmac<Sha256>,
}

impl RequestSignatureScheme for AbcApi {
    type Signature = CtOutput<SimpleHmac<Sha256>>;
    type Error = Error;

    async fn init(req: &HttpRequest) -> Result<Self, Self::Error> {
        let key = get_signing_key(req).await?;

        Ok(Self {
            hmac: SimpleHmac::new_from_slice(&key).unwrap(),
        })
    }

    async fn consume_chunk(&mut self, _req: &HttpRequest, chunk: Bytes) -> Result<(), Self::Error> {
        self.hmac.update(&chunk);
        Ok(())
    }

    async fn finalize(self, _req: &HttpRequest) -> Result<Self::Signature, Self::Error> {
        Ok(self.hmac.finalize())
    }

    fn verify(
        signature: Self::Signature,
        _req: &HttpRequest,
    ) -> Result<Self::Signature, Self::Error> {
        // pass-through signature since verification is not required for this scheme
        // (shown for completeness, this is the default impl of `verify` and could be removed)
        Ok(signature)
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    info!("staring server at http://localhost:8080");

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default().log_target("@"))
            .app_data(Data::new(AbcSigningKey([0; 32])))
            .route(
                "/",
                web::post().to(|body: RequestSignature<Bytes, AbcApi>| async move {
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
