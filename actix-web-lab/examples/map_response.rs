//! Shows a couple of ways to use the `from_fn` middleware.

use std::io;

use actix_web::{
    App, Error, HttpRequest, HttpResponse, HttpServer, body::MessageBody, dev::ServiceResponse,
    http::header, middleware::Logger, web,
};
use actix_web_lab::middleware::{map_response, map_response_body};
use tracing::info;

async fn add_res_header(
    mut res: ServiceResponse<impl MessageBody>,
) -> Result<ServiceResponse<impl MessageBody>, Error> {
    res.headers_mut()
        .insert(header::WARNING, header::HeaderValue::from_static("42"));

    Ok(res)
}

async fn mutate_body_type(
    _req: HttpRequest,
    _body: impl MessageBody + 'static,
) -> Result<impl MessageBody, Error> {
    Ok("foo".to_owned())
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    let bind = ("127.0.0.1", 8080);
    info!("staring server at http://{}:{}", &bind.0, &bind.1);

    HttpServer::new(|| {
        App::new()
            .service(
                web::resource("/foo")
                    .default_service(web::to(HttpResponse::Ok))
                    .wrap(Logger::default())
                    .wrap(map_response(add_res_header)),
            )
            .service(
                web::resource("/bar")
                    .default_service(web::to(HttpResponse::Ok))
                    .wrap(map_response_body(mutate_body_type))
                    .wrap(Logger::default()),
            )
            .default_service(web::to(HttpResponse::Ok))
    })
    .workers(1)
    .bind(bind)?
    .run()
    .await
}
