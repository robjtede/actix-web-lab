use actix_web::{get, http, App, HttpResponse, HttpServer, Responder};
use async_zip::write::{EntryOptions, ZipFileWriter};
use futures_util::stream::TryStreamExt;
use std::io;
use tokio::{fs, io::AsyncWrite};
use tokio_util::codec;

fn zip_to_io_err(err: async_zip::error::ZipError) -> io::Error {
    io::Error::new(io::ErrorKind::Other, err)
}

async fn read_dir<W>(zipper: &mut ZipFileWriter<W>) -> Result<(), io::Error>
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
            Ok(file) => file,
            Err(_) => continue, // we can't read the file
        };

        let filename = match entry.file_name().into_string() {
            Ok(filename) => filename,
            Err(_) => continue, // the file has a non UTF-8 name
        };

        let mut entry = zipper
            .write_entry_stream(EntryOptions::new(filename, async_zip::Compression::Deflate))
            .await
            .map_err(zip_to_io_err)?;

        tokio::io::copy(&mut file, &mut entry).await?;
        entry.close().await.map_err(zip_to_io_err)?;
    }

    Ok(())
}

#[get("/")]
async fn index() -> impl Responder {
    let (to_write, to_read) = tokio::io::duplex(2048);

    tokio::spawn(async move {
        let mut zipper = async_zip::write::ZipFileWriter::new(to_write);

        if let Err(err) = read_dir(&mut zipper).await {
            tracing::warn!("Failed to write files from directory to zip: {err}")
        }

        if let Err(err) = zipper.close().await {
            tracing::warn!("Failed to close zipper: {err}")
        }
    });

    let stream = codec::FramedRead::new(to_read, codec::BytesCodec::new()).map_ok(|b| b.freeze());

    HttpResponse::Ok()
        .append_header((
            http::header::CONTENT_DISPOSITION,
            r#"attachment; filename="folder.zip""#,
        ))
        .append_header((http::header::CONTENT_ENCODING, "identity"))
        .append_header((http::header::CONTENT_TYPE, "application/zip"))
        .streaming(stream)
}

#[actix_web::main]
async fn main() -> io::Result<()> {
    env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));

    tracing::info!("staring server at http://localhost:8080");

    HttpServer::new(|| App::new().service(index))
        .workers(2)
        .bind(("127.0.0.1", 8080))?
        .run()
        .await
}
