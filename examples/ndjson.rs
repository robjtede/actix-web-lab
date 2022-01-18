//! How to use `NdJson` as an efficient streaming response type.
//!
//! The same techniques can also be used for `Csv`.
//!
//! Select the number of NDJSON items to return using the query string. Example: `/users?n=100`.
//!
//! Also includes a low-efficiency route to demonstrate the difference.

use std::io::{self, Write as _};

use actix_web::{
    get,
    web::{self, BufMut as _, BytesMut},
    App, HttpResponse, HttpServer, Responder,
};
use actix_web_lab::respond::NdJson;
use futures_core::Stream;
use futures_util::{stream, StreamExt as _};
use rand::{distributions::Alphanumeric, Rng as _};
use serde::Deserialize;
use serde_json::json;

fn streaming_data_source(n: u32) -> impl Stream<Item = serde_json::Value> {
    stream::repeat_with(|| {
        json!({
            "email": random_email(),
            "address": random_address(),
        })
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
    while let Some(item) = stream.next().await {
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
    log::info!("staring server at http://{}:{}", &bind.0, &bind.1);

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
    let rng = rand::thread_rng();

    let id: String = rng
        .sample_iter(Alphanumeric)
        .take(10)
        .map(char::from)
        .collect();

    format!("user_{}@example.com", id)
}

fn random_address() -> String {
    let mut rng = rand::thread_rng();
    let street_no: u16 = rng.gen_range(10..99);
    format!("{} Random Street", street_no)
}
