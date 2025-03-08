//! How to use `NdJson` as an efficient streaming response type.
//!
//! The same techniques can also be used for `Csv`.
//!
//! Select the number of NDJSON items to return using the query string. Example: `/users?n=100`.
//!
//! Also includes a low-efficiency route to demonstrate the difference.

use std::io::{self, Write as _};

use actix_web::{
    App, HttpResponse, HttpServer, Responder, get,
    web::{self, BufMut as _, BytesMut},
};
use actix_web_lab::respond::NdJson;
use futures_core::Stream;
use futures_util::{StreamExt as _, stream};
use rand::{
    Rng as _,
    distr::{Alphanumeric, SampleString as _},
};
use serde::Deserialize;
use serde_json::json;
use tracing::info;

fn streaming_data_source(n: u32) -> impl Stream<Item = Result<serde_json::Value, io::Error>> {
    stream::repeat_with(|| {
        Ok(json!({
            "email": random_email(),
            "address": random_address(),
        }))
    })
    .take(n as usize)
}

#[derive(Debug, Deserialize)]
struct Opts {
    n: Option<u32>,
}

/// This handler streams data as NDJSON to the client in a fast and memory efficient way.
///
/// A real data source might be a downstream server, database query, or other external resource.
#[get("/users")]
async fn get_user_list(opts: web::Query<Opts>) -> impl Responder {
    let n_items = opts.n.unwrap_or(10);
    let data_stream = streaming_data_source(n_items);

    NdJson::new(data_stream)
        .into_responder()
        .customize()
        .insert_header(("num-results", n_items))

    // alternative if you need more control of the HttpResponse:
    //
    // HttpResponse::Ok()
    //     .insert_header(("content-type", NdJson::mime()))
    //     .insert_header(("num-results", n_items))
    //     .body(NdJson::new(data_stream).into_body_stream())
}

/// A comparison route that loads all the data into memory before sending it to the client.
///
/// If you provide a high number in the query string like `?n=300000` you should be able to observe
/// increasing memory usage of the process in your process monitor.
#[get("/users-high-mem")]
async fn get_high_mem_user_list(opts: web::Query<Opts>) -> impl Responder {
    let n_items = opts.n.unwrap_or(10);
    let mut stream = streaming_data_source(n_items);

    // buffer all data from the source into a Bytes container
    let mut buf = BytesMut::new().writer();
    while let Some(Ok(item)) = stream.next().await {
        serde_json::to_writer(&mut buf, &item).unwrap();
        buf.write_all(b"\n").unwrap();
    }

    HttpResponse::Ok()
        .insert_header(("content-type", NdJson::mime()))
        .insert_header(("num-results", n_items))
        .body(buf.into_inner())
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind = ("127.0.0.1", 8080);
    info!("staring server at http://{}:{}", &bind.0, &bind.1);

    HttpServer::new(|| {
        App::new()
            .service(get_user_list)
            .service(get_high_mem_user_list)
    })
    .workers(1)
    .bind(bind)?
    .run()
    .await
}

fn random_email() -> String {
    let id = Alphanumeric.sample_string(&mut rand::rng(), 10);
    format!("user_{id}@example.com")
}

fn random_address() -> String {
    let street_no: u16 = rand::rng().random_range(10..=99);
    format!("{street_no} Random Street")
}
