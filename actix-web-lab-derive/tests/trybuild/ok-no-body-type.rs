use actix_web::{http, web};
use actix_web_lab_derive::FromRequest;

#[derive(Debug, FromRequest)]
struct RequestParts {
    method: http::Method,
    pool: web::Data<u32>,
}

fn main() {}
