//! Serves an owned app with local state on one thread.
//!
//! This also works with `#[tokio::main]` on a multi-thread runtime.

use std::{cell::Cell, io, net::TcpListener};

use actix_web::{App, Responder, get, middleware::Logger};
use actix_web_lab::{extract::LocalData, server::LocalHttpServer};

#[get("/")]
async fn index(requests: LocalData<Cell<usize>>) -> impl Responder {
    requests.set(requests.get() + 1);

    format!("request {}\n", requests.get())
}

#[tokio::main(flavor = "local")]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let requests = LocalData::new(Cell::new(0usize));

    let app = App::new()
        .app_data(requests)
        .wrap(Logger::default())
        .service(index);

    let listener = TcpListener::bind(("127.0.0.1", 8080))?;
    tracing::info!("starting server at http://{}", listener.local_addr()?);

    LocalHttpServer::new(app).serve(listener).await
}
