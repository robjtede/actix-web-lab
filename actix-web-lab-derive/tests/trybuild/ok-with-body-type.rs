use std::collections::HashMap;

use actix_web::web;
use actix_web_lab_derive::FromRequest;

#[derive(Debug, FromRequest)]
struct RequestParts {
    pool: web::Data<u32>,
    form: web::Json<HashMap<String, String>>,
}

fn main() {}
