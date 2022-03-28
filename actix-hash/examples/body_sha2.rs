//! Body + checksum hash extractor usage.
//!
//! For example, sending an empty body will return the hash starting with "E3":
//! ```sh
//! $ curl -XPOST localhost:8080
//! [E3, B0, C4, 42, 98, FC, 1C, ...
//! ```

use std::io;

use actix_hash::BodySha256;
use actix_web::{middleware::Logger, web, App, HttpServer};
use tracing::info;

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    info!("staring server at http://localhost:8080");

    HttpServer::new(|| {
        App::new().wrap(Logger::default().log_target("@")).route(
            "/",
            web::post().to(|body: BodySha256<String>| async move { format!("{:X?}", body.hash()) }),
        )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
