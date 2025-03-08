#![allow(missing_docs)]

use actix_web::{
    App, HttpResponse, Responder,
    http::{Method, StatusCode},
    web,
};
use actix_web_lab_derive::FromRequest;

#[derive(Debug, FromRequest)]
struct RequestParts {
    method: Method,
    pool: web::Data<u32>,
    body: String,
    body2: String,

    #[from_request(copy_from_app_data)]
    copied_data: u64,
}

async fn handler(parts: RequestParts) -> impl Responder {
    let RequestParts {
        method,
        pool,
        body,
        body2,
        copied_data,
        ..
    } = parts;

    let pool = **pool;

    assert_eq!(copied_data, 43);

    assert_eq!(body, "foo");

    // assert that body is taken and second attempt to do so will be blank
    assert_eq!(body2, "");

    if method == Method::POST && pool == 42 {
        HttpResponse::Ok()
    } else {
        eprintln!("method: {method} | pool: {pool}");
        HttpResponse::NotImplemented()
    }
}

#[actix_web::test]
async fn tdd() {
    let srv = actix_test::start(|| {
        App::new()
            .app_data(43u64)
            .app_data(web::Data::new(42u32))
            .default_service(web::to(handler))
    });

    let res = srv.post("/").send_body("foo").await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
