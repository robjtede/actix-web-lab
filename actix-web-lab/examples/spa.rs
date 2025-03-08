//! Simple builder for a SPA (Single Page Application) service builder.

use std::io;

use actix_web::{App, HttpServer, middleware::Logger, web};
use actix_web_lab::web::spa;
use tracing::info;

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind = ("127.0.0.1", 8080);
    info!("staring server at http://{}:{}", &bind.0, &bind.1);

    HttpServer::new(|| {
        App::new()
            .wrap(Logger::default().log_target("@"))
            .route(
                "/api/greet",
                web::to(|| async {
                    if rand::random() {
                        "Hello World!"
                    } else {
                        "Greetings, World!"
                    }
                }),
            )
            .service(
                spa()
                    .index_file("./examples/assets/spa.html")
                    .static_resources_mount("/static")
                    .static_resources_location("./examples/assets")
                    .finish(),
            )
    })
    .workers(1)
    .bind(bind)?
    .run()
    .await
}
