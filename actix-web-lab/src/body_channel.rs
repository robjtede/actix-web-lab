use std::{
    pin::Pin,
    task::{Context, Poll},
};

use actix_web::body::{BodySize, MessageBody};
use bytes::Bytes;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, error::SendError};

use crate::BoxError;

/// Returns a sender half and a receiver half that can be used as a body type.
///
/// # Examples
/// ```
/// # use actix_web::{HttpResponse, web};
/// use std::convert::Infallible;
///
/// use actix_web_lab::body;
///
/// # async fn index() {
/// let (mut body_tx, body) = body::channel::<Infallible>();
///
/// let _ = web::block(move || {
///     body_tx
///         .send(web::Bytes::from_static(b"body from another thread"))
///         .unwrap();
/// });
///
/// HttpResponse::Ok().body(body)
/// # ;}
/// ```
pub fn channel<E: Into<BoxError>>() -> (Sender<E>, impl MessageBody) {
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    (Sender::new(tx), Receiver::new(rx))
}

/// A channel-like sender for body chunks.
#[derive(Debug, Clone)]
pub struct Sender<E> {
    tx: UnboundedSender<Result<Bytes, E>>,
}

impl<E> Sender<E> {
    fn new(tx: UnboundedSender<Result<Bytes, E>>) -> Self {
        Self { tx }
    }

    /// Submits a chunk of bytes to the response body stream.
    ///
    /// # Errors
    /// Errors if other side of channel body was dropped, returning `chunk`.
    pub fn send(&mut self, chunk: Bytes) -> Result<(), Bytes> {
        self.tx.send(Ok(chunk)).map_err(|SendError(err)| match err {
            Ok(chunk) => chunk,
            Err(_) => unreachable!(),
        })
    }

    /// Closes the stream, optionally sending an error.
    ///
    /// # Errors
    /// Errors if closing with error and other side of channel body was dropped, returning `error`.
    pub fn close(self, error: Option<E>) -> Result<(), E> {
        if let Some(err) = error {
            return self.tx.send(Err(err)).map_err(|SendError(err)| match err {
                Ok(_) => unreachable!(),
                Err(err) => err,
            });
        }

        Ok(())
    }
}

#[derive(Debug)]
struct Receiver<E> {
    rx: UnboundedReceiver<Result<Bytes, E>>,
}

impl<E> Receiver<E> {
    fn new(rx: UnboundedReceiver<Result<Bytes, E>>) -> Self {
        Self { rx }
    }
}

impl<E> MessageBody for Receiver<E>
where
    E: Into<BoxError>,
{
    type Error = E;

    fn size(&self) -> BodySize {
        BodySize::Stream
    }

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Bytes, Self::Error>>> {
        self.rx.poll_recv(cx)
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    static_assertions::assert_impl_all!(Sender<io::Error>: Send, Sync, Unpin);
    static_assertions::assert_impl_all!(Receiver<io::Error>: Send, Sync, Unpin, MessageBody);
}
