//! Demonstrates use of alternative JSON extractor with const-generic size limits.

use actix_web::{
    App, HttpRequest, HttpResponse, HttpServer, Responder,
    error::InternalError,
    middleware::{Logger, NormalizePath},
    web,
};
use actix_web_lab::extract::{Json, JsonPayloadError};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Debug, Serialize, Deserialize)]
struct MyObj {
    name: String,
    number: i32,
}

/// This handler uses the JSON extractor with the default size limit.
async fn index(
    res: Result<Json<MyObj>, JsonPayloadError>,
    req: HttpRequest,
) -> actix_web::Result<impl Responder> {
    let item = res.map_err(|err| json_error_handler(err, &req))?;
    tracing::debug!("model: {item:?}");

    Ok(HttpResponse::Ok().json(item.0))
}

/// This handler uses the JSON extractor with the default size limit.
async fn json_error(
    res: Result<Json<MyObj>, JsonPayloadError>,
) -> actix_web::Result<impl Responder> {
    let item = res.map_err(|err| {
        tracing::error!("failed to deserialize JSON: {err}");
        let res = HttpResponse::BadGateway().json(json!({
            "error": "invalid_json",
            "detail": err.to_string(),
        }));
        InternalError::from_response(err, res)
    })?;
    tracing::debug!("model: {item:?}");

    Ok(HttpResponse::Ok().json(item.0))
}

fn json_error_handler(err: JsonPayloadError, _req: &HttpRequest) -> actix_web::Error {
    tracing::error!(%err);

    let detail = err.to_string();
    let res = match &err {
        JsonPayloadError::ContentType => HttpResponse::UnsupportedMediaType().body(detail),
        JsonPayloadError::Deserialize { source: err, .. } if err.source().is_data() => {
            HttpResponse::UnprocessableEntity().body(detail)
        }
        _ => HttpResponse::BadRequest().body(detail),
    };

    InternalError::from_response(err, res).into()
}

/// This handler uses the JSON extractor with a 1KiB size limit.
async fn extract_item(item: Json<MyObj, 1024>, req: HttpRequest) -> HttpResponse {
    tracing::info!("model: {item:?}, req: {req:?}");
    HttpResponse::Ok().json(item.0)
}

/// This handler manually loads the request payload and parses the JSON data.
async fn index_manual(body: web::Bytes) -> actix_web::Result<HttpResponse> {
    // body is loaded, now we can deserialize using serde_json
    let obj = serde_json::from_slice::<MyObj>(&body)?;

    Ok(HttpResponse::Ok().json(obj))
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    tracing::info!("starting HTTP server at http://localhost:8080");

    HttpServer::new(|| {
        App::new()
            .service(web::resource("/extractor").route(web::post().to(index)))
            .service(web::resource("/extractor2").route(web::post().to(extract_item)))
            .service(web::resource("/extractor3").route(web::post().to(json_error)))
            .service(web::resource("/manual").route(web::post().to(index_manual)))
            .service(web::resource("/").route(web::post().to(index)))
            .wrap(NormalizePath::trim())
            .wrap(Logger::default())
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}

#[cfg(test)]
mod tests {
    use actix_web::{App, body::to_bytes, dev::Service, http, test, web};

    use super::*;

    #[actix_web::test]
    async fn test_index() {
        let app =
            test::init_service(App::new().service(web::resource("/").route(web::post().to(index))))
                .await;

        let req = test::TestRequest::post()
            .uri("/")
            .set_json(MyObj {
                name: "my-name".to_owned(),
                number: 43,
            })
            .to_request();
        let resp = app.call(req).await.unwrap();

        assert_eq!(resp.status(), http::StatusCode::OK);

        let body_bytes = to_bytes(resp.into_body()).await.unwrap();
        assert_eq!(body_bytes, r#"{"name":"my-name","number":43}"#);
    }
}
