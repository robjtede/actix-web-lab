//! Demonstrates use of the MessagePack responder.

use std::io;

use actix_web::{App, HttpServer, Responder, get, http::StatusCode};
use actix_web_lab::respond::{MessagePack, MessagePackNamed};
use serde::Serialize;
use tracing::info;

#[derive(Debug, Serialize)]
struct Test {
    one: u32,
    two: String,
}

#[get("/")]
async fn index() -> impl Responder {
    MessagePack(Test {
        one: 42,
        two: "two".to_owned(),
    })
}

#[get("/named")]
async fn named() -> impl Responder {
    MessagePackNamed(Test {
        one: 42,
        two: "two".to_owned(),
    })
    .customize()
    .with_status(StatusCode::CREATED)
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind = ("127.0.0.1", 8080);
    info!("staring server at http://{}:{}", &bind.0, &bind.1);

    HttpServer::new(|| App::new().service(index).service(named))
        .workers(1)
        .bind(bind)?
        .run()
        .await
}
