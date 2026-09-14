//! Actix Web with live Rustls credential rotation.

use actix_web::{App, HttpServer, web};

#[tokio::main]
async fn main() -> eyre::Result<()> {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .map_err(|_| eyre::eyre!("Crypto provider already installed"))?;

    let (config, mut watcher) = hot_reload_rustls::Builder::new("cert.pem", "key.pem").build()?;

    let observer = watcher.spawn_observer(|event| eprintln!("{event:?}"))?;

    let result = HttpServer::new(|| {
        App::new().route(
            "/",
            web::get().to(|| async { "TLS credentials can rotate\n" }),
        )
    })
    .bind_rustls_0_23(("127.0.0.1", 8443), config)?
    .run()
    .await;

    drop(watcher);
    observer.join().expect("observer thread panicked");

    Ok(result?)
}
