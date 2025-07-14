use std::{
    io,
    pin::Pin,
    task::{Context, Poll, ready},
};

use actix_web::body::{BodySize, MessageBody};
use bytes::Bytes;
use tokio::{
    io::AsyncWrite,
    sync::mpsc::{UnboundedReceiver, UnboundedSender},
};

/// Returns an `AsyncWrite` response body writer and its associated body type.
///
/// # Examples
/// ```
/// # use actix_web::{HttpResponse, web};
/// use actix_web_lab::body;
/// use tokio::io::AsyncWriteExt as _;
///
/// # async fn index() {
/// let (mut wrt, body) = body::writer();
///
/// let _ = tokio::spawn(async move { wrt.write_all(b"body from another thread").await });
///
/// HttpResponse::Ok().body(body)
/// # ;}
/// ```
pub fn writer() -> (Writer, impl MessageBody) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (Writer { tx }, BodyStream { rx })
}

/// An `AsyncWrite` response body writer.
#[derive(Debug, Clone)]
pub struct Writer {
    tx: UnboundedSender<Bytes>,
}

impl AsyncWrite for Writer {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<Result<usize, io::Error>> {
        self.tx
            .send(Bytes::copy_from_slice(buf))
            .map_err(io::Error::other)?;

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<(), io::Error>> {
        Poll::Ready(Ok(()))
    }
}

#[derive(Debug)]
struct BodyStream {
    rx: UnboundedReceiver<Bytes>,
}

impl MessageBody for BodyStream {
    type Error = io::Error;

    fn size(&self) -> BodySize {
        BodySize::Stream
    }

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        Poll::Ready(ready!(self.rx.poll_recv(cx)).map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static_assertions::assert_impl_all!(Writer: Send, Sync, Unpin);
    static_assertions::assert_impl_all!(BodyStream: Send, Sync, Unpin, MessageBody);
}
