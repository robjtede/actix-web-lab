//! Demonstrates using an `AsyncWrite` as a response body.

use std::{io, time::Duration};

use actix_web::{
    App, HttpResponse, HttpServer, Responder, get,
    http::{
        self,
        header::{ContentEncoding, ContentType},
    },
};
use actix_web_lab::body;
use async_zip::{ZipEntryBuilder, tokio::write::ZipFileWriter};
use tokio::{
    fs,
    io::{AsyncWrite, AsyncWriteExt as _},
};
use tokio_util::compat::TokioAsyncWriteCompatExt as _;

fn zip_to_io_err(err: async_zip::error::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

async fn read_dir<W>(zipper: &mut ZipFileWriter<W>) -> io::Result<()>
where
    W: AsyncWrite + Unpin,
{
    let mut path = fs::canonicalize(env!("CARGO_MANIFEST_DIR")).await?;
    path.push("examples");
    path.push("assets");

    tracing::info!("zipping {}", path.display());

    let mut dir = fs::read_dir(path).await?;

    while let Ok(Some(entry)) = dir.next_entry().await {
        if !entry.metadata().await.map(|m| m.is_file()).unwrap_or(false) {
            continue;
        }

        let mut file = match tokio::fs::OpenOptions::new()
            .read(true)
            .open(entry.path())
            .await
        {
            Ok(file) => file.compat_write(),
            Err(_) => continue, // we can't read the file
        };

        let filename = match entry.file_name().into_string() {
            Ok(filename) => filename,
            Err(_) => continue, // the file has a non UTF-8 name
        };

        let mut entry = zipper
            .write_entry_stream(ZipEntryBuilder::new(
                filename.into(),
                async_zip::Compression::Deflate,
            ))
            .await
            .map_err(zip_to_io_err)?;

        futures_util::io::copy(&mut file, &mut entry).await?;
        entry.close().await.map_err(zip_to_io_err)?;
    }

    Ok(())
}

#[get("/")]
async fn index() -> impl Responder {
    let (wrt, body) = body::writer();

    // allow response to be started while this is processing
    #[allow(clippy::let_underscore_future)]
    let _ = actix_web::rt::spawn(async move {
        let mut zipper = ZipFileWriter::new(wrt.compat_write());

        if let Err(err) = read_dir(&mut zipper).await {
            tracing::warn!("Failed to write files from directory to zip: {err}")
        }

        if let Err(err) = zipper.close().await {
            tracing::warn!("Failed to close zipper: {err}")
        }
    });

    HttpResponse::Ok()
        .append_header((
            http::header::CONTENT_DISPOSITION,
            r#"attachment; filename="folder.zip""#,
        ))
        .append_header(ContentEncoding::Identity)
        .append_header((http::header::CONTENT_TYPE, "application/zip"))
        .body(body)
}

#[get("/plain")]
async fn plaintext() -> impl Responder {
    let (mut wrt, body) = body::writer();

    // allow response to be started while this is processing
    #[allow(clippy::let_underscore_future)]
    let _ = tokio::spawn(async move {
        wrt.write_all(b"saying hello in\n").await?;

        wrt.write_all(b"3\n").await?;
        tokio::time::sleep(Duration::from_secs(1)).await;

        wrt.write_all(b"2\n").await?;
        tokio::time::sleep(Duration::from_secs(1)).await;

        wrt.write_all(b"1\n").await?;
        tokio::time::sleep(Duration::from_secs(1)).await;

        wrt.write_all(b"hello world\n").await
    });

    HttpResponse::Ok()
        .append_header(ContentType::plaintext())
        .body(body)
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    tracing::info!("staring server at http://localhost:8080");

    HttpServer::new(|| App::new().service(index).service(plaintext))
        .workers(2)
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
