//! Implements verification of Discord Webhook signatures.

use std::{fs::File, io, sync::LazyLock};

use actix_web::{
    App, Error, HttpRequest, HttpServer, error,
    http::header::{HeaderName, HeaderValue},
    middleware::Logger,
    web::{self, Bytes},
};
use actix_web_lab::extract::{Json, RequestSignature, RequestSignatureScheme};
use bytes::{BufMut as _, BytesMut};
use ed25519_dalek::{Signature, Verifier as _, VerifyingKey};
use hex_literal::hex;
use rustls::{ServerConfig, pki_types::PrivateKeyDer};
use rustls_pemfile::{certs, pkcs8_private_keys};
use tracing::info;

const APP_PUBLIC_KEY_BYTES: &[u8; 32] =
    &hex!("d7d9a14753b591be99a0c5721be8083b1e486c3fcdc6ac08bfb63a6e5c204569");

static SIG_HDR_NAME: HeaderName = HeaderName::from_static("x-signature-ed25519");
static TS_HDR_NAME: HeaderName = HeaderName::from_static("x-signature-timestamp");
static APP_PUBLIC_KEY: LazyLock<VerifyingKey> =
    LazyLock::new(|| VerifyingKey::from_bytes(APP_PUBLIC_KEY_BYTES).unwrap());

#[derive(Debug)]
struct DiscordWebhook {
    /// Signature taken from webhook request header.
    candidate_signature: Signature,

    /// Cloned payload state.
    chunks: Vec<Bytes>,
}

impl DiscordWebhook {
    fn get_timestamp(req: &HttpRequest) -> Result<&[u8], Error> {
        req.headers()
            .get(&TS_HDR_NAME)
            .map(HeaderValue::as_bytes)
            .ok_or_else(|| error::ErrorUnauthorized("timestamp not provided"))
    }

    fn get_signature(req: &HttpRequest) -> Result<Signature, Error> {
        let sig: [u8; 64] = req
            .headers()
            .get(&SIG_HDR_NAME)
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

impl RequestSignatureScheme for DiscordWebhook {
    type Signature = (BytesMut, Signature);

    type Error = Error;

    async fn init(req: &HttpRequest) -> Result<Self, Self::Error> {
        let ts = Self::get_timestamp(req)?.to_owned();
        let candidate_signature = Self::get_signature(req)?;

        Ok(Self {
            candidate_signature,
            chunks: vec![Bytes::from(ts)],
        })
    }

    async fn consume_chunk(&mut self, _req: &HttpRequest, chunk: Bytes) -> Result<(), Self::Error> {
        self.chunks.push(chunk);
        Ok(())
    }

    async fn finalize(self, _req: &HttpRequest) -> Result<Self::Signature, Self::Error> {
        let buf_len = self.chunks.iter().map(|chunk| chunk.len()).sum();
        let mut buf = BytesMut::with_capacity(buf_len);

        for chunk in self.chunks {
            buf.put(chunk);
        }

        Ok((buf, self.candidate_signature))
    }

    fn verify(
        (payload, candidate_signature): Self::Signature,
        _req: &HttpRequest,
    ) -> Result<Self::Signature, Self::Error> {
        if APP_PUBLIC_KEY
            .verify(&payload, &candidate_signature)
            .is_ok()
        {
            Ok((payload, candidate_signature))
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

    info!("staring server at http://0.0.0.0:443");

    HttpServer::new(|| {
        App::new().wrap(Logger::default().log_target("@")).route(
            "/webhook",
            web::post().to(
                |body: RequestSignature<Json<serde_json::Value>, DiscordWebhook>| async move {
                    let (Json(form), _) = body.into_parts();
                    println!("{}", serde_json::to_string_pretty(&form).unwrap());

                    web::Json(serde_json::json!({
                        "type": 1
                    }))
                },
            ),
        )
    })
    .workers(2)
    .bind_rustls_0_23(("0.0.0.0", 443), load_rustls_config())?
    .run()
    .await
}

fn load_rustls_config() -> rustls::ServerConfig {
    // init server config builder with safe defaults
    let config = ServerConfig::builder().with_no_client_auth();

    // load TLS key/cert files
    let cert_file = &mut io::BufReader::new(File::open("fullchain.pem").unwrap());
    let key_file = &mut io::BufReader::new(File::open("privkey.pem").unwrap());

    // convert files to key/cert objects
    let cert_chain = certs(cert_file).collect::<Result<Vec<_>, _>>().unwrap();
    let keys = pkcs8_private_keys(key_file)
        .flat_map(Result::ok)
        .next()
        .map(PrivateKeyDer::Pkcs8)
        .unwrap();

    config.with_single_cert(cert_chain, keys).unwrap()
}
