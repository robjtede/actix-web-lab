//! Demonstrates forking a request payload so that multiple extractors can derive data from a body.
//!
//! ```sh
//! curl -X POST localhost:8080/ -d 'foo'
//!
//! # or using HTTPie
//! http POST :8080/ --raw foo
//! ```

use std::io;

use actix_web::{App, FromRequest, HttpRequest, HttpServer, dev, middleware, web};
use actix_web_lab::util::fork_request_payload;
use futures_util::{TryFutureExt as _, future::LocalBoxFuture};
use tokio::try_join;
use tracing::info;

struct TwoBodies<T, U>(T, U);

impl<T, U> TwoBodies<T, U> {
    fn into_parts(self) -> (T, U) {
        (self.0, self.1)
    }
}

impl<T, U> FromRequest for TwoBodies<T, U>
where
    T: FromRequest,
    T::Future: 'static,
    U: FromRequest,
    U::Future: 'static,
{
    type Error = actix_web::Error;
    type Future = LocalBoxFuture<'static, Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, pl: &mut dev::Payload) -> Self::Future {
        let mut forked_pl = fork_request_payload(pl);

        let t_fut = T::from_request(req, pl);
        let u_fut = U::from_request(req, &mut forked_pl);

        Box::pin(async move {
            // .err_into to align error types to actix_web::Error
            let (t, u) = try_join!(t_fut.err_into(), u_fut.err_into())?;
            Ok(Self(t, u))
        })
    }
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    info!("staring server at http://localhost:8080");

    HttpServer::new(|| {
        App::new()
            .wrap(middleware::Logger::default().log_target("@"))
            .route(
                "/",
                web::post().to(|body: TwoBodies<String, web::Bytes>| async move {
                    let (string, bytes) = body.into_parts();

                    // proves that body was extracted twice since the bytes extracted are byte-equal to
                    // the string, without forking the request payload, the bytes parts would be empty
                    assert_eq!(string.as_bytes(), &bytes);

                    // echo string
                    string
                }),
            )
    })
    .workers(1)
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
