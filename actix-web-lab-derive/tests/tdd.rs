use actix_web::{get, http::Method, web, App, HttpResponse, Responder};
use actix_web_lab_derive::FromRequest;

#[derive(Debug, FromRequest)]
struct RequestParts {
    method: Method,
    pool: web::Data<u32>,
}

#[get("/")]
async fn handler(parts: RequestParts) -> impl Responder {
    let RequestParts { method, pool, .. } = parts;
    let pool = **pool;

    if method == Method::GET && pool == 42 {
        HttpResponse::Ok()
    } else {
        eprintln!("method: {method} | pool: {pool}");
        HttpResponse::NotImplemented()
    }
}

#[actix_web::test]
async fn basic() {
    let srv = actix_test::start(|| App::new().app_data(web::Data::new(42u32)).service(handler));

    let req = srv.get("/");
    let res = req.send().await.unwrap();
    assert!(res.status().is_success());
}
