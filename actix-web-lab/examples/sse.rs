use std::{
    io,
    time::{Duration, SystemTime},
};

use actix_web::{get, App, HttpServer, Responder};
use actix_web_lab::sse::sse;
use tokio::time::sleep;
use tracing::info;

#[get("/")]
async fn index() -> impl Responder {
    "index"
}

#[get("/countdown")]
async fn countdown() -> impl Responder {
    let (sender, sse) = sse();

    actix_web::rt::spawn(async move {
        let mut n = 8;

        while n > 0 {
            if sender
                .data_with_event("countdown", n.to_string())
                .await
                .is_err()
            {
                tracing::warn!("client disconnected at {n}; could not send SSE message");
                break;
            }

            n -= 1;

            sleep(Duration::from_secs(1)).await;
        }
    });

    sse.with_retry_duration(Duration::from_secs(10))
}

#[get("/time")]
async fn timestamp() -> impl Responder {
    let (sender, sse) = sse();

    actix_web::rt::spawn(async move {
        loop {
            if sender
                .data_with_event("timestamp", format!("{:?}", SystemTime::now()))
                .await
                .is_err()
            {
                tracing::warn!("client disconnected; could not send SSE message");
                break;
            }

            sleep(Duration::from_secs(10)).await;
        }
    });

    sse.with_keep_alive(Duration::from_secs(3))
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind = ("127.0.0.1", 8080);
    info!("staring server at http://{}:{}", &bind.0, &bind.1);

    HttpServer::new(|| {
        App::new()
            .service(index)
            .service(countdown)
            .service(timestamp)
    })
    .workers(1)
    .bind(bind)?
    .run()
    .await
}
