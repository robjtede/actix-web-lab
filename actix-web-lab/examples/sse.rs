use std::{io, time::Duration};

use actix_web::{get, middleware::Logger, App, HttpServer, Responder};
use actix_web_lab::{extract::Path, respond::Html, sse::sse};
use time::format_description::well_known::Rfc3339;
use tokio::time::sleep;
use tracing::info;

#[get("/")]
async fn index() -> impl Responder {
    Html(include_str!("./assets/sse.html").to_string())
}

#[get("/countdown")]
async fn countdown() -> impl Responder {
    common_countdown(8)
}

#[get("/countdown/{n:\\d+}")]
async fn countdown_from(Path((n,)): Path<(u32,)>) -> impl Responder {
    common_countdown(n.try_into().unwrap())
}

fn common_countdown(n: i32) -> impl Responder {
    let (sender, sse) = sse();

    actix_web::rt::spawn(async move {
        let mut n = n;

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
            let time = time::OffsetDateTime::now_utc();

            if sender
                .data_with_event("timestamp", time.format(&Rfc3339).unwrap())
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
            .service(countdown_from)
            .service(timestamp)
            .wrap(Logger::default())
    })
    .workers(1)
    .bind(bind)?
    .run()
    .await
}
