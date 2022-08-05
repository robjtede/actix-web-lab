use std::{io, time::Duration};

use actix_web::{get, App, HttpServer, Responder};
use actix_web_lab::sse::sse;
use tracing::info;

#[get("/")]
async fn index() -> impl Responder {
    "index"
}

#[get("/sse")]
async fn events() -> impl Responder {
    let (sender, sse) = sse();

    let _ = sender.comment("long comment\ninnit").await;
    let _ = sender.data("long data\ninnit").await;

    sse.with_retry_duration(Duration::from_secs(10))
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind = ("127.0.0.1", 8080);
    info!("staring server at http://{}:{}", &bind.0, &bind.1);

    HttpServer::new(|| App::new().service(index).service(events))
        .workers(1)
        .bind(bind)?
        .run()
        .await
}
