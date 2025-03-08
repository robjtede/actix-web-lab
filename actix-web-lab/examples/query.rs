//! Demonstrates use of alternative Query extractor with better deserializer and errors.

use std::io;

use actix_web::{
    App, HttpResponse, HttpServer, Resource, Responder,
    error::ErrorBadRequest,
    middleware::{Logger, NormalizePath},
};
use actix_web_lab::extract::{Query, QueryDeserializeError};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
enum Type {
    Planet,
    Moon,
}

#[derive(Debug, Deserialize, Serialize)]
struct Params {
    /// Limit number of results.
    count: u32,

    /// Filter by object type.
    #[serde(rename = "type", default)]
    types: Vec<Type>,
}

/// Demonstrates multiple query parameters and getting path from deserialization errors.
async fn query(
    query: Result<Query<Params>, QueryDeserializeError>,
) -> actix_web::Result<impl Responder> {
    let params = match query {
        Ok(Query(query)) => query,
        Err(err) => return Err(ErrorBadRequest(err)),
    };

    tracing::debug!("filters: {params:?}");

    Ok(HttpResponse::Ok().json(params))
}

/// Baseline comparison using the built-in `Query` extractor.
async fn baseline(
    query: actix_web::Result<actix_web::web::Query<Params>>,
) -> actix_web::Result<impl Responder> {
    let params = query?.0;

    tracing::debug!("filters: {params:?}");

    Ok(HttpResponse::Ok().json(params))
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    tracing::info!("starting HTTP server at http://localhost:8080");

    HttpServer::new(|| {
        App::new()
            .service(Resource::new("/").get(query))
            .service(Resource::new("/baseline").get(baseline))
            .wrap(NormalizePath::trim())
            .wrap(Logger::default())
    })
    .bind(("127.0.0.1", 8080))?
    .workers(2)
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use actix_web::{App, body::to_bytes, dev::Service, http::StatusCode, test, web};

    use super::*;

    #[actix_web::test]
    async fn test_index() {
        let app =
            test::init_service(App::new().service(web::resource("/").route(web::post().to(query))))
                .await;

        let req = test::TestRequest::post()
            .uri("/?count=5&type=planet&type=moon")
            .to_request();

        let res = app.call(req).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        let body_bytes = to_bytes(res.into_body()).await.unwrap();
        assert_eq!(body_bytes, r#"{"count":5,"type":["planet","moon"]}"#);
    }
}
