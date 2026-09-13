//! Actix Web with live Rustls credential rotation.

use actix_web::{App, HttpServer, web};

#[tokio::main]
async fn main() -> Result<(), hot_reload_rustls::Error> {
    let (config, mut watcher) = hot_reload_rustls::Builder::new("cert.pem", "key.pem").build()?;
    let observer = watcher.spawn_observer(|event| eprintln!("{event:?}"))?;

    let result = async move {
        HttpServer::new(|| {
            App::new().route(
                "/",
                web::get().to(|| async { "TLS credentials can rotate\n" }),
            )
        })
        .bind_rustls_0_23(("127.0.0.1", 8443), config)?
        .run()
        .await
    }
    .await;

    drop(watcher);
    observer.join().expect("observer thread panicked");

    Ok(result?)
}
