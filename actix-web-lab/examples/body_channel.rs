//! Demonstrates returning a response stream composed of byte chunks sent through a channel-like
//! interface. You can observe the effects using this cURL command:
//!
//! ```sh
//! curl --no-buffer localhost:8080/
//! ```

use std::io;

use actix_web::{App, HttpResponse, HttpServer, Responder, get, http::header::ContentType, web};
use actix_web_lab::body;
use tracing::info;

#[get("/")]
async fn index() -> impl Responder {
    let (mut body_tx, body) = body::channel::<io::Error>();

    // do not wait for this task to finish before sending response
    #[allow(clippy::let_underscore_future)]
    let _ = web::block(move || {
        body_tx.send(web::Bytes::from_static(b"body "))?;
        body_tx.send(web::Bytes::from_static(b"from "))?;

        // this is only acceptable due to being inside the `web::block` closure
        std::thread::sleep(std::time::Duration::from_millis(1000));

        body_tx.send(web::Bytes::from_static(b"another "))?;
        body_tx.send(web::Bytes::from_static(b"thread"))?;

        // options for closing the stream early:
        // body_tx.close(None)
        // body_tx.close(Some(io::Error::new(io::ErrorKind::Other, "it broke")))

        Ok::<_, web::Bytes>(())
    });

    HttpResponse::Ok()
        .insert_header(ContentType::plaintext())
        .message_body(body)
        .unwrap()
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    info!("staring server at http://localhost:8080");

    HttpServer::new(|| App::new().service(index))
        .workers(2)
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
