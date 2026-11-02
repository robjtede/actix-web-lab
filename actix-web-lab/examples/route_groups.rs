//! Apply middleware to unrelated paths without an empty scope.
//!
//! Run with `cargo run -p actix-web-lab --example route_groups`.
//! `/dashboard` and `/users/7` return an `x-group` header; `/public` does not.

use actix_web::{App, HttpResponse, HttpServer, middleware::DefaultHeaders, web};
use actix_web_lab::web::group;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        App::new()
            .configure(
                group((
                    web::resource("/dashboard").to(|| async { "dashboard" }),
                    web::scope("/users").route(
                        "/{id}",
                        web::get().to(|id: web::Path<u32>| async move { format!("user {id}") }),
                    ),
                ))
                .wrap(DefaultHeaders::new().add(("x-group", "account")))
                .into_config(),
            )
            .route("/public", web::get().to(HttpResponse::Ok))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
