use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::body::{BodySize, MessageBody};
use bytes::Bytes;
use tokio::sync::mpsc::{error::SendError, UnboundedReceiver, UnboundedSender};

/// Returns a sender half and a receiver half that can be used as a body type.
///
/// # Examples
/// ```
/// # use actix_web::{HttpResponse, web};
/// use actix_web_lab::body;
/// # async fn index() {
/// let (mut body_tx, body) = body::channel();
///
/// web::block(move || {
///     body_tx.send(web::Bytes::from_static(b"body from another thread")).unwrap();
/// })
/// .await
/// .unwrap();
///
/// HttpResponse::Ok().body(body)
/// # ;}
/// ```
pub fn channel() -> (Sender, impl MessageBody) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (Sender::new(tx), Receiver::new(rx))
}

/// A channel-like sender for body chunks.
#[derive(Debug, Clone)]
pub struct Sender {
    tx: UnboundedSender<Bytes>,
}

impl Sender {
    fn new(tx: UnboundedSender<Bytes>) -> Self {
        Self { tx }
    }

    /// Submit a chunk of bytes to the response body stream.
    pub fn send(&mut self, chunk: Bytes) -> Result<(), SendError<Bytes>> {
        self.tx.send(chunk)
    }
}

#[derive(Debug)]
struct Receiver {
    rx: UnboundedReceiver<Bytes>,
}

impl Receiver {
    fn new(rx: UnboundedReceiver<Bytes>) -> Self {
        Self { rx }
    }
}

impl MessageBody for Receiver {
    type Error = Infallible;

    fn size(&self) -> BodySize {
        BodySize::Stream
    }

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        self.rx.poll_recv(cx).map(|opt_bytes| opt_bytes.map(Ok))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static_assertions::assert_impl_all!(Sender: Send, Sync);
    static_assertions::assert_impl_all!(Receiver: Send, Sync, MessageBody);
}
