use std::io;

use actix_web::{
    get,
    http::header::ContentType,
    web::{self},
    App, HttpResponse, HttpServer, Responder,
};
use actix_web_lab::body;
use tracing::info;

#[get("/")]
async fn index() -> impl Responder {
    let (mut body_tx, body) = body::channel();

    web::block(move || {
        body_tx.send(web::Bytes::from_static(b"body "))?;
        body_tx.send(web::Bytes::from_static(b"from "))?;
        body_tx.send(web::Bytes::from_static(b"another "))?;
        body_tx.send(web::Bytes::from_static(b"thread"))
    })
    .await
    .unwrap()
    .unwrap();

    HttpResponse::Ok()
        .insert_header(ContentType::plaintext())
        .body(body)
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind = ("127.0.0.1", 8080);
    info!("staring server at http://{}:{}", &bind.0, &bind.1);

    HttpServer::new(|| App::new().service(index))
        .workers(1)
        .bind(bind)?
        .run()
        .await
}
