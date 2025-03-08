//! Demonstrates use of the Server-Sent Events (SSE) responder.

use std::{convert::Infallible, io, time::Duration};

use actix_web::{App, HttpRequest, HttpServer, Responder, get, middleware::Logger, web::Html};
use actix_web_lab::{extract::Path, sse};
use futures_util::stream;
use time::format_description::well_known::Rfc3339;
use tokio::time::sleep;

#[get("/")]
async fn index() -> impl Responder {
    Html::new(include_str!("./assets/sse.html").to_string())
}

/// Countdown event stream starting from 8.
#[get("/countdown")]
async fn countdown(req: HttpRequest) -> impl Responder {
    // note: a more production-ready implementation might want to use the lastEventId header
    // sent by the reconnecting browser after the _retry_ period
    tracing::debug!("lastEventId: {:?}", req.headers().get("Last-Event-ID"));

    common_countdown(8)
}

/// Countdown event stream with given starting number.
#[get("/countdown/{n:\\d+}")]
async fn countdown_from(Path(n): Path<u32>, req: HttpRequest) -> impl Responder {
    // note: a more production-ready implementation might want to use the lastEventId header
    // sent by the reconnecting browser after the _retry_ period
    tracing::debug!("lastEventId: {:?}", req.headers().get("Last-Event-ID"));

    common_countdown(n.try_into().unwrap())
}

fn common_countdown(n: i32) -> impl Responder {
    let countdown_stream = stream::unfold((false, n), |(started, n)| async move {
        // allow first countdown value to yield immediately
        if started {
            sleep(Duration::from_secs(1)).await;
        }

        if n > 0 {
            let data = sse::Data::new(n.to_string())
                .event("countdown")
                .id(n.to_string());

            Some((Ok::<_, Infallible>(sse::Event::Data(data)), (true, n - 1)))
        } else {
            None
        }
    });

    sse::Sse::from_stream(countdown_stream).with_retry_duration(Duration::from_secs(5))
}

#[get("/time")]
async fn timestamp() -> impl Responder {
    let (sender, receiver) = tokio::sync::mpsc::channel(2);

    actix_web::rt::spawn(async move {
        loop {
            let time = time::OffsetDateTime::now_utc();
            let msg = sse::Data::new(time.format(&Rfc3339).unwrap()).event("timestamp");

            if sender.send(msg.into()).await.is_err() {
                tracing::warn!("client disconnected; could not send SSE message");
                break;
            }

            sleep(Duration::from_secs(10)).await;
        }
    });

    sse::Sse::from_infallible_receiver(receiver).with_keep_alive(Duration::from_secs(3))
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    tracing::info!("starting HTTP server at http://localhost:8080");

    HttpServer::new(|| {
        App::new()
            .service(index)
            .service(countdown)
            .service(countdown_from)
            .service(timestamp)
            .wrap(Logger::default())
    })
    .workers(2)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
